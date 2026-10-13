//! Phantom web engine orchestration boundary.
//!
//! The engine owns immutable snapshots of DOM, computed styles, cold layout,
//! and renderer-neutral paint commands. Networking and native-window
//! responsibilities remain outside this crate.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

pub use phantom_dom::NodeKind;
use phantom_dom::{Document, DomError, ElementData, NodeId};
use phantom_html::{HtmlError, tree_builder::TreeBuildReport};

// PHANTOM_2H2_BOX_VISUAL_FIDELITY_I
pub use phantom_css::{
    AlignContent, AlignItems, AlignSelf, AutoEdges, BorderStyle, BoxSizing, ComputedStyle, Display,
    EdgeSizes, FlexDirection, FlexWrap, FontFamily, FontStyle, FontWeight, JustifyContent, Length,
    ObjectFit, ObjectPosition, Overflow, Rgba, StyleMap, ZIndex,
};
pub use phantom_image::{
    DecodedImage, ImageCatalog, ImageDecodeLimits, ImageDecoder, ImageError, ImageFormat,
    ImageMetadata, ImageResourceId, IntrinsicSize, probe_image,
};
pub use phantom_layout::{
    ControlKind, LayoutBox, LayoutError, LayoutId, LayoutKind, LayoutSnapshot, Rect,
    build_layout_snapshot, build_layout_snapshot_with_images,
    build_layout_snapshot_with_images_and_viewport, build_layout_snapshot_with_shaper,
    build_layout_snapshot_with_shaper_and_images,
    build_layout_snapshot_with_shaper_and_images_and_viewport, build_layout_snapshot_with_viewport,
};
pub use phantom_paint::{
    PaintAttachment, PaintColor, PaintCommand, PaintError, PaintFontFamily, PaintFontStyle,
    PaintFontWeight, PaintList, PaintRect, PaintTextRange, build_paint_list,
};
// PHANTOM_2J2_LIVE_DOM_BRIDGE_I
pub use phantom_js::{
    BrowserDomElementSnapshot, BrowserDomMutation, BrowserHostSnapshot, FetchCredentialsMode,
    FetchHostRequest, RuntimeError as JavaScriptError, Value as JavaScriptValue,
};

use thiserror::Error;

const DEFAULT_LAYOUT_VIEWPORT_WIDTH: f32 = 1024.0;
const DEFAULT_LAYOUT_VIEWPORT_HEIGHT: f32 = 768.0;
// PHANTOM_2N2_EXTERNAL_STYLESHEETS_I
const MAX_EXTERNAL_STYLESHEETS_PER_DOCUMENT: usize = 32;
const MAX_EXTERNAL_STYLESHEET_SOURCE_BYTES: usize = 1024 * 1024;

// PHANTOM_2H4_RESPONSIVE_VIEWPORT_I

// PHANTOM_2G13_VIEWPORT_GEOMETRY_FOUNDATION

// PHANTOM_2C14_FRAGMENT_TARGET
/// One visible same-document fragment target resolved from the active snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct FragmentTarget {
    id: String,
    top: f32,
}

impl FragmentTarget {
    /// Returns the matched HTML `id`.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the target's top edge in document coordinates.
    #[must_use]
    pub const fn top(&self) -> f32 {
        self.top
    }
}
// PHANTOM_2C13_FORM_NAVIGATION_I
/// Stable document-generation-local identifier for one form control.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FormControlId(u64);

impl FormControlId {
    /// Returns the underlying DOM identifier value.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Stable document-generation-local identifier for one `<form>`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FormId(u64);

impl FormId {
    /// Returns the underlying DOM identifier value.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Browser-control role supported by Form Navigation I.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormControlKind {
    /// `<input type=text>` or omitted type.
    Text,
    /// `<input type=search>`.
    Search,
    /// `<input type=submit>` or a submit `<button>`.
    Submit,
}

/// One visible form-control region derived from the cold layout snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct FormControlRegion {
    id: FormControlId,
    form: FormId,
    kind: FormControlKind,
    rect: Rect,
    name: Option<String>,
    initial_value: String,
    placeholder: String,
    label: String,
    enabled: bool,
}

impl FormControlRegion {
    /// Returns the control identifier.
    #[must_use]
    pub const fn id(&self) -> FormControlId {
        self.id
    }

    /// Returns the owning form.
    #[must_use]
    pub const fn form(&self) -> FormId {
        self.form
    }

    /// Returns the supported control role.
    #[must_use]
    pub const fn kind(&self) -> FormControlKind {
        self.kind
    }

    /// Returns the document-coordinate widget rectangle.
    #[must_use]
    pub const fn rect(&self) -> Rect {
        self.rect
    }

    /// Returns the HTML control name when declared.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the initial HTML value.
    #[must_use]
    pub fn initial_value(&self) -> &str {
        &self.initial_value
    }

    /// Returns the placeholder text for editable controls.
    #[must_use]
    pub fn placeholder(&self) -> &str {
        &self.placeholder
    }

    /// Returns the browser-visible control label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns whether the HTML `disabled` attribute is absent.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

/// Deterministic GET form submission prepared by the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormGetSubmission {
    action: String,
    fields: Vec<(String, String)>,
}

impl FormGetSubmission {
    /// Returns the raw form action reference. Empty means the current document.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    /// Returns successful name/value pairs in DOM order.
    #[must_use]
    pub fn fields(&self) -> &[(String, String)] {
        &self.fields
    }
}

/// Errors produced while preparing the bounded Form Navigation I subset.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FormSubmissionError {
    /// The requested form does not exist in the active document.
    #[error("form not found in active document")]
    FormNotFound,

    /// The form requests a method not implemented by Form Navigation I.
    #[error("unsupported form method: {0}")]
    UnsupportedMethod(String),
}
// PHANTOM_2C12_LINK_NAVIGATION_I
/// One clickable hyperlink region in the active cold layout snapshot.
///
/// URL resolution deliberately remains outside the engine. `href` is preserved
/// exactly as declared so browser navigation can resolve it against the
/// committed document URL through `phantom-net`.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkRegion {
    node_id: u64,
    href: String,
    rect: Rect,
    opens_new_context: bool,
}

impl LinkRegion {
    // PHANTOM_2M24_NATIVE_INTERACTION_JAVASCRIPT_EVENTS_I
    /// Returns the owning anchor's stable document-generation-local node id.
    #[must_use]
    pub const fn node_id(&self) -> u64 {
        self.node_id
    }

    /// Returns the raw `href` value declared by the nearest ancestor anchor.
    #[must_use]
    pub fn href(&self) -> &str {
        &self.href
    }

    /// Returns the clickable rectangle in document coordinates.
    #[must_use]
    pub const fn rect(&self) -> Rect {
        self.rect
    }

    /// Returns whether HTML `target="_blank"` requested a new browsing context.
    ///
    /// Named browsing contexts other than `_blank` are intentionally outside
    /// this first navigation slice.
    #[must_use]
    pub const fn opens_new_context(&self) -> bool {
        self.opens_new_context
    }
}
/// One image subresource request discovered in the active document snapshot.
///
/// The request contains only the opaque engine resource identifier and the raw
/// HTML source reference. URL resolution, fetching and decoding remain outside
/// the engine.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageRequest {
    resource: ImageResourceId,
    source: String,
    loading: ImageLoading,
    priority: ResourcePriority,
    top: f32,
    bottom: f32,
}

/// Fetch-timing hint parsed from an image element's `loading` attribute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImageLoading {
    /// Fetch without waiting for proximity to the visual viewport.
    #[default]
    Eager,

    /// Defer fetching until the image approaches the visual viewport.
    Lazy,
}

/// Fetch priority hint exposed by HTML `fetchpriority`.
///
/// Ordering is intentional: `High < Auto < Low`, allowing normal Rust sorting
/// to place higher-priority work first without numeric magic constants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourcePriority {
    /// User-agent scheduling should prefer this resource over normal work.
    High,

    /// Use the browser's normal scheduling policy.
    #[default]
    Auto,

    /// User-agent scheduling may defer this resource behind normal work.
    Low,
}

/// One `<link rel="preload" as="image">` hint discovered in the document.
#[derive(Clone, Debug, PartialEq)]
pub struct ImagePreloadRequest {
    source: String,
    priority: ResourcePriority,
}

// PHANTOM_2C11_SITE_IDENTITY_I
/// One site icon explicitly declared by the active document.
///
/// Site Identity I intentionally does not synthesize `/favicon.ico`. The
/// browser fetches only a document-declared `<link rel="icon" href="...">`
/// whose MIME type is compatible with Phantom's current raster decoder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteIconRequest {
    source: String,
}
impl ImageRequest {
    /// Returns the opaque resource identifier used by layout and paint.
    #[must_use]
    pub const fn resource(&self) -> ImageResourceId {
        self.resource
    }

