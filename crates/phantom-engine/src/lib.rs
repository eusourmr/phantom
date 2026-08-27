//! Phantom web engine orchestration boundary.
//!
//! The engine owns immutable snapshots of DOM, computed styles, cold layout,
//! and renderer-neutral paint commands. Networking and native-window
//! responsibilities remain outside this crate.

#![forbid(unsafe_code)]

use phantom_dom::Document;
use phantom_html::HtmlError;

pub use phantom_css::{
    BorderStyle, BoxSizing, ComputedStyle, Display, EdgeSizes, FontFamily, FontStyle, FontWeight,
    Length, Rgba, StyleMap,
};
pub use phantom_layout::{
    LayoutBox, LayoutError, LayoutId, LayoutKind, LayoutSnapshot, Rect, build_layout_snapshot,
    build_layout_snapshot_with_shaper,
};
pub use phantom_paint::{
    PaintColor, PaintCommand, PaintError, PaintFontFamily, PaintFontStyle, PaintFontWeight,
    PaintList, PaintRect, PaintTextRange, build_paint_list,
};

use thiserror::Error;

const DEFAULT_LAYOUT_VIEWPORT_WIDTH: f32 = 1024.0;

/// High-level lifecycle state of a Phantom engine instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineState {
    /// The engine is initialized and waiting for a document.
    Idle,

    /// DOM, style, layout, and paint snapshots are available.
    Ready,
}

