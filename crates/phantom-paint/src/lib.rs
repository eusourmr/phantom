//! DOM-independent paint-list generation for Phantom.
//!
//! This crate is the wall between geometry and the eventual GPU compositor.
//! Its input is an immutable [`phantom_layout::LayoutSnapshot`] plus the
//! immutable computed-style snapshot. It never reads or depends on the DOM.
//!
//! Inline layout now provides positioned text fragments and explicit line
//! boxes. Paint consumes text-fragment geometry directly and does not perform
//! wrapping or text-flow decisions.

#![forbid(unsafe_code)]

use phantom_css::{FontFamily, FontStyle, FontWeight, ObjectFit, ObjectPosition, Rgba, StyleMap};
use phantom_image::ImageResourceId;
use phantom_layout::{LayoutBox, LayoutKind, LayoutSnapshot, Rect as LayoutRect};
use thiserror::Error;

/// Renderer-neutral RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaintColor {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl PaintColor {
    /// Creates a renderer-neutral RGBA color.
    #[must_use]
    pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// Returns the red channel.
    #[must_use]
    pub const fn red(self) -> u8 {
        self.red
    }

    /// Returns the green channel.
    #[must_use]
    pub const fn green(self) -> u8 {
        self.green
    }

    /// Returns the blue channel.
    #[must_use]
    pub const fn blue(self) -> u8 {
        self.blue
    }

    /// Returns the alpha channel.
    #[must_use]
    pub const fn alpha(self) -> u8 {
        self.alpha
    }
}

impl From<Rgba> for PaintColor {
    fn from(color: Rgba) -> Self {
        Self::new(color.red(), color.green(), color.blue(), color.alpha())
    }
}

/// Renderer-neutral physical rectangle.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PaintRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl PaintRect {
    /// Creates a paint rectangle in logical pixels.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns the horizontal origin.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    /// Returns the vertical origin.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }

    /// Returns the width.
    #[must_use]
    pub const fn width(self) -> f32 {
        self.width
    }

    /// Returns the height.
    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }
}

impl From<LayoutRect> for PaintRect {
    fn from(rect: LayoutRect) -> Self {
        Self::new(rect.x(), rect.y(), rect.width(), rect.height())
    }
}

/// Compact UTF-8 range inside one [`PaintList`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaintTextRange {
    start: u32,
    len: u32,
}

impl PaintTextRange {
    /// Returns the byte offset in the shared paint text buffer.
    #[must_use]
    pub const fn start(self) -> u32 {
        self.start
    }

    /// Returns the UTF-8 byte length.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.len
    }

    /// Returns `true` when the range is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// Coarse font family encoded into a paint command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintFontFamily {
    /// Sans-serif text.
    SansSerif,

    /// Monospace text.
    Monospace,
}

impl From<FontFamily> for PaintFontFamily {
    fn from(family: FontFamily) -> Self {
        match family {
            FontFamily::SansSerif => Self::SansSerif,
            FontFamily::Monospace => Self::Monospace,
        }
    }
}

/// Font weight encoded into a paint command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintFontWeight {
    /// Normal weight.
    Normal,

    /// Bold weight.
    Bold,
}

impl From<FontWeight> for PaintFontWeight {
    fn from(weight: FontWeight) -> Self {
        match weight {
            FontWeight::Normal => Self::Normal,
            FontWeight::Bold => Self::Bold,
        }
    }
}

/// Font posture encoded into a paint command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintFontStyle {
    /// Upright text.
    Normal,

    /// Italic or oblique text.
    Italic,
}

impl From<FontStyle> for PaintFontStyle {
    fn from(style: FontStyle) -> Self {
        match style {
            FontStyle::Normal => Self::Normal,
            FontStyle::Italic => Self::Italic,
        }
    }
}

/// One immutable, renderer-neutral paint command.
#[derive(Debug, Clone, PartialEq)]
pub enum PaintCommand {
    /// Fills one rectangular background.
    FillRect {
        /// Rectangle to fill.
        rect: PaintRect,

        /// Fill color.
        color: PaintColor,
    },

    /// Paints one replaced image resource inside its content box.
    Image {
        /// Geometry of the image content box.
        rect: PaintRect,

        /// Opaque resource identifier resolved by the renderer/resource layer.
        resource: ImageResourceId,

        /// Optional alternative text stored in the shared paint text buffer.
        alt: Option<PaintTextRange>,

        /// CSS object-fit value resolved by the style system.
        fit: ObjectFit,

        /// CSS object-position value resolved by the style system.
        position: ObjectPosition,
    },