    /// Returns the raw image source reference from the document.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the element's normalized loading policy.
    #[must_use]
    pub const fn loading(&self) -> ImageLoading {
        self.loading
    }

    /// Returns the normalized `fetchpriority` scheduling hint.
    #[must_use]
    pub const fn priority(&self) -> ResourcePriority {
        self.priority
    }

    /// Returns the image border-box top in document coordinates.
    #[must_use]
    pub const fn top(&self) -> f32 {
        self.top
    }

    /// Returns the image border-box bottom in document coordinates.
    #[must_use]
    pub const fn bottom(&self) -> f32 {
        self.bottom
    }
}

impl ImagePreloadRequest {
    /// Returns the raw preload source reference from the document.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the normalized preload `fetchpriority` hint.
    #[must_use]
    pub const fn priority(&self) -> ResourcePriority {
        self.priority
    }
}

impl SiteIconRequest {
    /// Returns the raw icon URL reference declared by the document.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}
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

    /// A bounded live-DOM mutation failed.
    #[error("DOM mutation failed: {0}")]
    Dom(#[from] DomError),

    /// An external stylesheet exceeded the bounded author-source budget.
    #[error("external stylesheet exceeds configured limit ({limit} bytes)")]
    ExternalStylesheetTooLarge {
        /// Maximum stylesheet source bytes accepted by this engine slice.
        limit: usize,
    },

    /// The browser attempted to install CSS for a node that is not an eligible stylesheet link.
    #[error("external stylesheet target is not eligible: node {0}")]
    ExternalStylesheetTargetInvalid(u64),

    /// JavaScript runtime or browser-host execution failed.
    #[error("JavaScript execution failed: {0}")]
    JavaScript(#[from] JavaScriptError),
}

// PHANTOM_2N2_EXTERNAL_STYLESHEETS_I
fn stylesheet_link_is_eligible(element: &ElementData) -> bool {
    if element.tag_name() != "link" || element.attribute("disabled").is_some() {
        return false;
    }

    if !rel_has_token(element.attribute("rel"), "stylesheet")
        || rel_has_token(element.attribute("rel"), "alternate")
    {
        return false;
    }

    if !element.attribute("type").is_none_or(|value| {
        value.trim().is_empty() || value.trim().eq_ignore_ascii_case("text/css")
    }) {
        return false;
    }

    if !element.attribute("media").is_none_or(|value| {
        let value = value.trim();
        value.is_empty()
            || value.eq_ignore_ascii_case("all")
            || value.eq_ignore_ascii_case("screen")
    }) {
        return false;
    }

    element
        .attribute("href")
        .is_some_and(|href| !href.trim().is_empty())
}

// PHANTOM_2N1_REAL_WEB_NETWORK_I
fn is_supported_classic_script_type(kind: Option<&str>) -> bool {
    kind.is_none_or(|kind| {
        let kind = kind.trim();
        kind.is_empty()
            || kind.eq_ignore_ascii_case("text/javascript")
            || kind.eq_ignore_ascii_case("application/javascript")
            || kind.eq_ignore_ascii_case("text/ecmascript")
            || kind.eq_ignore_ascii_case("application/ecmascript")
            || kind.eq_ignore_ascii_case("application/x-javascript")
    })
}

// PHANTOM_2N2_EXTERNAL_STYLESHEETS_I
/// One external author stylesheet discovered in parsed document order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalStylesheetRequest {
    node_id: u64,
    source: String,
}

impl ExternalStylesheetRequest {
    /// Stable document-generation-local DOM node id of the owning `<link>`.
    #[must_use]
    pub const fn node_id(&self) -> u64 {
        self.node_id
    }

    /// Raw stylesheet URL reference declared by `href`.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

// PHANTOM_2L5_ORDERED_PAGE_SCRIPTS_I
/// One eligible classic script captured from the parsed document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PageScriptTask {
    /// Inline JavaScript source.
    Inline(String),
    /// External JavaScript URL reference, resolved by the browser host.
    External(String),
}

/// Read-only counters for a Phantom document rendering snapshot.
///
/// A discovered stylesheet is not necessarily downloaded or installed.
/// These counters deliberately avoid claiming CSS standards coverage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderDiagnostics {
    /// Number of nodes in the active DOM snapshot.
    pub dom_nodes: usize,
    /// Number of layout boxes in the active snapshot.
    pub layout_boxes: usize,
    /// Number of paint commands generated for the snapshot.
    pub paint_commands: usize,
    /// Eligible stylesheet links discovered in the current document.
    pub external_css_discovered: usize,
    /// External stylesheets installed in the active engine generation.
    pub external_css_installed: usize,
}

/// High-level Phantom engine orchestration shell.
///
/// The engine owns processing snapshots but does not own networking,
/// operating-system windows, or browser chrome.
#[derive(Debug)]
pub struct Engine {
    document: Document,
    html_build_report: Option<TreeBuildReport>,
    styles: StyleMap,
    external_stylesheets: BTreeMap<NodeId, String>,
    images: ImageCatalog,
    layout: LayoutSnapshot,
    paint: PaintList,
    links: Vec<LinkRegion>,
    javascript: phantom_js::Runtime,
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
            html_build_report: None,
            styles: StyleMap::default(),
            external_stylesheets: BTreeMap::new(),
            images: ImageCatalog::default(),
            layout: LayoutSnapshot::empty_with_viewport(
                DEFAULT_LAYOUT_VIEWPORT_WIDTH,
                DEFAULT_LAYOUT_VIEWPORT_HEIGHT,
            ),
            paint: PaintList::empty(DEFAULT_LAYOUT_VIEWPORT_WIDTH),
            links: Vec::new(),
            javascript: phantom_js::Runtime::new(),
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

    /// Returns the bounded report produced by the parse that built the active DOM.
    #[must_use]
    pub const fn html_build_report(&self) -> Option<&TreeBuildReport> {
        self.html_build_report.as_ref()
    }

    /// Returns the active computed-style snapshot.
    #[must_use]
    pub const fn styles(&self) -> &StyleMap {
        &self.styles
    }