/// Errors produced while loading content into the engine.
#[derive(Debug, Error)]
pub enum EngineError {
    /// HTML parsing or DOM construction failed.
    #[error("HTML processing failed: {0}")]
    Html(#[from] HtmlError),

    /// Cold layout snapshot construction failed.
    #[error("layout processing failed: {0}")]
    Layout(#[from] LayoutError),

    /// Renderer-neutral paint-list generation failed.
    #[error("paint processing failed: {0}")]
    Paint(#[from] PaintError),
}

/// High-level Phantom engine orchestration shell.
///
/// The engine owns processing snapshots but does not own networking,
/// operating-system windows, or browser chrome.
#[derive(Debug)]
pub struct Engine {
    document: Document,
    styles: StyleMap,
    layout: LayoutSnapshot,
    paint: PaintList,
    state: EngineState,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// Creates a new idle Phantom engine.
    #[must_use]
    pub fn new() -> Self {
        Self {
            document: Document::new(),
            styles: StyleMap::default(),
            layout: LayoutSnapshot::empty(DEFAULT_LAYOUT_VIEWPORT_WIDTH),
            paint: PaintList::empty(DEFAULT_LAYOUT_VIEWPORT_WIDTH),
            state: EngineState::Idle,
        }
    }

    /// Returns the current engine lifecycle state.
    #[must_use]
    pub const fn state(&self) -> EngineState {
        self.state
    }

    /// Returns the active DOM snapshot.
    #[must_use]
    pub const fn document(&self) -> &Document {
        &self.document
    }

    /// Returns the active computed-style snapshot.
    #[must_use]
    pub const fn styles(&self) -> &StyleMap {
        &self.styles
    }

    /// Returns the active cold layout snapshot.
    #[must_use]
    pub const fn layout(&self) -> &LayoutSnapshot {
        &self.layout
    }

    /// Returns the active renderer-neutral paint list.
    #[must_use]
    pub const fn paint_list(&self) -> &PaintList {
        &self.paint
    }

    /// Parses HTML and replaces all engine snapshots.
    ///
    /// The active pipeline is:
    ///
    /// `HTML -> DOM -> Computed Style -> Cold Layout -> Paint List`
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if parsing, layout, or paint generation fails.
    pub fn load_html(&mut self, source: &str) -> Result<(), EngineError> {
        self.load_html_with_viewport(source, DEFAULT_LAYOUT_VIEWPORT_WIDTH)
    }

    /// Parses HTML using an explicit layout viewport width.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if parsing, layout, or paint generation fails.
    pub fn load_html_with_viewport(
        &mut self,
        source: &str,
        viewport_width: f32,
    ) -> Result<(), EngineError> {
        let document = phantom_html::parse(source)?;
        let styles = phantom_css::compute_styles(&document);
        let layout = build_layout_snapshot(&document, &styles, viewport_width)?;
        let paint = build_paint_list(&layout, &styles)?;

        self.document = document;
        self.styles = styles;
        self.layout = layout;
        self.paint = paint;
        self.state = EngineState::Ready;

        Ok(())
    }

    /// Recalculates geometry and paint commands for a changed viewport width.
    ///
    /// DOM parsing and CSS cascade are deliberately not repeated.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if layout or paint generation fails.
    pub fn relayout(&mut self, viewport_width: f32) -> Result<(), EngineError> {
        let layout = build_layout_snapshot(&self.document, &self.styles, viewport_width)?;

        let paint = build_paint_list(&layout, &self.styles)?;

        self.layout = layout;
        self.paint = paint;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use phantom_dom::NodeKind;

    use super::{Engine, EngineError, EngineState, LayoutKind, PaintCommand, Rgba};

    #[test]
    fn new_engine_is_idle() {
        let engine = Engine::new();

        assert_eq!(engine.state(), EngineState::Idle);
        assert_eq!(engine.document().len(), 1);
        assert!(engine.styles().is_empty());
        assert!(engine.layout().is_empty());
        assert!(engine.paint_list().is_empty());
    }

    #[test]
    fn html_generates_layout_and_paint_snapshots() -> Result<(), EngineError> {
        let mut engine = Engine::new();

        engine.load_html("<html><body><h1>Phantom</h1><p>Independent browser</p></body></html>")?;

        assert_eq!(engine.state(), EngineState::Ready);
        assert!(!engine.layout().is_empty());
        assert!(!engine.paint_list().is_empty());

        let contains_text_box = engine
            .layout()
            .boxes()
            .iter()
            .any(|layout_box| matches!(layout_box.kind(), LayoutKind::Text { .. }));

        assert!(contains_text_box);

        let contains_text_paint = engine
            .paint_list()
            .commands()
            .iter()
            .any(|command| matches!(command, PaintCommand::Text { .. }));

        assert!(contains_text_paint);

        Ok(())
    }

    #[test]
    fn css_color_reaches_paint_without_dom_rendering() -> Result<(), EngineError> {
        let mut engine = Engine::new();

        engine.load_html(
            "<style>#hero { color: #12ab34; }</style>\
             <p id=\"hero\">Styled by Phantom</p>",
        )?;

        let paragraph_id = engine
            .document()
            .nodes()
            .find_map(|node| match node.kind() {
                NodeKind::Element(element)
                    if element.tag_name() == "p" && element.attribute("id") == Some("hero") =>
                {
                    Some(node.id())
                }
                _ => None,
            });

        let color = paragraph_id
            .and_then(|node_id| engine.styles().get(node_id))
            .map(|style| style.color());

        assert_eq!(color, Some(Rgba::new(18, 171, 52, 255)));

        let contains_green_text = engine.paint_list().commands().iter().any(|command| {
            matches!(
                command,
                PaintCommand::Text { color, .. }
                    if color.red() == 18
                        && color.green() == 171
                        && color.blue() == 52
            )
        });

        assert!(contains_green_text);

        Ok(())
    }

    #[test]
    fn relayout_regenerates_paint_without_reparsing() -> Result<(), EngineError> {
        let mut engine = Engine::new();

        engine.load_html_with_viewport(
            "<div style=\"width: 50%; background: #112233\">Phantom</div>",
            1000.0,
        )?;

        let initial_width = engine
            .layout()
            .boxes()
            .iter()
            .find(|layout_box| matches!(layout_box.kind(), LayoutKind::Block))
            .map(|layout_box| layout_box.rect().width())
            .unwrap_or_default();

        engine.relayout(600.0)?;

        let relayout_width = engine
            .layout()
            .boxes()
            .iter()
            .find(|layout_box| matches!(layout_box.kind(), LayoutKind::Block))
            .map(|layout_box| layout_box.rect().width())
            .unwrap_or_default();

        assert!((initial_width - 500.0).abs() < f32::EPSILON);
        assert!((relayout_width - 300.0).abs() < f32::EPSILON);
        assert!(!engine.paint_list().is_empty());

        Ok(())
    }
}