    /// Paints one laid-out UTF-8 text fragment.
    Text {
        /// Geometry produced by inline layout.
        rect: PaintRect,

        /// Text range inside the shared UTF-8 buffer.
        text: PaintTextRange,

        /// Foreground color.
        color: PaintColor,

        /// Font size in logical pixels.
        font_size: f32,

        /// Coarse font weight.
        weight: PaintFontWeight,

        /// Font posture.
        style: PaintFontStyle,

        /// Coarse font family.
        family: PaintFontFamily,

        /// Whether the fragment should be underlined.
        underline: bool,
    },
}

/// Immutable renderer-neutral paint output.
///
/// Commands and text storage are contiguous. A renderer does not need access
/// to DOM nodes, CSS source, HTML source, or line-wrapping logic.
#[derive(Debug, Clone, Default)]
pub struct PaintList {
    commands: Vec<PaintCommand>,
    text: String,
    viewport_width: f32,
    content_height: f32,
}

impl PaintList {
    /// Creates an empty paint list for one viewport width.
    #[must_use]
    pub fn empty(viewport_width: f32) -> Self {
        Self {
            commands: Vec::new(),
            text: String::new(),
            viewport_width: viewport_width.max(1.0),
            content_height: 0.0,
        }
    }

    /// Returns all commands in paint order.
    #[must_use]
    pub fn commands(&self) -> &[PaintCommand] {
        &self.commands
    }

    /// Returns the number of paint commands.
    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Returns `true` if no paint commands exist.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Returns the shared UTF-8 text-buffer size.
    #[must_use]
    pub fn text_bytes(&self) -> usize {
        self.text.len()
    }

    /// Returns text addressed by one compact range.
    #[must_use]
    pub fn text(&self, range: PaintTextRange) -> Option<&str> {
        let start = usize::try_from(range.start).ok()?;
        let len = usize::try_from(range.len).ok()?;
        let end = start.checked_add(len)?;

        self.text.get(start..end)
    }

    /// Returns the layout viewport width represented by this list.
    #[must_use]
    pub const fn viewport_width(&self) -> f32 {
        self.viewport_width
    }

    /// Returns the document's laid-out vertical extent.
    #[must_use]
    pub const fn content_height(&self) -> f32 {
        self.content_height
    }
}

/// Errors raised while creating a compact paint list.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum PaintError {
    /// The shared UTF-8 buffer can no longer use compact ranges.
    #[error("paint text-buffer capacity exceeded")]
    TextCapacityExceeded,
}

/// Converts cold layout geometry into a DOM-independent paint list.
///
/// Line boxes are structural and do not generate paint commands. Text
/// fragments already contain their final line geometry.
///
/// # Errors
///
/// Returns [`PaintError::TextCapacityExceeded`] if a compact UTF-8 range can
/// no longer address the shared paint text buffer.
pub fn build_paint_list(
    layout: &LayoutSnapshot,
    styles: &StyleMap,
) -> Result<PaintList, PaintError> {
    let mut paint = PaintList {
        commands: Vec::with_capacity(layout.len()),
        text: String::with_capacity(layout.text_bytes()),
        viewport_width: layout.viewport_width(),
        content_height: layout.content_height(),
    };

    for layout_box in layout.boxes() {
        match layout_box.kind() {
            LayoutKind::Root | LayoutKind::Line => {}

            LayoutKind::Block | LayoutKind::Flex => {
                push_background(&mut paint, layout_box, styles);

                push_border(&mut paint, layout_box, styles);
            }

            LayoutKind::Image { resource, .. } => {
                push_background(&mut paint, layout_box, styles);

                push_border(&mut paint, layout_box, styles);

                push_image(&mut paint, layout, layout_box, styles, resource)?;
            }

            LayoutKind::Text { underline, .. } => {
                push_text(&mut paint, layout, layout_box, styles, underline)?;
            }
        }
    }

    Ok(paint)
}

fn push_background(paint: &mut PaintList, layout_box: &LayoutBox, styles: &StyleMap) {
    let Some(style) = styles.get(layout_box.source_node()) else {
        return;
    };

    let Some(background) = style.background_color() else {
        return;
    };

    if background.alpha() == 0 {
        return;
    }

    let rect = layout_box.rect();

    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    paint.commands.push(PaintCommand::FillRect {
        rect: rect.into(),
        color: background.into(),
    });
}