    /// Returns image metadata registered for the active document generation.
    #[must_use]
    pub const fn images(&self) -> &ImageCatalog {
        &self.images
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

    /// Returns passive rendering diagnostics without reloading or mutating the page.
    ///
    /// These counters help distinguish missing stylesheet installation from
    /// later CSS/layout/paint problems during browser-host investigations.
    #[must_use]
    pub fn render_diagnostics(&self) -> RenderDiagnostics {
        RenderDiagnostics {
            dom_nodes: self.document.len(),
            layout_boxes: self.layout.boxes().len(),
            paint_commands: self.paint.commands().len(),
            external_css_discovered: self.external_stylesheet_requests().len(),
            external_css_installed: self.external_stylesheets.len(),
        }
    }

    /// Resolves a visible HTML `id` to its first laid-out document position.
    ///
    /// Fragment decoding beyond the serialized first-slice identifier is
    /// deliberately deferred to the standards-conformance phase.
    #[must_use]
    pub fn fragment_target(&self, fragment: &str) -> Option<FragmentTarget> {
        if fragment.is_empty() {
            return None;
        }

        let target_node = self.document.nodes().find_map(|node| {
            let NodeKind::Element(element) = node.kind() else {
                return None;
            };

            (element.attribute("id") == Some(fragment)).then_some(node.id())
        })?;

        let top = self
            .layout
            .boxes()
            .iter()
            .filter(|layout_box| {
                node_is_descendant_or_self(&self.document, layout_box.source_node(), target_node)
            })
            .map(|layout_box| layout_box.rect().y())
            .min_by(f32::total_cmp)?;

        Some(FragmentTarget {
            id: fragment.to_owned(),
            top,
        })
    }
    /// Returns visible controls supported by Browser Inputs I.
    ///
    /// Hidden inputs participate in submission but deliberately have no visual
    /// region. Unsupported control types remain outside this first slice.
    #[must_use]
    pub fn form_control_regions(&self) -> Vec<FormControlRegion> {
        self.layout
            .boxes()
            .iter()
            .filter_map(|layout_box| {
                let LayoutKind::Control { kind } = layout_box.kind() else {
                    return None;
                };

                let node_id = layout_box.source_node();
                let element = element_for(&self.document, node_id)?;
                let form_node = form_for_node(&self.document, node_id)?;
                let form = FormId(form_node.as_u64());

                let kind = match kind {
                    ControlKind::TextInput => FormControlKind::Text,
                    ControlKind::SearchInput => FormControlKind::Search,
                    ControlKind::SubmitButton => FormControlKind::Submit,
                };

                let initial_value = element.attribute("value").unwrap_or_default().to_owned();
                let placeholder = element
                    .attribute("placeholder")
                    .unwrap_or_default()
                    .to_owned();

                Some(FormControlRegion {
                    id: FormControlId(node_id.as_u64()),
                    form,
                    kind,
                    rect: layout_box.rect(),
                    name: element.attribute("name").map(str::to_owned),
                    initial_value,
                    placeholder,
                    label: form_control_label(&self.document, node_id, element, kind),
                    enabled: element.attribute("disabled").is_none(),
                })
            })
            .collect()
    }

    /// Builds a deterministic GET submission for one form.
    ///
    /// The current-value map is owned by the browser shell; the immutable DOM is
    /// not mutated merely because a user edits an input.
    ///
    /// # Errors
    ///
    /// Returns [`FormSubmissionError::FormNotFound`] for an unknown form or
    /// [`FormSubmissionError::UnsupportedMethod`] for non-GET forms.
    pub fn build_get_form_submission(
        &self,
        form: FormId,
        submitter: Option<FormControlId>,
        values: &BTreeMap<FormControlId, String>,
    ) -> Result<FormGetSubmission, FormSubmissionError> {
        let form_node = self
            .document
            .nodes()
            .find(|node| node.id().as_u64() == form.as_u64())
            .filter(|node| {
                matches!(
                    node.kind(),
                    NodeKind::Element(element) if element.tag_name() == "form"
                )
            })
            .ok_or(FormSubmissionError::FormNotFound)?;

        let NodeKind::Element(form_element) = form_node.kind() else {
            return Err(FormSubmissionError::FormNotFound);
        };

        let method = form_element
            .attribute("method")
            .unwrap_or("get")
            .trim()
            .to_ascii_lowercase();

        if !method.is_empty() && method != "get" {
            return Err(FormSubmissionError::UnsupportedMethod(method));
        }

        let action = form_element
            .attribute("action")
            .unwrap_or_default()
            .to_owned();

        let mut fields = Vec::new();

        for node in self.document.nodes() {
            let NodeKind::Element(element) = node.kind() else {
                continue;
            };

            if form_for_node(&self.document, node.id()) != Some(form_node.id()) {
                continue;
            }

            if element.attribute("disabled").is_some() {
                continue;
            }

            let Some(name) = element.attribute("name").filter(|name| !name.is_empty()) else {
                continue;
            };

            let control_id = FormControlId(node.id().as_u64());

            match element.tag_name() {
                "input" => {
                    let input_type = normalized_input_type(element);

                    match input_type.as_str() {
                        "" | "text" | "search" | "hidden" => {
                            let value = values.get(&control_id).cloned().unwrap_or_else(|| {
                                element.attribute("value").unwrap_or_default().to_owned()
                            });

                            fields.push((name.to_owned(), value));
                        }

                        "submit" if submitter == Some(control_id) => {
                            fields.push((
                                name.to_owned(),
                                element.attribute("value").unwrap_or_default().to_owned(),
                            ));
                        }

                        _ => {}
                    }
                }

                "button" if button_is_submit(element) && submitter == Some(control_id) => {
                    fields.push((
                        name.to_owned(),
                        element.attribute("value").unwrap_or_default().to_owned(),
                    ));
                }

                _ => {}
            }
        }

        Ok(FormGetSubmission { action, fields })
    }
    /// Returns clickable hyperlink regions for the active layout snapshot.
    ///
    /// Regions are generated only for visible text/image layout fragments that
    /// descend from an `<a href>` element. This avoids making non-rendered DOM
    /// nodes interactive.
    #[must_use]
    pub fn link_regions(&self) -> &[LinkRegion] {
        &self.links
    }

    /// Hit-tests one document-coordinate point against hyperlink fragments.
    ///
    /// Later paint-order fragments win when boxes overlap.
    #[must_use]
    pub fn link_at(&self, x: f32, y: f32) -> Option<&LinkRegion> {
        self.links
            .iter()
            .rev()
            .find(|region| rect_contains(region.rect(), x, y))
    }

    // PHANTOM_2M24_NATIVE_INTERACTION_JAVASCRIPT_EVENTS_I
    /// Returns the current visible link fragment for one anchor node id.
    ///
    /// This is intentionally consulted after JavaScript click dispatch so
    /// listener mutations to `href`/`target` affect the native default action.
    #[must_use]
    pub fn link_for_node(&self, node_id: u64) -> Option<&LinkRegion> {
        self.links.iter().find(|region| region.node_id() == node_id)
    }

    /// Hit-tests the active layout and resolves the nearest DOM element.
    ///
    /// Later layout fragments win. Text fragments are promoted to their
    /// nearest element ancestor so native pointer input never exposes text
    /// node internals to the browser shell.
    #[must_use]
    pub fn dom_element_at(&self, x: f32, y: f32) -> Option<u64> {
        self.layout
            .boxes()
            .iter()
            .rev()
            .find(|layout_box| rect_contains(layout_box.rect(), x, y))
            .and_then(|layout_box| nearest_element_node(&self.document, layout_box.source_node()))
            .map(NodeId::as_u64)
    }
    /// Returns responsive image subresources for a default device-pixel ratio.
    ///
    /// This keeps URL fetching outside the engine while making HTML candidate
    /// selection an engine responsibility, where DOM semantics belong.
    #[must_use]
    pub fn image_requests(&self) -> Vec<ImageRequest> {
        self.image_requests_for_device(1.0)
    }

    /// Returns responsive image requests selected for the supplied device-pixel ratio.
    ///
    /// The current standards slice supports `src`, density and width `srcset`
    /// descriptors, a bounded `sizes` subset, and the first matching `<source>`
    /// in a `<picture>` parent. Unsupported media expressions simply do not match.
    #[must_use]
    pub fn image_requests_for_device(&self, device_pixel_ratio: f32) -> Vec<ImageRequest> {
        let viewport_width = self.layout.viewport_width().max(1.0);
        let dpr = device_pixel_ratio.max(0.1);

        self.layout
            .boxes()
            .iter()
            .filter_map(|layout_box| {
                let LayoutKind::Image { resource, .. } = layout_box.kind() else {
                    return None;
                };

                let node_id = layout_box.source_node();
                let source = select_image_source(&self.document, node_id, viewport_width, dpr)?;
                let element = element_for(&self.document, node_id);
                let loading = element
                    .and_then(|element| element.attribute("loading"))
                    .filter(|value| value.eq_ignore_ascii_case("lazy"))
                    .map_or(ImageLoading::Eager, |_| ImageLoading::Lazy);
                let priority = element.map_or(ResourcePriority::Auto, |element| {
                    resource_priority(element.attribute("fetchpriority"))
                });
                let rect = layout_box.rect();

                Some(ImageRequest {
                    resource,
                    source,
                    loading,
                    priority,
                    top: rect.y(),
                    bottom: rect.bottom(),
                })
            })
            .collect()
    }

    /// Returns image preload hints for a default device-pixel ratio.
    #[must_use]
    pub fn image_preload_requests(&self) -> Vec<ImagePreloadRequest> {
        self.image_preload_requests_for_device(1.0)
    }

    /// Returns `<link rel="preload" as="image">` requests selected for the
    /// supplied device-pixel ratio.
    ///
    /// This initial standards slice supports `href`, `imagesrcset`,
    /// `imagesizes`, simple `media`, supported image `type`, and
    /// `fetchpriority`. Fetching remains outside the engine.
    #[must_use]
    pub fn image_preload_requests_for_device(
        &self,
        device_pixel_ratio: f32,
    ) -> Vec<ImagePreloadRequest> {
        let viewport_width = self.layout.viewport_width().max(1.0);
        let dpr = device_pixel_ratio.max(0.1);

        self.document
            .nodes()
            .filter_map(|node| {
                let NodeKind::Element(element) = node.kind() else {
                    return None;
                };

                preload_image_from_element(element, viewport_width, dpr)
            })
            .collect()
    }

    // PHANTOM_2C15_SITE_IDENTITY_II
    /// Returns supported document-declared site icons in DOM declaration order.
    ///
    /// Missing MIME type remains eligible because the bounded decoder is
    /// authoritative after fetch. Explicitly unsupported MIME types are skipped.
    #[must_use]
    pub fn site_icon_requests(&self) -> Vec<SiteIconRequest> {
        self.document
            .nodes()
            .filter_map(|node| {
                let NodeKind::Element(element) = node.kind() else {
                    return None;
                };

                site_icon_from_element(element)
            })
            .collect()
    }

    /// Returns the first supported declared site icon.
    ///
    /// Kept as a compatibility convenience for callers that only need one
    /// candidate. Browser Site Identity II uses [`Self::site_icon_requests`].
    #[must_use]
    pub fn site_icon_request(&self) -> Option<SiteIconRequest> {
        self.site_icon_requests().into_iter().next()
    }

    /// Returns the normalized text of the first HTML `<title>` element.
    #[must_use]
    pub fn document_title(&self) -> Option<String> {
        let title_node = self.document.nodes().find_map(|node| {
            let NodeKind::Element(element) = node.kind() else {
                return None;
            };

            (element.tag_name() == "title").then_some(node.id())
        })?;

        let raw = descendant_text_content(&self.document, title_node);
        let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");

        (!normalized.is_empty()).then_some(normalized)
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
        self.load_html_with_viewport_size(
            source,
            DEFAULT_LAYOUT_VIEWPORT_WIDTH,
            DEFAULT_LAYOUT_VIEWPORT_HEIGHT,
        )
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
        self.load_html_with_viewport_size(source, viewport_width, DEFAULT_LAYOUT_VIEWPORT_HEIGHT)
    }

    /// Parses HTML using explicit two-dimensional layout viewport geometry.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if parsing, layout, or paint generation fails.
    pub fn load_html_with_viewport_size(
        &mut self,
        source: &str,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Result<(), EngineError> {
        let (document, html_build_report) = phantom_html::parse_with_report(source)?;
        let styles =
            phantom_css::compute_styles_with_viewport(&document, viewport_width, viewport_height);
        let images = ImageCatalog::default();
        let layout = build_layout_snapshot_with_images_and_viewport(
            &document,
            &styles,
            viewport_width,
            viewport_height,
            &images,
        )?;
        let paint = build_paint_list(&layout, &styles)?;
        let links = build_link_regions(&document, &layout);

        self.document = document;
        self.html_build_report = Some(html_build_report);
        self.styles = styles;
        self.external_stylesheets.clear();
        self.images = images;
        self.layout = layout;
        self.paint = paint;
        self.links = links;
        self.javascript = phantom_js::Runtime::new();
        self.state = EngineState::Ready;

        Ok(())
    }

    // PHANTOM_2N2_EXTERNAL_STYLESHEETS_I
    /// Returns eligible external author stylesheets in parsed document order.
    ///
    /// This slice supports ordinary non-alternate screen stylesheets. Dynamic
    /// stylesheet discovery after JavaScript mutation remains outside 2N-2.
    #[must_use]
    pub fn external_stylesheet_requests(&self) -> Vec<ExternalStylesheetRequest> {
        self.document
            .nodes()
            .filter_map(|node| {
                let NodeKind::Element(element) = node.kind() else {
                    return None;
                };
                if !stylesheet_link_is_eligible(element) {
                    return None;
                }
                let source = element.attribute("href")?.trim();
                Some(ExternalStylesheetRequest {
                    node_id: node.id().as_u64(),
                    source: source.to_owned(),
                })
            })
            .take(MAX_EXTERNAL_STYLESHEETS_PER_DOCUMENT)
            .collect()
    }

    /// Installs one bounded external author stylesheet and rebuilds style,
    /// layout, paint and link snapshots without reparsing HTML.
    ///
    /// The stylesheet remains associated with the owning `<link>` NodeId so
    /// `phantom-css` can preserve source order against inline `<style>` blocks.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] when the source is oversized, the target link
    /// is no longer eligible, or downstream layout/paint rebuilding fails.
    pub fn install_external_stylesheet(
        &mut self,
        node_id: u64,
        source: &str,
    ) -> Result<(), EngineError> {
        if source.len() > MAX_EXTERNAL_STYLESHEET_SOURCE_BYTES {
            return Err(EngineError::ExternalStylesheetTooLarge {
                limit: MAX_EXTERNAL_STYLESHEET_SOURCE_BYTES,
            });
        }

        let Some(node_id) = self
            .document
            .nodes()
            .find(|node| node.id().as_u64() == node_id)
            .map(|node| node.id())
        else {
            return Err(EngineError::ExternalStylesheetTargetInvalid(node_id));
        };
        let eligible = self
            .document
            .node(node_id)
            .is_some_and(|node| match node.kind() {
                NodeKind::Element(element) => stylesheet_link_is_eligible(element),
                _ => false,
            });
        if !eligible {
            return Err(EngineError::ExternalStylesheetTargetInvalid(
                node_id.as_u64(),
            ));
        }

        let mut external_stylesheets = self.external_stylesheets.clone();
        external_stylesheets.insert(node_id, source.to_owned());

        let viewport_width = self.layout.viewport_width().max(1.0);
        let viewport_height = self.layout.viewport_height().max(1.0);
        let styles = phantom_css::compute_styles_with_external_author_css_with_viewport(
            &self.document,
            &external_stylesheets,
            viewport_width,
            viewport_height,
        );
        let layout = build_layout_snapshot_with_images_and_viewport(
            &self.document,
            &styles,
            viewport_width,
            viewport_height,
            &self.images,
        )?;
        let paint = build_paint_list(&layout, &styles)?;
        let links = build_link_regions(&self.document, &layout);

        self.external_stylesheets = external_stylesheets;
        self.styles = styles;
        self.layout = layout;
        self.paint = paint;
        self.links = links;
        Ok(())
    }

    /// Executes bounded JavaScript against the current DOM snapshot and applies
    /// supported DOM mutations back into the real Phantom document.
    ///
    /// Browser Host Foundation II currently exposes `document.getElementById`,
    /// a simple `document.querySelector` subset, element `textContent`, and
    /// attribute read/write/remove methods. After mutations, CSS/layout/paint
    /// are rebuilt without reparsing the original HTML source.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] for JavaScript, DOM mutation, layout, or paint
    /// failures.
    pub fn execute_javascript(
        &mut self,
        source: &str,
        document_url: &str,
        device_pixel_ratio: f64,
    ) -> Result<JavaScriptValue, EngineError> {
        self.refresh_javascript_host(document_url, device_pixel_ratio)?;
        let value = self.javascript.execute(source)?;
        let mutations = self.javascript.drain_dom_mutations();

        if !mutations.is_empty() {
            let mappings = self.apply_javascript_dom_mutations(mutations)?;
            self.javascript.remap_dom_node_ids(&mappings);
            self.rebuild_after_dom_mutation()?;
            self.refresh_javascript_host(document_url, device_pixel_ratio)?;
        }

        Ok(value)
    }

    // PHANTOM_2M24_NATIVE_INTERACTION_JAVASCRIPT_EVENTS_I
    /// Dispatches a native-originated trusted click into JavaScript before
    /// the browser shell performs any default action.
    ///
    /// Listener DOM mutations are committed through the normal engine
    /// mutation boundary and CSS/layout/paint are rebuilt before returning.
    /// The boolean result is `false` when the click was canceled with
    /// `preventDefault()`.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] for JavaScript, DOM mutation, layout, or
    /// paint failures. Runtime state is not committed when engine mutation
    /// application fails.
    pub fn dispatch_native_click(
        &mut self,
        node_id: u64,
        document_url: &str,
        device_pixel_ratio: f64,
    ) -> Result<bool, EngineError> {
        self.refresh_javascript_host(document_url, device_pixel_ratio)?;

        let mut javascript = self.javascript.clone();
        let default_allowed = javascript.dispatch_trusted_click(node_id)?;
        let mutations = javascript.drain_dom_mutations();

        if !mutations.is_empty() {
            let mappings = self.apply_javascript_dom_mutations(mutations)?;
            javascript.remap_dom_node_ids(&mappings);
            self.rebuild_after_dom_mutation()?;
        }

        self.javascript = javascript;
        self.refresh_javascript_host(document_url, device_pixel_ratio)?;
        Ok(default_allowed)
    }

    // PHANTOM_2L5_ORDERED_PAGE_SCRIPTS_I
    /// Returns at most 64 eligible classic scripts in document order.
    /// Script contents are captured before execution mutates the DOM.
    #[must_use]
    pub fn page_script_tasks(&self) -> Vec<PageScriptTask> {
        self.document
            .nodes()
            .filter_map(|node| {
                let NodeKind::Element(element) = node.kind() else {
                    return None;
                };
                if element.tag_name() != "script"
                    || element.attribute("async").is_some()
                    || !is_supported_classic_script_type(element.attribute("type"))
                {
                    return None;
                }
                if let Some(src) = element.attribute("src") {
                    return (!src.trim().is_empty())
                        .then(|| PageScriptTask::External(src.to_owned()));
                }
                let source = descendant_text_content(&self.document, node.id());
                (!source.trim().is_empty()).then_some(PageScriptTask::Inline(source))
            })
            .take(64)
            .collect()
    }

    // PHANTOM_2L1_PAGE_SCRIPT_EXECUTION_I
    /// Executes eligible inline classic scripts in document order after HTML loading.
    /// Classic `defer` is admitted by the bounded post-parse model used by Phantom.
    /// External scripts are sequenced by the browser host; module and async scripts
    /// remain outside this compatibility slice. Each script is a separate job with
    /// its own microtask checkpoint. Invoke once
    /// per navigation; a second invocation would execute the scripts again.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if a script or resulting DOM update fails.
    pub fn execute_inline_page_scripts(
        &mut self,
        document_url: &str,
        device_pixel_ratio: f64,
    ) -> Result<usize, EngineError> {
        let scripts: Vec<String> = self
            .document
            .nodes()
            .filter_map(|node| {
                let NodeKind::Element(element) = node.kind() else {
                    return None;
                };
                if element.tag_name() != "script"
                    || element.attribute("src").is_some()
                    || element.attribute("async").is_some()
                    || !is_supported_classic_script_type(element.attribute("type"))
                {
                    return None;
                }
                Some(descendant_text_content(&self.document, node.id()))
            })
            .filter(|source| !source.trim().is_empty())
            .collect();

        let mut executed = 0;
        for source in scripts {
            self.execute_javascript(&source, document_url, device_pixel_ratio)?;
            executed += 1;
        }
        Ok(executed)
    }

    // PHANTOM_2L3_BROWSER_TIMER_PUMP_I
    /// Number of active JavaScript timers in the current document generation.
    #[must_use]
    pub fn pending_javascript_timer_count(&self) -> usize {
        self.javascript.pending_timer_count()
    }

    /// Advances the JavaScript virtual timer clock and executes ready tasks.
    ///
    /// The browser shell supplies elapsed monotonic milliseconds. Timers do not
    /// read wall-clock time directly. JavaScript runtime state is staged until
    /// all callbacks and any resulting DOM mutation batch succeed.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] for JavaScript timer failures, DOM mutation
    /// failures, layout failures, or paint failures.
    pub fn advance_javascript_time(
        &mut self,
        elapsed_ms: u64,
        document_url: &str,
        device_pixel_ratio: f64,
    ) -> Result<usize, EngineError> {
        self.refresh_javascript_host(document_url, device_pixel_ratio)?;

        let mut javascript = self.javascript.clone();
        let fired = javascript.advance_time(elapsed_ms)?;
        let mutations = javascript.drain_dom_mutations();

        if !mutations.is_empty() {
            let mappings = self.apply_javascript_dom_mutations(mutations)?;
            javascript.remap_dom_node_ids(&mappings);
            self.rebuild_after_dom_mutation()?;
        }

        self.javascript = javascript;
        self.refresh_javascript_host(document_url, device_pixel_ratio)?;
        Ok(fired)
    }

    /// Drains bounded structured JavaScript fetch requests for the Browser Host.
    ///
    /// URL resolution and transport security policy remain outside the engine.
    pub fn drain_javascript_fetch_host_requests(&mut self) -> Vec<FetchHostRequest> {
        self.javascript.drain_fetch_host_requests()
    }

    /// Drains JavaScript fetch requests for the browser/network layer.
    ///
    /// Each entry is `(request_id, raw_url)`. The engine deliberately does not
    /// perform URL resolution or network I/O here.
    pub fn drain_javascript_fetch_requests(&mut self) -> Vec<(u64, String)> {
        self.javascript.drain_fetch_requests()
    }

    /// Completes one JavaScript fetch request with a successful bounded response.
    ///
    /// Promise reactions and their microtasks execute before this method
    /// returns. Any DOM mutations produced by those reactions are applied
    /// transactionally and trigger CSS/layout/paint rebuild.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] for JavaScript completion, DOM, layout, or paint
    /// failures.
    pub fn complete_javascript_fetch_success(
        &mut self,
        request_id: u64,
        status: u16,
        response_url: &str,
        body: &str,
        document_url: &str,
        device_pixel_ratio: f64,
    ) -> Result<(), EngineError> {
        self.refresh_javascript_host(document_url, device_pixel_ratio)?;

        let mut javascript = self.javascript.clone();
        javascript.complete_fetch_success(request_id, status, response_url, body)?;
        let mutations = javascript.drain_dom_mutations();

        if !mutations.is_empty() {
            let mappings = self.apply_javascript_dom_mutations(mutations)?;
            javascript.remap_dom_node_ids(&mappings);
            self.rebuild_after_dom_mutation()?;
        }

        self.javascript = javascript;
        self.refresh_javascript_host(document_url, device_pixel_ratio)?;
        Ok(())
    }

    // PHANTOM_2N18_RESPONSE_HEADERS_EXPOSURE_I
    /// Completes a JavaScript fetch with network-admitted response headers.
    ///
    /// Header safety is checked again inside phantom-js, independently of the
    /// network response representation. DOM commits remain transactional.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on invalid headers, JavaScript or DOM failure.
    pub fn complete_javascript_fetch_success_with_headers(
        &mut self,
        request_id: u64,
        status: u16,
        response_url: &str,
        response_body_and_headers: (&str, &[(String, String)]),
        document_url: &str,
        device_pixel_ratio: f64,
    ) -> Result<(), EngineError> {
        self.refresh_javascript_host(document_url, device_pixel_ratio)?;
        let mut javascript = self.javascript.clone();
        let (body, headers) = response_body_and_headers;
        javascript.complete_fetch_success_with_headers(
            request_id,
            status,
            response_url,
            body,
            headers,
        )?;
        let mutations = javascript.drain_dom_mutations();
        if !mutations.is_empty() {
            let mappings = self.apply_javascript_dom_mutations(mutations)?;
            javascript.remap_dom_node_ids(&mappings);
            self.rebuild_after_dom_mutation()?;
        }
        self.javascript = javascript;
        self.refresh_javascript_host(document_url, device_pixel_ratio)?;
        Ok(())
    }

    /// Completes one JavaScript fetch request as a bounded network failure.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] for JavaScript rejection, DOM, layout, or paint
    /// failures.
    pub fn complete_javascript_fetch_failure(
        &mut self,
        request_id: u64,
        reason: &str,
        document_url: &str,
        device_pixel_ratio: f64,
    ) -> Result<(), EngineError> {
        self.refresh_javascript_host(document_url, device_pixel_ratio)?;

        let mut javascript = self.javascript.clone();
        javascript.complete_fetch_failure(request_id, reason)?;
        let mutations = javascript.drain_dom_mutations();

        if !mutations.is_empty() {
            let mappings = self.apply_javascript_dom_mutations(mutations)?;
            javascript.remap_dom_node_ids(&mappings);
            self.rebuild_after_dom_mutation()?;
        }

        self.javascript = javascript;
        self.refresh_javascript_host(document_url, device_pixel_ratio)?;
        Ok(())
    }

    fn refresh_javascript_host(
        &mut self,
        document_url: &str,
        device_pixel_ratio: f64,
    ) -> Result<(), EngineError> {
        let title = self.document_title().unwrap_or_default();
        let elements = javascript_dom_element_snapshots(&self.document)?;

        let snapshot = BrowserHostSnapshot::new(
            document_url,
            title,
            f64::from(self.layout.viewport_width()),
            f64::from(self.layout.viewport_height()),
            device_pixel_ratio,
        )?
        .with_dom_elements(elements)?;

        self.javascript.install_browser_host(&snapshot)?;
        Ok(())
    }

    fn apply_javascript_dom_mutations(
        &mut self,
        mutations: Vec<BrowserDomMutation>,
    ) -> Result<Vec<(u64, u64)>, EngineError> {
        let mut document = self.document.clone();
        let mut remap = BTreeMap::<u64, NodeId>::new();

        for mutation in mutations {
            match mutation {
                BrowserDomMutation::CreateElement { node_id, tag_name } => {
                    let committed = document.create_detached_element(tag_name)?;
                    remap.insert(node_id, committed);
                }
                BrowserDomMutation::AppendChild {
                    parent_id,
                    child_id,
                } => {
                    let parent = resolve_javascript_node_id(&document, &remap, parent_id)?;
                    let child = resolve_javascript_node_id(&document, &remap, child_id)?;
                    document.append_existing_child(parent, child)?;
                }
                BrowserDomMutation::DetachNode { node_id } => {
                    let node = resolve_javascript_node_id(&document, &remap, node_id)?;
                    document.detach_node(node)?;
                }
                BrowserDomMutation::SetTextContent { node_id, value } => {
                    let node = resolve_javascript_node_id(&document, &remap, node_id)?;
                    document.set_text_content(node, &value)?;
                }
                BrowserDomMutation::SetAttribute {
                    node_id,
                    name,
                    value,
                } => {
                    let node = resolve_javascript_node_id(&document, &remap, node_id)?;
                    document.set_element_attribute(node, name, value)?;
                }
                BrowserDomMutation::RemoveAttribute { node_id, name } => {
                    let node = resolve_javascript_node_id(&document, &remap, node_id)?;
                    document.remove_element_attribute(node, &name)?;
                }
            }
        }

        let mappings = remap
            .into_iter()
            .map(|(temporary, committed)| (temporary, committed.as_u64()))
            .collect::<Vec<_>>();
        self.document = document;
        Ok(mappings)
    }

    fn rebuild_after_dom_mutation(&mut self) -> Result<(), EngineError> {
        let viewport_width = self.layout.viewport_width().max(1.0);
        let viewport_height = self.layout.viewport_height().max(1.0);

        let styles = phantom_css::compute_styles_with_external_author_css_with_viewport(
            &self.document,
            &self.external_stylesheets,
            viewport_width,
            viewport_height,
        );
        let layout = build_layout_snapshot_with_images_and_viewport(
            &self.document,
            &styles,
            viewport_width,
            viewport_height,
            &self.images,
        )?;
        let paint = build_paint_list(&layout, &styles)?;
        let links = build_link_regions(&self.document, &layout);

        self.styles = styles;
        self.layout = layout;
        self.paint = paint;
        self.links = links;
        Ok(())
    }

    /// Registers intrinsic metadata for one image resource and recomputes
    /// geometry/paint for the active document.
    ///
    /// Resource identifiers are intentionally opaque. The current engine
    /// generation maps an image element's `NodeId::as_u64()` to
    /// [`ImageResourceId`].
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if relayout or paint generation fails.
    pub fn install_image_metadata(
        &mut self,
        resource: ImageResourceId,
        metadata: ImageMetadata,
        viewport_width: f32,
    ) -> Result<(), EngineError> {
        self.images.insert(resource, metadata);

        self.relayout_with_viewport(viewport_width, self.layout.viewport_height().max(1.0))
    }

    /// Recalculates computed styles, geometry, and paint commands for a changed viewport width.
    ///
    /// DOM parsing is deliberately not repeated. The CSS cascade is recomputed because
    /// viewport-dependent `@media` rules may activate or deactivate.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if layout or paint generation fails.
    pub fn relayout(&mut self, viewport_width: f32) -> Result<(), EngineError> {
        self.relayout_with_viewport(viewport_width, self.layout.viewport_height().max(1.0))
    }

    /// Recalculates computed styles, geometry, and paint commands for a changed two-dimensional viewport.
    ///
    /// DOM parsing is deliberately not repeated. The CSS cascade is recomputed against
    /// the new viewport before layout so responsive rules remain synchronized.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if layout or paint generation fails.
    pub fn relayout_with_viewport(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Result<(), EngineError> {
        let styles = phantom_css::compute_styles_with_external_author_css_with_viewport(
            &self.document,
            &self.external_stylesheets,
            viewport_width,
            viewport_height,
        );
        let layout = build_layout_snapshot_with_images_and_viewport(
            &self.document,
            &styles,
            viewport_width,
            viewport_height,
            &self.images,
        )?;

        let paint = build_paint_list(&layout, &styles)?;
        let links = build_link_regions(&self.document, &layout);

        self.styles = styles;
        self.layout = layout;
        self.paint = paint;
        self.links = links;

        Ok(())
    }
}

// PHANTOM_2J4_DOM_COLLECTIONS_HARDENING_I
fn javascript_dom_element_snapshots(
    document: &Document,
) -> Result<Vec<BrowserDomElementSnapshot>, JavaScriptError> {
    let mut roots = vec![document.root()];
    roots.extend(
        document
            .nodes()
            .filter(|node| node.id() != document.root() && node.parent().is_none())
            .map(|node| node.id()),
    );

    let mut seen = BTreeSet::new();
    let mut order = Vec::with_capacity(document.len());
    for root in roots {
        let mut stack = vec![root];
        while let Some(node_id) = stack.pop() {
            if !seen.insert(node_id) {
                continue;
            }
            order.push(node_id);
            if let Some(node) = document.node(node_id) {
                stack.extend(node.children().iter().rev().copied());
            }
        }
    }

    for node in document.nodes() {
        if seen.insert(node.id()) {
            order.push(node.id());
        }
    }

    order
        .into_iter()
        .filter_map(|node_id| {
            let node = document.node(node_id)?;
            let NodeKind::Element(element) = node.kind() else {
                return None;
            };
            Some(
                BrowserDomElementSnapshot::new(
                    node.id().as_u64(),
                    element.tag_name(),
                    element.attributes().clone(),
                    descendant_text_content(document, node.id()),
                )
                .map(|snapshot| {
                    snapshot
                        .with_parent_node_id(node.parent().map(NodeId::as_u64))
                        .with_connected(node_is_connected(document, node.id()))
                }),
            )
        })
        .collect()
}

fn resolve_javascript_node_id(
    document: &Document,
    remap: &BTreeMap<u64, NodeId>,
    raw_id: u64,
) -> Result<NodeId, DomError> {
    if let Some(node_id) = remap.get(&raw_id).copied() {
        return Ok(node_id);
    }

    find_node_by_raw_id(document, raw_id).ok_or(DomError::NodeNotFound(document.root()))
}

fn node_is_connected(document: &Document, node_id: NodeId) -> bool {
    let mut current = Some(node_id);
    let mut visited = 0usize;

    while let Some(current_id) = current {
        if current_id == document.root() {
            return true;
        }
        if visited >= phantom_dom::MAX_DOM_NODES {
            return false;
        }
        visited = visited.saturating_add(1);
        current = document.node(current_id).and_then(|node| node.parent());
    }

    false
}

fn find_node_by_raw_id(document: &Document, raw_id: u64) -> Option<NodeId> {
    document
        .nodes()
        .find(|node| node.id().as_u64() == raw_id)
        .map(|node| node.id())
}

fn node_is_descendant_or_self(document: &Document, node_id: NodeId, ancestor: NodeId) -> bool {
    let mut current = Some(node_id);

    while let Some(current_id) = current {
        if current_id == ancestor {
            return true;
        }

        current = document.node(current_id).and_then(|node| node.parent());
    }

    false
}
fn form_for_node(document: &Document, node_id: NodeId) -> Option<NodeId> {
    let mut current = document.node(node_id)?.parent();

    while let Some(current_id) = current {
        let node = document.node(current_id)?;

        if let NodeKind::Element(element) = node.kind()
            && element.tag_name() == "form"
        {
            return Some(current_id);
        }

        current = node.parent();
    }

    None
}

fn normalized_input_type(element: &ElementData) -> String {
    element
        .attribute("type")
        .unwrap_or("text")
        .trim()
        .to_ascii_lowercase()
}

fn button_is_submit(element: &ElementData) -> bool {
    matches!(
        element
            .attribute("type")
            .unwrap_or("submit")
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "" | "submit"
    )
}

fn form_control_label(
    document: &Document,
    node_id: NodeId,
    element: &ElementData,
    kind: FormControlKind,
) -> String {
    match kind {
        FormControlKind::Text | FormControlKind::Search => String::new(),

        FormControlKind::Submit if element.tag_name() == "input" => element
            .attribute("value")
            .filter(|value| !value.is_empty())
            .unwrap_or("Submit")
            .to_owned(),

        FormControlKind::Submit => {
            let text = descendant_text_content(document, node_id);
            if text.trim().is_empty() {
                "Submit".to_owned()
            } else {
                text.trim().to_owned()
            }
        }
    }
}

fn descendant_text_content(document: &Document, node_id: NodeId) -> String {
    let Some(node) = document.node(node_id) else {
        return String::new();
    };

    let mut text = String::new();
    let mut stack = node.children().iter().rev().copied().collect::<Vec<_>>();

    while let Some(current_id) = stack.pop() {
        let Some(current) = document.node(current_id) else {
            continue;
        };

        match current.kind() {
            NodeKind::Text(value) => text.push_str(value),
            NodeKind::Document | NodeKind::Element(_) | NodeKind::Comment(_) => {}
        }

        stack.extend(current.children().iter().rev().copied());
    }

    text
}
fn build_link_regions(document: &Document, layout: &LayoutSnapshot) -> Vec<LinkRegion> {
    layout
        .boxes()
        .iter()
        .filter_map(|layout_box| {
            if !matches!(
                layout_box.kind(),
                LayoutKind::Text { .. } | LayoutKind::Image { .. }
            ) {
                return None;
            }

            let (anchor_node, anchor) = link_element_for_node(document, layout_box.source_node())?;
            let href = anchor.attribute("href")?.to_owned();
            let rect = layout_box.rect();

            if rect.width() <= 0.0 || rect.height() <= 0.0 {
                return None;
            }

            Some(LinkRegion {
                node_id: anchor_node.as_u64(),
                href,
                rect,
                opens_new_context: anchor
                    .attribute("target")
                    .is_some_and(|target| target.trim().eq_ignore_ascii_case("_blank")),
            })
        })
        .collect()
}

fn link_element_for_node(document: &Document, node_id: NodeId) -> Option<(NodeId, &ElementData)> {
    let mut current = Some(node_id);

    while let Some(current_id) = current {
        let node = document.node(current_id)?;

        if let NodeKind::Element(element) = node.kind()
            && element.tag_name() == "a"
            && element.attribute("href").is_some()
        {
            return Some((current_id, element));
        }

        current = node.parent();
    }

    None
}

fn nearest_element_node(document: &Document, node_id: NodeId) -> Option<NodeId> {
    let mut current = Some(node_id);

    while let Some(current_id) = current {
        let node = document.node(current_id)?;
        if matches!(node.kind(), NodeKind::Element(_)) {
            return Some(current_id);
        }
        current = node.parent();
    }

    None
}

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x() && y >= rect.y() && x <= rect.x() + rect.width() && y <= rect.y() + rect.height()
}
#[derive(Clone, Copy, Debug)]
enum CandidateDescriptor {
    Density(f32),
    Width(f32),
}

#[derive(Clone, Debug)]
struct ImageCandidate {
    source: String,
    descriptor: CandidateDescriptor,
}

fn select_image_source(
    document: &Document,
    image_id: NodeId,
    viewport_width: f32,
    dpr: f32,
) -> Option<String> {
    let image = element_for(document, image_id)?;

    if let Some(parent_id) = document.node(image_id)?.parent()
        && let Some(parent) = element_for(document, parent_id)
        && parent.tag_name() == "picture"
        && let Some(source) = picture_source(document, parent_id, image_id, viewport_width, dpr)
    {
        return Some(source);
    }

    select_from_element(image, viewport_width, dpr)
}

fn picture_source(
    document: &Document,
    picture_id: NodeId,
    image_id: NodeId,
    viewport_width: f32,
    dpr: f32,
) -> Option<String> {
    let picture = document.node(picture_id)?;

    for child_id in picture.children() {
        if *child_id == image_id {
            break;
        }

        let Some(source) = element_for(document, *child_id) else {
            continue;
        };

        if source.tag_name() != "source" {
            continue;
        }

        if !supported_source_type(source.attribute("type")) {
            continue;
        }

        if !media_matches(source.attribute("media"), viewport_width) {
            continue;
        }

        let Some(srcset) = source.attribute("srcset") else {
            continue;
        };
        let sizes = source.attribute("sizes");
        if let Some(selected) = select_srcset(srcset, sizes, viewport_width, dpr) {
            return Some(selected);
        }
    }

    None
}

fn select_from_element(element: &ElementData, viewport_width: f32, dpr: f32) -> Option<String> {
    if let Some(srcset) = element.attribute("srcset")
        && let Some(selected) =
            select_srcset(srcset, element.attribute("sizes"), viewport_width, dpr)
    {
        return Some(selected);
    }

    element
        .attribute("src")
        .map(str::trim)
        .filter(|source| !source.is_empty())
        .map(str::to_owned)
}

fn resource_priority(value: Option<&str>) -> ResourcePriority {
    match value.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("high") => ResourcePriority::High,
        Some(value) if value.eq_ignore_ascii_case("low") => ResourcePriority::Low,
        _ => ResourcePriority::Auto,
    }
}

fn preload_image_from_element(
    element: &ElementData,
    viewport_width: f32,
    dpr: f32,
) -> Option<ImagePreloadRequest> {
    if element.tag_name() != "link"
        || !rel_has_token(element.attribute("rel"), "preload")
        || !element
            .attribute("as")
            .is_some_and(|value| value.eq_ignore_ascii_case("image"))
        || !supported_source_type(element.attribute("type"))
        || !media_matches(element.attribute("media"), viewport_width)
    {
        return None;
    }

    let source = element
        .attribute("imagesrcset")
        .and_then(|srcset| {
            select_srcset(srcset, element.attribute("imagesizes"), viewport_width, dpr)
        })
        .or_else(|| {
            element
                .attribute("href")
                .map(str::trim)
                .filter(|source| !source.is_empty())
                .map(str::to_owned)
        })?;

    Some(ImagePreloadRequest {
        source,
        priority: resource_priority(element.attribute("fetchpriority")),
    })
}

fn rel_has_token(value: Option<&str>, token: &str) -> bool {
    value.is_some_and(|value| {
        value
            .split_ascii_whitespace()
            .any(|candidate| candidate.eq_ignore_ascii_case(token))
    })
}

fn site_icon_from_element(element: &ElementData) -> Option<SiteIconRequest> {
    if element.tag_name() != "link"
        || !rel_has_token(element.attribute("rel"), "icon")
        || !site_icon_type_supported(element.attribute("type"))
    {
        return None;
    }

    let source = element
        .attribute("href")
        .map(str::trim)
        .filter(|source| !source.is_empty())?
        .to_owned();

    Some(SiteIconRequest { source })
}

fn site_icon_type_supported(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return true;
    };

    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "image/png"
            | "image/jpeg"
            | "image/jpg"
            | "image/gif"
            | "image/webp"
            | "image/x-icon"
            | "image/vnd.microsoft.icon"
            | "image/ico"
    )
}
fn element_for(document: &Document, node_id: NodeId) -> Option<&ElementData> {
    let node = document.node(node_id)?;
    let NodeKind::Element(element) = node.kind() else {
        return None;
    };
    Some(element)
}

