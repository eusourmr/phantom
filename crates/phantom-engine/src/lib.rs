//! Phantom web engine orchestration boundary.
//!
//! The engine owns immutable snapshots of DOM, computed styles, cold layout,
//! and renderer-neutral paint commands. Networking and native-window
//! responsibilities remain outside this crate.

#![forbid(unsafe_code)]

use phantom_dom::{Document, ElementData, NodeId, NodeKind};
use phantom_html::HtmlError;

pub use phantom_css::{
    AlignContent, AlignItems, AlignSelf, AutoEdges, BorderStyle, BoxSizing, ComputedStyle, Display,
    EdgeSizes, FlexDirection, FlexWrap, FontFamily, FontStyle, FontWeight, JustifyContent, Length,
    ObjectFit, ObjectPosition, Rgba, StyleMap,
};
pub use phantom_image::{
    DecodedImage, ImageCatalog, ImageDecodeLimits, ImageDecoder, ImageError, ImageFormat,
    ImageMetadata, ImageResourceId, IntrinsicSize, probe_image,
};
pub use phantom_layout::{
    LayoutBox, LayoutError, LayoutId, LayoutKind, LayoutSnapshot, Rect, build_layout_snapshot,
    build_layout_snapshot_with_images, build_layout_snapshot_with_shaper,
    build_layout_snapshot_with_shaper_and_images,
};
pub use phantom_paint::{
    PaintColor, PaintCommand, PaintError, PaintFontFamily, PaintFontStyle, PaintFontWeight,
    PaintList, PaintRect, PaintTextRange, build_paint_list,
};

use thiserror::Error;

const DEFAULT_LAYOUT_VIEWPORT_WIDTH: f32 = 1024.0;

// PHANTOM_2C12_LINK_NAVIGATION_I
/// One clickable hyperlink region in the active cold layout snapshot.
///
/// URL resolution deliberately remains outside the engine. `href` is preserved
/// exactly as declared so browser navigation can resolve it against the
/// committed document URL through `phantom-net`.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkRegion {
    href: String,
    rect: Rect,
    opens_new_context: bool,
}

impl LinkRegion {
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
}

/// High-level Phantom engine orchestration shell.
///
/// The engine owns processing snapshots but does not own networking,
/// operating-system windows, or browser chrome.
#[derive(Debug)]
pub struct Engine {
    document: Document,
    styles: StyleMap,
    images: ImageCatalog,
    layout: LayoutSnapshot,
    paint: PaintList,
    links: Vec<LinkRegion>,
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
            images: ImageCatalog::default(),
            layout: LayoutSnapshot::empty(DEFAULT_LAYOUT_VIEWPORT_WIDTH),
            paint: PaintList::empty(DEFAULT_LAYOUT_VIEWPORT_WIDTH),
            links: Vec::new(),
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

    /// Returns the first supported site icon explicitly declared by the active
    /// document.
    ///
    /// The first milestone recognizes `rel` token `icon`, a non-empty `href`,
    /// and PNG/JPEG/GIF/WebP MIME types. A missing `type` is allowed and the
    /// bounded decoder remains authoritative after fetch.
    #[must_use]
    pub fn site_icon_request(&self) -> Option<SiteIconRequest> {
        self.document.nodes().find_map(|node| {
            let NodeKind::Element(element) = node.kind() else {
                return None;
            };

            site_icon_from_element(element)
        })
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
        let images = ImageCatalog::default();
        let layout =
            build_layout_snapshot_with_images(&document, &styles, viewport_width, &images)?;
        let paint = build_paint_list(&layout, &styles)?;
        let links = build_link_regions(&document, &layout);

        self.document = document;
        self.styles = styles;
        self.images = images;
        self.layout = layout;
        self.paint = paint;
        self.links = links;
        self.state = EngineState::Ready;

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

        self.relayout(viewport_width)
    }

    /// Recalculates geometry and paint commands for a changed viewport width.
    ///
    /// DOM parsing and CSS cascade are deliberately not repeated.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if layout or paint generation fails.
    pub fn relayout(&mut self, viewport_width: f32) -> Result<(), EngineError> {
        let layout = build_layout_snapshot_with_images(
            &self.document,
            &self.styles,
            viewport_width,
            &self.images,
        )?;

        let paint = build_paint_list(&layout, &self.styles)?;
        let links = build_link_regions(&self.document, &layout);

        self.layout = layout;
        self.paint = paint;
        self.links = links;

        Ok(())
    }
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

            let anchor = link_element_for_node(document, layout_box.source_node())?;
            let href = anchor.attribute("href")?.to_owned();
            let rect = layout_box.rect();

            if rect.width() <= 0.0 || rect.height() <= 0.0 {
                return None;
            }

            Some(LinkRegion {
                href,
                rect,
                opens_new_context: anchor
                    .attribute("target")
                    .is_some_and(|target| target.trim().eq_ignore_ascii_case("_blank")),
            })
        })
        .collect()
}

fn link_element_for_node(document: &Document, node_id: NodeId) -> Option<&ElementData> {
    let mut current = Some(node_id);

    while let Some(current_id) = current {
        let node = document.node(current_id)?;

        if let NodeKind::Element(element) = node.kind()
            && element.tag_name() == "a"
            && element.attribute("href").is_some()
        {
            return Some(element);
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
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp"
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

    use super::{Engine, EngineError, EngineState, ImageLoading, LayoutKind, PaintCommand, Rgba};

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
}