fn push_border(paint: &mut PaintList, layout_box: &LayoutBox, styles: &StyleMap) {
    let Some(style) = styles.get(layout_box.source_node()) else {
        return;
    };

    let border = layout_box.border();

    if border.top() <= 0.0
        && border.right() <= 0.0
        && border.bottom() <= 0.0
        && border.left() <= 0.0
    {
        return;
    }

    let rect = layout_box.rect();
    let color = style.border_color().into();

    if border.top() > 0.0 {
        paint.commands.push(PaintCommand::FillRect {
            rect: PaintRect::new(rect.x(), rect.y(), rect.width(), border.top()),
            color,
        });
    }

    let vertical_start = rect.y() + border.top();

    let vertical_height = (rect.height() - border.top() - border.bottom()).max(0.0);

    if border.left() > 0.0 && vertical_height > 0.0 {
        paint.commands.push(PaintCommand::FillRect {
            rect: PaintRect::new(rect.x(), vertical_start, border.left(), vertical_height),
            color,
        });
    }

    if border.right() > 0.0 && vertical_height > 0.0 {
        paint.commands.push(PaintCommand::FillRect {
            rect: PaintRect::new(
                rect.x() + rect.width() - border.right(),
                vertical_start,
                border.right(),
                vertical_height,
            ),
            color,
        });
    }

    if border.bottom() > 0.0 {
        paint.commands.push(PaintCommand::FillRect {
            rect: PaintRect::new(
                rect.x(),
                rect.y() + rect.height() - border.bottom(),
                rect.width(),
                border.bottom(),
            ),
            color,
        });
    }
}

fn push_image(
    paint: &mut PaintList,
    layout: &LayoutSnapshot,
    layout_box: &LayoutBox,
    styles: &StyleMap,
    resource: ImageResourceId,
) -> Result<(), PaintError> {
    let rect = layout_box.rect();
    let border = layout_box.border();
    let padding = layout_box.padding();

    let x = rect.x() + border.left() + padding.left();

    let y = rect.y() + border.top() + padding.top();

    let width =
        (rect.width() - border.left() - border.right() - padding.left() - padding.right()).max(0.0);

    let height =
        (rect.height() - border.top() - border.bottom() - padding.top() - padding.bottom())
            .max(0.0);

    let alt = if let Some(alt) = layout.image_alt_for(layout_box)
        && !alt.is_empty()
    {
        let start =
            u32::try_from(paint.text.len()).map_err(|_| PaintError::TextCapacityExceeded)?;

        let len = u32::try_from(alt.len()).map_err(|_| PaintError::TextCapacityExceeded)?;

        paint.text.push_str(alt);

        Some(PaintTextRange { start, len })
    } else {
        None
    };

    paint.commands.push(PaintCommand::Image {
        rect: PaintRect::new(x, y, width, height),
        resource,
        alt,
        fit: styles
            .get(layout_box.source_node())
            .map_or(ObjectFit::Fill, |style| style.object_fit()),
        position: styles
            .get(layout_box.source_node())
            .map_or_else(ObjectPosition::default, |style| style.object_position()),
    });

    Ok(())
}

fn push_text(
    paint: &mut PaintList,
    layout: &LayoutSnapshot,
    layout_box: &LayoutBox,
    styles: &StyleMap,
    underline: bool,
) -> Result<(), PaintError> {
    let Some(text) = layout.text_for(layout_box) else {
        return Ok(());
    };

    if text.is_empty() {
        return Ok(());
    }

    let start = u32::try_from(paint.text.len()).map_err(|_| PaintError::TextCapacityExceeded)?;

    let len = u32::try_from(text.len()).map_err(|_| PaintError::TextCapacityExceeded)?;

    paint.text.push_str(text);

    let style = styles
        .get(layout_box.source_node())
        .cloned()
        .unwrap_or_default();

    paint.commands.push(PaintCommand::Text {
        rect: layout_box.rect().into(),
        text: PaintTextRange { start, len },
        color: style.color().into(),
        font_size: style.font_size(),
        weight: style.font_weight().into(),
        style: style.font_style().into(),
        family: style.font_family().into(),
        underline,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use phantom_css::Rgba;

    use super::{PaintColor, PaintList, PaintTextRange};

    #[test]
    fn converts_css_color_without_renderer_dependency() {
        let color = PaintColor::from(Rgba::new(18, 52, 86, 200));

        assert_eq!(color.red(), 18);
        assert_eq!(color.green(), 52);
        assert_eq!(color.blue(), 86);
        assert_eq!(color.alpha(), 200);
    }

    #[test]
    fn empty_paint_list_has_no_commands() {
        let paint = PaintList::empty(800.0);

        assert!(paint.is_empty());
        assert_eq!(paint.viewport_width(), 800.0);
    }

    #[test]
    fn invalid_text_range_is_rejected() {
        let paint = PaintList::empty(800.0);

        let range = PaintTextRange { start: 1, len: 4 };

        assert_eq!(paint.text(range), None);
    }
}