fn supported_source_type(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return true;
    };

    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp"
    )
}

fn media_matches(value: Option<&str>, viewport_width: f32) -> bool {
    let Some(value) = value else {
        return true;
    };

    let condition = value.trim().to_ascii_lowercase();
    if condition.is_empty() {
        return true;
    }

    parse_media_px(&condition, "max-width").is_some_and(|limit| viewport_width <= limit)
        || parse_media_px(&condition, "min-width").is_some_and(|limit| viewport_width >= limit)
}

fn parse_media_px(condition: &str, feature: &str) -> Option<f32> {
    let body = condition.strip_prefix('(')?.strip_suffix(')')?.trim();
    let (name, value) = body.split_once(':')?;
    if name.trim() != feature {
        return None;
    }
    parse_css_px(value.trim())
}

fn select_srcset(
    srcset: &str,
    sizes: Option<&str>,
    viewport_width: f32,
    dpr: f32,
) -> Option<String> {
    let candidates = parse_srcset(srcset);
    if candidates.is_empty() {
        return None;
    }

    let uses_width = candidates
        .iter()
        .any(|candidate| matches!(candidate.descriptor, CandidateDescriptor::Width(_)));

    let target = if uses_width {
        resolve_source_size(sizes, viewport_width) * dpr
    } else {
        dpr
    };

    let mut best_above: Option<(&ImageCandidate, f32)> = None;
    let mut best_below: Option<(&ImageCandidate, f32)> = None;

    for candidate in &candidates {
        let value = match candidate.descriptor {
            CandidateDescriptor::Density(value) if !uses_width => value,
            CandidateDescriptor::Width(value) if uses_width => value,
            _ => continue,
        };

        if value >= target {
            if best_above.is_none_or(|(_, best)| value < best) {
                best_above = Some((candidate, value));
            }
        } else if best_below.is_none_or(|(_, best)| value > best) {
            best_below = Some((candidate, value));
        }
    }

    best_above
        .or(best_below)
        .map(|(candidate, _)| candidate.source.clone())
}

fn parse_srcset(srcset: &str) -> Vec<ImageCandidate> {
    srcset
        .split(',')
        .filter_map(|raw| {
            let mut parts = raw.split_ascii_whitespace();
            let source = parts.next()?.trim();
            if source.is_empty() {
                return None;
            }

            let descriptor = match parts.next() {
                None => CandidateDescriptor::Density(1.0),
                Some(value) if value.ends_with('x') => value
                    .strip_suffix('x')?
                    .parse::<f32>()
                    .ok()
                    .filter(|value| *value > 0.0)
                    .map(CandidateDescriptor::Density)?,
                Some(value) if value.ends_with('w') => value
                    .strip_suffix('w')?
                    .parse::<f32>()
                    .ok()
                    .filter(|value| *value > 0.0)
                    .map(CandidateDescriptor::Width)?,
                Some(_) => return None,
            };

            if parts.next().is_some() {
                return None;
            }

            Some(ImageCandidate {
                source: source.to_owned(),
                descriptor,
            })
        })
        .collect()
}

fn resolve_source_size(sizes: Option<&str>, viewport_width: f32) -> f32 {
    let Some(sizes) = sizes else {
        return viewport_width;
    };

    for raw in sizes.split(',') {
        let item = raw.trim();
        if item.is_empty() {
            continue;
        }

        if item.starts_with('(') {
            let Some(close) = item.find(')') else {
                continue;
            };
            let media = &item[..=close];
            let length = item[close + 1..].trim();
            if media_matches(Some(media), viewport_width)
                && let Some(px) = parse_source_size(length, viewport_width)
            {
                return px;
            }
        } else if let Some(px) = parse_source_size(item, viewport_width) {
            return px;
        }
    }

    viewport_width
}

fn parse_source_size(value: &str, viewport_width: f32) -> Option<f32> {
    if let Some(px) = parse_css_px(value) {
        return Some(px.max(1.0));
    }

    value
        .trim()
        .strip_suffix("vw")?
        .trim()
        .parse::<f32>()
        .ok()
        .map(|percent| (viewport_width * percent / 100.0).max(1.0))
}

fn parse_css_px(value: &str) -> Option<f32> {
    value
        .trim()
        .strip_suffix("px")?
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| *value >= 0.0)
}

#[cfg(test)]
mod tests {
    use phantom_dom::NodeKind;

    use super::{
        Engine, EngineError, EngineState, ImageLoading, LayoutKind, PageScriptTask, PaintCommand,
        Rgba,
    };

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
            "<body style=\"margin: 0\"><div id=\"relayout-target\" style=\"width: 50%; background: #112233\">Phantom</div></body>",
            1000.0,
        )?;

        let target_node = engine
            .document()
            .nodes()
            .find_map(|node| match node.kind() {
                NodeKind::Element(element)
                    if element.attribute("id") == Some("relayout-target") =>
                {
                    Some(node.id())
                }
                NodeKind::Document
                | NodeKind::Element(_)
                | NodeKind::Text(_)
                | NodeKind::Comment(_) => None,
            });
        assert!(target_node.is_some());
        let target_node = target_node.unwrap_or(engine.document().root());

        let initial_width = engine
            .layout()
            .boxes()
            .iter()
            .find(|layout_box| {
                layout_box.source_node() == target_node
                    && matches!(layout_box.kind(), LayoutKind::Block)
            })
            .map(|layout_box| layout_box.rect().width())
            .unwrap_or_default();

        engine.relayout(600.0)?;

        let relayout_width = engine
            .layout()
            .boxes()
            .iter()
            .find(|layout_box| {
                layout_box.source_node() == target_node
                    && matches!(layout_box.kind(), LayoutKind::Block)
            })
            .map(|layout_box| layout_box.rect().width())
            .unwrap_or_default();

        assert!((initial_width - 500.0).abs() < f32::EPSILON);
        assert!((relayout_width - 300.0).abs() < f32::EPSILON);
        assert!(!engine.paint_list().is_empty());

        Ok(())
    }

    #[test]
    fn exposes_image_requests_without_browser_layout_inspection() -> Result<(), EngineError> {
        let mut engine = Engine::new();

        engine
            .load_html("<img src=\"/media/hero.png\" width=\"40\" height=\"20\" alt=\"Hero\">")?;

        let requests = engine.image_requests();

        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests.first().map(|request| request.source()),
            Some("/media/hero.png"),
        );

        Ok(())
    }

    #[test]
    fn exposes_normalized_image_loading_policy() -> Result<(), EngineError> {
        let mut engine = Engine::new();

        engine.load_html("<img src=\"eager.png\"><img src=\"lazy.png\" loading=\"LaZy\">")?;

        let requests = engine.image_requests();

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].loading(), ImageLoading::Eager);
        assert_eq!(requests[1].loading(), ImageLoading::Lazy);
        assert!(requests[1].bottom() >= requests[1].top());

        Ok(())
    }

    // PHANTOM_2N1_REAL_WEB_NETWORK_I
    #[test]
    fn defer_and_legacy_classic_script_types_are_exposed_as_page_tasks() -> Result<(), EngineError>
    {
        let mut engine = Engine::new();

        engine.load_html(
            r#"
            <script defer src="/deferred.js" type="application/ecmascript"></script>
            <script defer type="text/ecmascript">window.inlineDefer = 1;</script>
            <script async src="/async.js"></script>
            <script type="module" src="/module.js"></script>
            "#,
        )?;

        let tasks = engine.page_script_tasks();
        assert_eq!(tasks.len(), 2);
        assert_eq!(
            tasks.first(),
            Some(&PageScriptTask::External("/deferred.js".to_owned()))
        );
        assert!(matches!(
            tasks.get(1),
            Some(PageScriptTask::Inline(source)) if source.contains("inlineDefer")
        ));

        Ok(())
    }

    // PHANTOM_2N2_EXTERNAL_STYLESHEETS_I
    #[test]
    fn external_stylesheets_preserve_author_source_order_and_rebuild_paint()
    -> Result<(), EngineError> {
        let mut engine = Engine::new();
        engine.load_html(
            r#"
            <style>#target { color: red; }</style>
            <link rel="stylesheet" href="/first.css">
            <style>#target { color: blue; }</style>
            <link rel="stylesheet" href="/last.css" type="text/css">
            <link rel="alternate stylesheet" href="/alternate.css">
            <link rel="stylesheet" href="/print.css" media="print">
            <p id="target">External CSS</p>
            "#,
        )?;

        let requests = engine.external_stylesheet_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].source(), "/first.css");
        assert_eq!(requests[1].source(), "/last.css");

        engine.install_external_stylesheet(
            requests[0].node_id(),
            "#target { color: rgb(0, 128, 0); }",
        )?;
        let target = engine
            .document()
            .nodes()
            .find_map(|node| match node.kind() {
                NodeKind::Element(element) if element.attribute("id") == Some("target") => {
                    Some(node.id())
                }
                _ => None,
            })
            .ok_or(EngineError::ExternalStylesheetTargetInvalid(0))?;
        assert_eq!(
            engine.styles().get(target).map(|style| style.color()),
            Some(Rgba::new(0, 0, 255, 255))
        );

        engine.install_external_stylesheet(
            requests[1].node_id(),
            "#target { color: rgb(106, 27, 154); }",
        )?;
        assert_eq!(
            engine.styles().get(target).map(|style| style.color()),
            Some(Rgba::new(106, 27, 154, 255))
        );
        assert!(engine.paint_list().commands().iter().any(|command| {
            matches!(
                command,
                PaintCommand::Text { color, .. }
                    if color.red() == 106 && color.green() == 27 && color.blue() == 154
            )
        }));

        Ok(())
    }

    // PHANTOM_2N3_STYLESHEET_LIFECYCLE_CACHE_III
    #[test]
    fn external_stylesheet_media_reacts_to_relayout_without_reinstall() -> Result<(), EngineError> {
        let mut engine = Engine::new();
        engine.load_html(
            "<link rel='stylesheet' href='/responsive.css'><p id='responsive'>Responsive external CSS</p>",
        )?;
        let request = engine
            .external_stylesheet_requests()
            .into_iter()
            .next()
            .ok_or(EngineError::ExternalStylesheetTargetInvalid(0))?;
        engine.install_external_stylesheet(
            request.node_id(),
            "#responsive { color: rgb(198, 40, 40); } @media (max-width: 700px) { #responsive { color: rgb(21, 101, 192); } }",
        )?;
        let responsive = engine
            .document()
            .nodes()
            .find_map(|node| match node.kind() {
                NodeKind::Element(element) if element.attribute("id") == Some("responsive") => {
                    Some(node.id())
                }
                _ => None,
            })
            .ok_or(EngineError::ExternalStylesheetTargetInvalid(0))?;

        let wide = engine
            .styles()
            .get(responsive)
            .map(|style| style.color())
            .ok_or(EngineError::ExternalStylesheetTargetInvalid(0))?;
        assert_eq!(wide, Rgba::new(198, 40, 40, 255));
        engine.relayout_with_viewport(600.0, 768.0)?;
        let narrow = engine
            .styles()
            .get(responsive)
            .map(|style| style.color())
            .ok_or(EngineError::ExternalStylesheetTargetInvalid(0))?;
        assert_eq!(narrow, Rgba::new(21, 101, 192, 255));
        engine.relayout_with_viewport(1024.0, 768.0)?;
        let wide_again = engine
            .styles()
            .get(responsive)
            .map(|style| style.color())
            .ok_or(EngineError::ExternalStylesheetTargetInvalid(0))?;
        assert_eq!(wide_again, Rgba::new(198, 40, 40, 255));
        Ok(())
    }
}
