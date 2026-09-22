//! Independent CSS parser, cascade, and computed-style engine for Phantom.
//!
//! This crate is the style-system wall between the DOM and layout. It parses a
//! deliberately bounded CSS subset, resolves selectors and cascade priority,
//! computes inherited values, and emits a compact immutable style snapshot.
//!
//! The current milestone consolidates Phantom Flexbox across both axes:
//! direction, wrapping including `wrap-reverse`, main/cross-axis alignment,
//! gap, grow, shrink, basis, `flex`, and `flex-flow` shorthand.
//!
//! Layout never parses CSS, and paint never reads CSS or the DOM directly.

#![forbid(unsafe_code)]

use std::array;
use std::collections::BTreeMap;

use phantom_dom::{Document, ElementData, MAX_DOM_NODES, Node, NodeId, NodeKind};

const ROOT_FONT_SIZE_PX: f32 = 16.0;
const NO_STYLE: usize = usize::MAX;
const MAX_CSS_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_SCANNED_RULE_BLOCKS: usize = 2_048;
const MAX_ACCEPTED_RULES: usize = 1_024;
const MAX_DECLARATIONS_PER_RULE: usize = 64;
const MAX_SELECTOR_BYTES: usize = 4 * 1024;
const MAX_SELECTOR_PARTS: usize = 32;
const MAX_CLASSES_PER_COMPOUND: usize = 32;
const MAX_ATTRIBUTE_SELECTORS_PER_COMPOUND: usize = 32;
const MAX_PSEUDO_CLASSES_PER_COMPOUND: usize = 32;
const MAX_INLINE_STYLE_BYTES: usize = 64 * 1024;
const MAX_CASCADE_RULE_EVALUATIONS: usize = 2_000_000;
const MAX_NUMERIC_MAGNITUDE: f32 = 1_000_000.0;
const DEFAULT_MEDIA_VIEWPORT_WIDTH: f32 = 1024.0;
const DEFAULT_MEDIA_VIEWPORT_HEIGHT: f32 = 768.0;
const MAX_MEDIA_NESTING: usize = 8;
const MAX_MEDIA_QUERIES_PER_LIST: usize = 32;

// PHANTOM_2H4_RESPONSIVE_VIEWPORT_I

/// RGBA color represented with eight-bit channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rgba {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl Rgba {
    /// Creates an RGBA color.
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

/// CSS display behavior currently understood by Phantom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Display {
    /// Block formatting behavior.
    Block,

    /// Inline formatting behavior.
    Inline,

    /// Element does not generate a layout box.
    None,

    /// Flex formatting context.
    Flex,
}

// PHANTOM_2G10_POSITIONED_LAYOUT_I
// PHANTOM_2G11_POSITIONED_LAYOUT_II
// PHANTOM_2G12_POSITIONED_LAYOUT_III
// PHANTOM_2G14_FIXED_POSITIONING_I
/// CSS positioning mode supported by the current computed-style layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Position {
    /// Ordinary normal-flow positioning.
    Static,

    /// Normal-flow placement followed by a visual relative offset.
    Relative,

    /// Out-of-flow positioning against the nearest positioned containing block.
    Absolute,

    /// Out-of-flow positioning against the initial viewport containing block.
    Fixed,
}

// PHANTOM_2G15_STACKING_Z_INDEX_I
/// Coarse computed z-index used by the first Phantom stacking model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ZIndex {
    /// Preserve source-order stacking without an explicit integer level.
    Auto,

    /// Explicit bounded integer stacking level.
    Integer(i32),
}

/// Font weight used by a computed style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontWeight {
    /// Normal font weight.
    Normal,

    /// Bold font weight.
    Bold,
}

/// Font posture used by a computed style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontStyle {
    /// Upright font.
    Normal,

    /// Italic or oblique font.
    Italic,
}

/// Coarse font-family category used before web-font support exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontFamily {
    /// Sans-serif family.
    SansSerif,

    /// Monospace family.
    Monospace,
}

// PHANTOM_2G6_LINE_HEIGHT_FOUNDATION
/// Computed CSS `line-height` behavior.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    /// Use the text backend's natural line height.
    Normal,

    /// Unitless multiplier resolved against the element's own font size.
    Number(f32),

    /// Absolute computed line height in logical pixels.
    Px(f32),
}

// PHANTOM_2G7_WHITE_SPACE_FOUNDATION
// PHANTOM_2G9_WHITE_SPACE_II
/// Computed CSS `white-space` behavior supported by Phantom inline layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WhiteSpace {
    /// Collapse consecutive whitespace and wrap at collapsible spaces.
    Normal,

    /// Collapse consecutive whitespace but suppress automatic line wrapping.
    NoWrap,

    /// Preserve spaces, tabs, and explicit line breaks without automatic wrap.
    Pre,

    /// Preserve spaces, tabs, and explicit line breaks while allowing wrapping.
    PreWrap,

    /// Collapse spaces while preserving explicit line breaks and allowing wrap.
    PreLine,
}

/// CSS length supported by the current computed-style layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// Automatic sizing.
    Auto,

    /// Absolute logical pixels.
    Px(f32),

    /// Percentage resolved later by layout.
    Percent(f32),
}

/// Box sizing model used for width and height calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BoxSizing {
    /// Declared width and height describe the content box.
    ContentBox,

    /// Declared width and height include padding and borders.
    BorderBox,
}

/// Sizing mode used to fit a replaced image inside its content box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectFit {
    /// Stretch independently on both axes to fill the content box.
    Fill,
    /// Preserve aspect ratio and fit entirely inside the content box.
    Contain,
    /// Preserve aspect ratio and cover the entire content box.
    Cover,
    /// Keep the intrinsic size.
    None,
    /// Use intrinsic size when it fits, otherwise contain it.
    ScaleDown,
}

/// Percentage-based object position inside a replaced element content box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectPosition {
    x: f32,
    y: f32,
}

impl ObjectPosition {
    /// Creates a clamped percentage position, where `0.0` is start and `1.0` is end.
    #[must_use]
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x: sanitize_unit_interval(x),
            y: sanitize_unit_interval(y),
        }
    }
    /// Returns the horizontal percentage.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }
    /// Returns the vertical percentage.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }
}

impl Default for ObjectPosition {
    fn default() -> Self {
        Self::new(0.5, 0.5)
    }
}

/// Border painting style supported by the first Phantom box model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BorderStyle {
    /// Border does not generate visible edge geometry.
    None,

    /// Solid border.
    Solid,
}

// PHANTOM_2H2_BOX_VISUAL_FIDELITY_I
/// Overflow painting behavior supported by the first Phantom clipping model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Overflow {
    /// Descendant paint may extend outside the padding box.
    Visible,

    /// Descendant paint is clipped to the padding box.
    Hidden,
}

/// Main-axis direction used by the first Phantom Flexbox core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FlexDirection {
    /// Main axis runs horizontally from left to right.
    Row,

    /// Main axis runs horizontally from right to left.
    RowReverse,

    /// Main axis runs vertically from top to bottom.
    Column,

    /// Main axis runs vertically from bottom to top.
    ColumnReverse,
}

/// Whether flex items stay on one line or may wrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FlexWrap {
    /// All flex items remain on one flex line.
    NoWrap,

    /// Flex items may create additional flex lines from cross-start.
    Wrap,

    /// Flex items wrap while reversing the cross-start/cross-end direction.
    WrapReverse,
}

/// Distribution of flex items along the main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum JustifyContent {
    /// Items begin at the main-start edge.
    FlexStart,

    /// Items are centered on the main axis.
    Center,

    /// Items end at the main-end edge.
    FlexEnd,

    /// Remaining space is distributed between adjacent items.
    SpaceBetween,

    /// Remaining space is distributed around items with half-size edge spaces.
    SpaceAround,
}

/// Cross-axis alignment used by a flex container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlignItems {
    /// Auto-sized flex items stretch across the cross axis.
    Stretch,

    /// Items align to the cross-start edge.
    FlexStart,

    /// Items are centered on the cross axis.
    Center,

    /// Items align to the cross-end edge.
    FlexEnd,
}

/// Cross-axis distribution of multiple flex lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlignContent {
    /// Extra cross-axis space is distributed into line cross sizes.
    Stretch,

    /// Flex lines begin at the cross-start edge.
    FlexStart,

    /// Flex lines are centered on the cross axis.
    Center,

    /// Flex lines end at the cross-end edge.
    FlexEnd,

    /// Extra space is distributed between flex lines.
    SpaceBetween,

    /// Extra space is distributed around flex lines with half-size edge spaces.
    SpaceAround,
}

/// Per-item override of a flex container's `align-items`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlignSelf {
    /// Use the parent flex container's `align-items` value.
    Auto,

    /// Auto-sized item stretches across its flex line.
    Stretch,

    /// Item aligns to the cross-start edge.
    FlexStart,

    /// Item is centered on the cross axis.
    Center,

    /// Item aligns to the cross-end edge.
    FlexEnd,
}

/// Four physical edge values in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeSizes {
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
}

impl EdgeSizes {
    /// Creates four edge values.
    #[must_use]
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Returns zero-valued edges.
    #[must_use]
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }

    /// Returns the top edge.
    #[must_use]
    pub const fn top(self) -> f32 {
        self.top
    }

    /// Returns the right edge.
    #[must_use]
    pub const fn right(self) -> f32 {
        self.right
    }

    /// Returns the bottom edge.
    #[must_use]
    pub const fn bottom(self) -> f32 {
        self.bottom
    }

    /// Returns the left edge.
    #[must_use]
    pub const fn left(self) -> f32 {
        self.left
    }
}

/// Semantic `auto` state for physical margin edges.
///
/// Numeric margin lengths remain in [`EdgeSizes`]. This companion value keeps
/// `auto` explicit until the formatting context that owns its resolution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct AutoEdges {
    top: bool,
    right: bool,
    bottom: bool,
    left: bool,
}

impl AutoEdges {
    /// Returns edges with no automatic margins.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            top: false,
            right: false,
            bottom: false,
            left: false,
        }
    }

    /// Returns whether the top margin is automatic.
    #[must_use]
    pub const fn top(self) -> bool {
        self.top
    }

    /// Returns whether the right margin is automatic.
    #[must_use]
    pub const fn right(self) -> bool {
        self.right
    }

    /// Returns whether the bottom margin is automatic.
    #[must_use]
    pub const fn bottom(self) -> bool {
        self.bottom
    }

    /// Returns whether the left margin is automatic.
    #[must_use]
    pub const fn left(self) -> bool {
        self.left
    }

    /// Returns the number of automatic horizontal margins.
    #[must_use]
    pub const fn horizontal_count(self) -> u8 {
        (self.left as u8) + (self.right as u8)
    }

    /// Returns the number of automatic vertical margins.
    #[must_use]
    pub const fn vertical_count(self) -> u8 {
        (self.top as u8) + (self.bottom as u8)
    }
}

/// Immutable values computed for one DOM node.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    display: Display,
    position: Position,
    z_index: ZIndex,
    top: Length,
    right: Length,
    bottom: Length,
    left: Length,
    color: Rgba,
    background_color: Option<Rgba>,
    font_size: f32,
    font_weight: FontWeight,
    font_style: FontStyle,
    font_family: FontFamily,
    line_height: LineHeight,
    white_space: WhiteSpace,
    underline: bool,
    margin: EdgeSizes,
    margin_auto: AutoEdges,
    padding: EdgeSizes,
    border_width: EdgeSizes,
    border_color: Option<Rgba>,
    border_style: BorderStyle,
    border_radius: Length,
    overflow: Overflow,
    opacity: f32,
    box_sizing: BoxSizing,
    object_fit: ObjectFit,
    object_position: ObjectPosition,
    width: Length,
    min_width: Length,
    max_width: Length,
    height: Length,
    min_height: Length,
    max_height: Length,
    flex_direction: FlexDirection,
    flex_wrap: FlexWrap,
    justify_content: JustifyContent,
    align_items: AlignItems,
    align_content: AlignContent,
    align_self: AlignSelf,
    gap: Length,
    flex_grow: f32,
    flex_shrink: f32,
    flex_basis: Length,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            position: Position::Static,
            z_index: ZIndex::Auto,
            top: Length::Auto,
            right: Length::Auto,
            bottom: Length::Auto,
            left: Length::Auto,
            color: Rgba::new(0, 0, 0, 255),
            background_color: None,
            font_size: ROOT_FONT_SIZE_PX,
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            font_family: FontFamily::SansSerif,
            line_height: LineHeight::Normal,
            white_space: WhiteSpace::Normal,
            underline: false,
            margin: EdgeSizes::zero(),
            margin_auto: AutoEdges::none(),
            padding: EdgeSizes::zero(),
            border_width: EdgeSizes::new(3.0, 3.0, 3.0, 3.0),
            border_color: None,
            border_style: BorderStyle::None,
            border_radius: Length::Px(0.0),
            overflow: Overflow::Visible,
            opacity: 1.0,
            box_sizing: BoxSizing::ContentBox,
            object_fit: ObjectFit::Fill,
            object_position: ObjectPosition::default(),
            width: Length::Auto,
            min_width: Length::Auto,
            max_width: Length::Auto,
            height: Length::Auto,
            min_height: Length::Auto,
            max_height: Length::Auto,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            align_content: AlignContent::Stretch,
            align_self: AlignSelf::Auto,
            gap: Length::Px(0.0),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
        }
    }
}

impl ComputedStyle {
    /// Returns the computed display behavior.
    #[must_use]
    pub const fn display(&self) -> Display {
        self.display
    }

    /// Returns the computed positioning mode.
    #[must_use]
    pub const fn position(&self) -> Position {
        self.position
    }

    /// Returns the computed z-index value.
    #[must_use]
    pub const fn z_index(&self) -> ZIndex {
        self.z_index
    }

    /// Returns the computed physical top inset.
    #[must_use]
    pub const fn top(&self) -> Length {
        self.top
    }

    /// Returns the computed physical right inset.
    #[must_use]
    pub const fn right(&self) -> Length {
        self.right
    }

    /// Returns the computed physical bottom inset.
    #[must_use]
    pub const fn bottom(&self) -> Length {
        self.bottom
    }

    /// Returns the computed physical left inset.
    #[must_use]
    pub const fn left(&self) -> Length {
        self.left
    }

    /// Returns the computed foreground color.
    #[must_use]
    pub const fn color(&self) -> Rgba {
        self.color
    }

    /// Returns the computed background color.
    #[must_use]
    pub const fn background_color(&self) -> Option<Rgba> {
        self.background_color
    }

    /// Returns the computed font size in logical pixels.
    #[must_use]
    pub const fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Returns the computed font weight.
    #[must_use]
    pub const fn font_weight(&self) -> FontWeight {
        self.font_weight
    }

    /// Returns the computed font posture.
    #[must_use]
    pub const fn font_style(&self) -> FontStyle {
        self.font_style
    }

    /// Returns the coarse computed font family.
    #[must_use]
    pub const fn font_family(&self) -> FontFamily {
        self.font_family
    }

    /// Returns the computed line-height behavior.
    #[must_use]
    pub const fn line_height(&self) -> LineHeight {
        self.line_height
    }

    /// Returns the computed whitespace behavior.
    #[must_use]
    pub const fn white_space(&self) -> WhiteSpace {
        self.white_space
    }

    /// Resolves line height for the current font size.
    ///
    /// Phantom 2G-6 conservatively allows `line-height` to expand the line
    /// box while preserving the natural text metrics as a lower bound.
    #[must_use]
    pub fn used_line_height(&self, normal: f32) -> f32 {
        let normal = if normal.is_finite() && normal >= 0.0 {
            normal.min(MAX_NUMERIC_MAGNITUDE)
        } else {
            self.font_size.max(0.0)
        };

        match self.line_height {
            LineHeight::Normal => normal,
            LineHeight::Number(multiplier) => {
                bounded_computed(self.font_size * multiplier).unwrap_or(normal)
            }
            LineHeight::Px(value) if value.is_finite() => value.clamp(0.0, MAX_NUMERIC_MAGNITUDE),
            LineHeight::Px(_) => normal,
        }
    }

    /// Returns whether text should be underlined.
    #[must_use]
    pub const fn underline(&self) -> bool {
        self.underline
    }

    /// Returns the computed margins.
    #[must_use]
    pub const fn margin(&self) -> EdgeSizes {
        self.margin
    }

    /// Returns semantic `auto` state for physical margin edges.
    #[must_use]
    pub const fn margin_auto(&self) -> AutoEdges {
        self.margin_auto
    }

    /// Returns the computed padding.
    #[must_use]
    pub const fn padding(&self) -> EdgeSizes {
        self.padding
    }

    /// Returns effective border widths.
    ///
    /// A `none` border style contributes no geometry even when a width was
    /// declared earlier in the cascade.
    #[must_use]
    pub const fn border_width(&self) -> EdgeSizes {
        match self.border_style {
            BorderStyle::None => EdgeSizes::zero(),
            BorderStyle::Solid => self.border_width,
        }
    }

    /// Returns the effective border color.
    ///
    /// When no explicit border color was supplied, CSS `currentColor`
    /// semantics are approximated by the computed foreground color.
    #[must_use]
    pub const fn border_color(&self) -> Rgba {
        match self.border_color {
            Some(color) => color,
            None => self.color,
        }
    }

    /// Returns the computed border style.
    #[must_use]
    pub const fn border_style(&self) -> BorderStyle {
        self.border_style
    }

    /// Returns the computed uniform border radius.
    ///
    /// Percentages are retained for paint-time resolution against the box.
    #[must_use]
    pub const fn border_radius(&self) -> Length {
        self.border_radius
    }

    /// Returns the computed overflow painting mode.
    #[must_use]
    pub const fn overflow(&self) -> Overflow {
        self.overflow
    }

    /// Returns the computed element opacity in the inclusive range `0..=1`.
    #[must_use]
    pub const fn opacity(&self) -> f32 {
        self.opacity
    }

    /// Returns the box-sizing model used by layout.
    #[must_use]
    pub const fn box_sizing(&self) -> BoxSizing {
        self.box_sizing
    }

    /// Returns the replaced-element object fitting mode.
    #[must_use]
    pub const fn object_fit(&self) -> ObjectFit {
        self.object_fit
    }

    /// Returns the replaced-element object position.
    #[must_use]
    pub const fn object_position(&self) -> ObjectPosition {
        self.object_position
    }

    /// Returns the computed width.
    #[must_use]
    pub const fn width(&self) -> Length {
        self.width
    }

    /// Returns the computed minimum width.
    #[must_use]
    pub const fn min_width(&self) -> Length {
        self.min_width
    }

    /// Returns the computed maximum width.
    ///
    /// [`Length::Auto`] represents the initial unconstrained maximum.
    #[must_use]
    pub const fn max_width(&self) -> Length {
        self.max_width
    }

    /// Returns the computed height.
    #[must_use]
    pub const fn height(&self) -> Length {
        self.height
    }

    /// Returns the computed minimum height.
    #[must_use]
    pub const fn min_height(&self) -> Length {
        self.min_height
    }

    /// Returns the computed maximum height.
    ///
    /// [`Length::Auto`] represents the initial unconstrained maximum.
    #[must_use]
    pub const fn max_height(&self) -> Length {
        self.max_height
    }

    /// Returns the computed flex-direction.
    #[must_use]
    pub const fn flex_direction(&self) -> FlexDirection {
        self.flex_direction
    }

    /// Returns whether flex items stay on one line or may wrap.
    #[must_use]
    pub const fn flex_wrap(&self) -> FlexWrap {
        self.flex_wrap
    }

    /// Returns the computed main-axis distribution.
    #[must_use]
    pub const fn justify_content(&self) -> JustifyContent {
        self.justify_content
    }

    /// Returns the computed cross-axis alignment.
    #[must_use]
    pub const fn align_items(&self) -> AlignItems {
        self.align_items
    }

    /// Returns the computed distribution of multiple flex lines.
    #[must_use]
    pub const fn align_content(&self) -> AlignContent {
        self.align_content
    }

    /// Returns this item's cross-axis alignment override.
    #[must_use]
    pub const fn align_self(&self) -> AlignSelf {
        self.align_self
    }

    /// Returns the computed gap between direct flex items.
    #[must_use]
    pub const fn gap(&self) -> Length {
        self.gap
    }

    /// Returns the computed flex-grow factor.
    #[must_use]
    pub const fn flex_grow(&self) -> f32 {
        self.flex_grow
    }

    /// Returns the computed flex-shrink factor.
    #[must_use]
    pub const fn flex_shrink(&self) -> f32 {
        self.flex_shrink
    }

    /// Returns the computed flex-basis value.
    #[must_use]
    pub const fn flex_basis(&self) -> Length {
        self.flex_basis
    }
}

/// Immutable mapping from DOM nodes to interned computed styles.
///
/// Equivalent computed styles are stored once in a shared pool. Node lookups
/// use the numeric [`NodeId`] as a direct vector index instead of a tree map.
#[derive(Debug, Clone, Default)]
pub struct StyleMap {
    node_styles: Vec<usize>,
    styles: Vec<ComputedStyle>,
    node_count: usize,
}

impl StyleMap {
    /// Returns the computed style for one node.
    #[must_use]
    pub fn get(&self, node_id: NodeId) -> Option<&ComputedStyle> {
        let node_index = usize::try_from(node_id.as_u64()).ok()?;
        let style_index = *self.node_styles.get(node_index)?;

        if style_index == NO_STYLE {
            return None;
        }

        self.styles.get(style_index)
    }

    /// Returns the number of nodes that received a style.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.node_count
    }

    /// Returns the number of unique interned computed styles.
    #[must_use]
    pub fn unique_len(&self) -> usize {
        self.styles.len()
    }

    /// Returns `true` when no style snapshot exists.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.node_count == 0
    }

    fn insert(
        &mut self,
        node_id: NodeId,
        style: ComputedStyle,
        interner: &mut BTreeMap<StyleKey, usize>,
    ) {
        let key = StyleKey::from(&style);

        let style_index = if let Some(existing) = interner.get(&key) {
            *existing
        } else {
            let index = self.styles.len();
            self.styles.push(style);
            interner.insert(key, index);
            index
        };

        let Ok(node_index) = usize::try_from(node_id.as_u64()) else {
            return;
        };

        if self.node_styles.len() <= node_index {
            self.node_styles
                .resize(node_index.saturating_add(1), NO_STYLE);
        }

        if self.node_styles[node_index] == NO_STYLE {
            self.node_count = self.node_count.saturating_add(1);
        }

        self.node_styles[node_index] = style_index;
    }
}

/// Parsed stylesheet containing the CSS subset understood by Phantom.
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    rules: Vec<StyleRule>,
}

impl Stylesheet {
    /// Parses CSS using Phantom's own tolerant subset parser.
    ///
    /// This compatibility entry point uses Phantom's default desktop viewport.
    /// Call [`Stylesheet::parse_for_viewport`] when media queries must reflect
    /// an explicit browsing viewport.
    #[must_use]
    pub fn parse(source: &str) -> Self {
        Self::parse_for_viewport(
            source,
            DEFAULT_MEDIA_VIEWPORT_WIDTH,
            DEFAULT_MEDIA_VIEWPORT_HEIGHT,
        )
    }

    /// Parses CSS for an explicit viewport and activates the supported
    /// `@media` subset before selector/cascade evaluation.
    ///
    /// Supported media features in 2H-4 are `width`, `min-width`,
    /// `max-width`, `height`, `min-height`, `max-height`, and `orientation`.
    /// Media types `all` and `screen`, `only`, `not`, comma-separated OR, and
    /// `and` conjunction are handled. Lengths are bounded CSS `px`, `em`, or
    /// `rem` values; `em`/`rem` use the initial 16px root size in media queries.
    #[must_use]
    pub fn parse_for_viewport(source: &str, viewport_width: f32, viewport_height: f32) -> Self {
        if source.len() > MAX_CSS_SOURCE_BYTES {
            return Self::default();
        }

        let cleaned = remove_comments(source);
        let viewport = MediaViewport::new(viewport_width, viewport_height);
        let mut rules = Vec::new();
        let mut source_order = 0_u32;
        let mut scanned_blocks = 0_usize;

        append_rule_source(
            &cleaned,
            viewport,
            0,
            &mut scanned_blocks,
            &mut source_order,
            &mut rules,
        );

        Self { rules }
    }

    /// Returns the number of accepted selector rules.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Returns `true` when no supported rules were parsed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Computes an immutable, interned style snapshot for the document.
///
/// This compatibility entry point uses Phantom's default desktop viewport.
#[must_use]
pub fn compute_styles(document: &Document) -> StyleMap {
    compute_styles_with_viewport(
        document,
        DEFAULT_MEDIA_VIEWPORT_WIDTH,
        DEFAULT_MEDIA_VIEWPORT_HEIGHT,
    )
}

/// Computes styles for an explicit two-dimensional viewport.
///
/// The DOM is not mutated. Viewport-dependent `@media` rules are resolved
/// before cascade evaluation so a later viewport change can safely recascade
/// the existing document without reparsing HTML.
#[must_use]
pub fn compute_styles_with_viewport(
    document: &Document,
    viewport_width: f32,
    viewport_height: f32,
) -> StyleMap {
    let author_css = collect_author_css(document);
    let stylesheet = Stylesheet::parse_for_viewport(&author_css, viewport_width, viewport_height);

    let selector_context = SelectorMatchContext::build(document);
    let mut style_map = StyleMap::default();
    let mut interner = BTreeMap::new();
    let mut stack = vec![document.root()];
    let mut cascade_budget = MAX_CASCADE_RULE_EVALUATIONS;

    while let Some(node_id) = stack.pop() {
        let Some(node) = document.node(node_id) else {
            continue;
        };

        let parent_style = node
            .parent()
            .and_then(|parent| style_map.get(parent))
            .cloned();

        let mut style = initial_style(node, parent_style.as_ref());

        if matches!(node.kind(), NodeKind::Element(_)) {
            apply_cascade(
                document,
                node_id,
                &stylesheet,
                &selector_context,
                parent_style.as_ref(),
                &mut style,
                &mut cascade_budget,
            );
        }

        style_map.insert(node_id, style, &mut interner);

        for child in node.children().iter().rev() {
            stack.push(*child);
        }
    }

    style_map
}

#[derive(Debug, Clone, Copy)]
struct MediaViewport {
    width: f32,
    height: f32,
}

impl MediaViewport {
    fn new(width: f32, height: f32) -> Self {
        Self {
            width: sanitize_media_dimension(width),
            height: sanitize_media_dimension(height),
        }
    }
}

fn sanitize_media_dimension(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value.min(MAX_NUMERIC_MAGNITUDE)
    } else {
        1.0
    }
}

fn append_rule_source(
    source: &str,
    viewport: MediaViewport,
    depth: usize,
    scanned_blocks: &mut usize,
    source_order: &mut u32,
    rules: &mut Vec<StyleRule>,
) {
    if depth > MAX_MEDIA_NESTING
        || *scanned_blocks >= MAX_SCANNED_RULE_BLOCKS
        || rules.len() >= MAX_ACCEPTED_RULES
    {
        return;
    }

    for block in scan_rule_blocks(source) {
        if *scanned_blocks >= MAX_SCANNED_RULE_BLOCKS || rules.len() >= MAX_ACCEPTED_RULES {
            break;
        }
        *scanned_blocks = (*scanned_blocks).saturating_add(1);

        let prelude = block.selector.trim();
        let lower = prelude.to_ascii_lowercase();
        let media_tail = lower
            .starts_with("@media")
            .then(|| prelude.get(6..).unwrap_or_default());

        if let Some(media_tail) = media_tail
            && (media_tail.is_empty()
                || media_tail
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_whitespace() || character == '('))
        {
            if depth < MAX_MEDIA_NESTING {
                let condition = media_tail.trim();
                if media_query_list_matches(condition, viewport) {
                    append_rule_source(
                        block.body,
                        viewport,
                        depth.saturating_add(1),
                        scanned_blocks,
                        source_order,
                        rules,
                    );
                }
            }
            continue;
        }

        if prelude.starts_with('@') {
            continue;
        }

        let Some(selectors) = parse_selector_list(prelude) else {
            *source_order = (*source_order).saturating_add(1);
            continue;
        };

        let declarations = parse_declarations(block.body);
        if declarations.is_empty() {
            *source_order = (*source_order).saturating_add(1);
            continue;
        }

        for selector in selectors {
            if rules.len() >= MAX_ACCEPTED_RULES {
                break;
            }
            rules.push(StyleRule {
                specificity: selector.specificity(),
                selector,
                declarations: declarations.clone(),
                source_order: *source_order,
            });
        }

        *source_order = (*source_order).saturating_add(1);
    }
}

fn media_query_list_matches(source: &str, viewport: MediaViewport) -> bool {
    split_media_query_list(source)
        .into_iter()
        .any(|query| media_query_matches(query, viewport))
}

fn split_media_query_list(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0_usize;
    let mut cursor = 0_usize;
    let mut depth = 0_u32;
    let mut quote = None;
    let mut escaped = false;

    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'(' => depth = depth.saturating_add(1),
                b')' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => {
                    if parts.len() >= MAX_MEDIA_QUERIES_PER_LIST {
                        return parts;
                    }
                    let part = source[start..cursor].trim();
                    if !part.is_empty() {
                        parts.push(part);
                    }
                    start = cursor.saturating_add(1);
                }
                _ => {}
            }
        }
        cursor = cursor.saturating_add(1);
    }

    if parts.len() < MAX_MEDIA_QUERIES_PER_LIST {
        let tail = source[start..].trim();
        if !tail.is_empty() {
            parts.push(tail);
        }
    }
    parts
}

fn media_query_matches(source: &str, viewport: MediaViewport) -> bool {
    let mut query = source.trim();
    if query.is_empty() {
        return false;
    }

    let lower = query.to_ascii_lowercase();
    if lower.starts_with("only ") {
        query = query.get(5..).unwrap_or_default().trim_start();
    }

    let lower = query.to_ascii_lowercase();
    let (negated, query) = if lower.starts_with("not ") {
        (true, query.get(4..).unwrap_or_default().trim_start())
    } else {
        (false, query)
    };

    let first_condition = query.find('(');
    let (media_type, mut conditions) = match first_condition {
        Some(index) => {
            let mut prefix = query[..index].trim();
            if let Some(without_and) = strip_trailing_media_and(prefix) {
                prefix = without_and;
            }
            (prefix, &query[index..])
        }
        None => (query.trim(), ""),
    };

    let type_matches = match media_type.to_ascii_lowercase().as_str() {
        "" | "all" | "screen" => true,
        "print" | "speech" => false,
        _ => false,
    };

    let mut matches = type_matches;
    while matches && !conditions.trim().is_empty() {
        conditions = conditions.trim_start();
        if !conditions.starts_with('(') {
            matches = false;
            break;
        }

        let Some(close) = find_media_condition_close(conditions) else {
            matches = false;
            break;
        };
        let condition = &conditions[1..close];
        matches = media_condition_matches(condition, viewport);
        conditions = conditions[close.saturating_add(1)..].trim_start();

        if matches && !conditions.is_empty() {
            let lower_tail = conditions.to_ascii_lowercase();
            if lower_tail == "and" {
                matches = false;
                break;
            }
            if lower_tail.starts_with("and ") {
                conditions = conditions.get(3..).unwrap_or_default().trim_start();
            } else {
                matches = false;
            }
        }
    }

    if negated { !matches } else { matches }
}

fn strip_trailing_media_and(source: &str) -> Option<&str> {
    let trimmed = source.trim_end();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.ends_with("and") || trimmed.len() < 3 {
        return None;
    }

    let prefix = &trimmed[..trimmed.len().saturating_sub(3)];
    if prefix.is_empty()
        || prefix
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        Some(prefix.trim_end())
    } else {
        None
    }
}

fn find_media_condition_close(source: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0_u32;
    for (index, byte) in bytes.iter().copied().enumerate() {
        match byte {
            b'(' => depth = depth.saturating_add(1),
            b')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn media_condition_matches(source: &str, viewport: MediaViewport) -> bool {
    let Some((name, value)) = source.split_once(':') else {
        return false;
    };
    let name = name.trim().to_ascii_lowercase();
    let value = value.trim();

    match name.as_str() {
        "width" => parse_media_length(value).is_some_and(|length| media_eq(viewport.width, length)),
        "min-width" => parse_media_length(value).is_some_and(|length| viewport.width >= length),
        "max-width" => parse_media_length(value).is_some_and(|length| viewport.width <= length),
        "height" => {
            parse_media_length(value).is_some_and(|length| media_eq(viewport.height, length))
        }
        "min-height" => parse_media_length(value).is_some_and(|length| viewport.height >= length),
        "max-height" => parse_media_length(value).is_some_and(|length| viewport.height <= length),
        "orientation" => match value.to_ascii_lowercase().as_str() {
            "portrait" => viewport.height >= viewport.width,
            "landscape" => viewport.width > viewport.height,
            _ => false,
        },
        _ => false,
    }
}

fn media_eq(left: f32, right: f32) -> bool {
    (left - right).abs() <= 0.01
}

fn parse_media_length(source: &str) -> Option<f32> {
    let value = source.trim().to_ascii_lowercase();
    if value == "0" {
        return Some(0.0);
    }

    let (number, scale) = if let Some(number) = value.strip_suffix("px") {
        (number, 1.0)
    } else if let Some(number) = value.strip_suffix("rem") {
        (number, ROOT_FONT_SIZE_PX)
    } else if let Some(number) = value.strip_suffix("em") {
        (number, ROOT_FONT_SIZE_PX)
    } else {
        return None;
    };

    let number = number.trim().parse::<f32>().ok()?;
    let result = number * scale;
    (result.is_finite() && (0.0..=MAX_NUMERIC_MAGNITUDE).contains(&result)).then_some(result)
}

#[derive(Debug, Clone)]
struct StyleRule {
    selector: Selector,
    declarations: Vec<Declaration>,
    specificity: Specificity,
    source_order: u32,
}

#[derive(Debug, Clone)]
struct Declaration {
    property: Property,
    value: SpecifiedValue,
    important: bool,
    declaration_order: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Specificity {
    ids: u16,
    classes: u16,
    tags: u16,
}

impl Specificity {
    const INLINE: Self = Self {
        ids: u16::MAX,
        classes: u16::MAX,
        tags: u16::MAX,
    };

    const fn zero() -> Self {
        Self {
            ids: 0,
            classes: 0,
            tags: 0,
        }
    }

    fn add(self, other: Self) -> Self {
        Self {
            ids: self.ids.saturating_add(other.ids),
            classes: self.classes.saturating_add(other.classes),
            tags: self.tags.saturating_add(other.tags),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CascadePriority {
    important: bool,
    inline: bool,
    specificity: Specificity,
    source_order: u32,
    declaration_order: u16,
}

#[derive(Debug, Clone)]
struct CascadeWinner {
    priority: CascadePriority,
    value: SpecifiedValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Property {
    Display,
    Color,
    BackgroundColor,
    FontSize,
    FontWeight,
    FontStyle,
    FontFamily,
    TextDecoration,
    MarginTop,
    MarginRight,
    MarginBottom,
    MarginLeft,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    BorderTopWidth,
    BorderRightWidth,
    BorderBottomWidth,
    BorderLeftWidth,
    BorderColor,
    BorderStyle,
    BoxSizing,
    ObjectFit,
    ObjectPosition,
    Width,
    MinWidth,
    MaxWidth,
    Height,
    MinHeight,
    MaxHeight,
    FlexDirection,
    FlexWrap,
    JustifyContent,
    AlignItems,
    AlignContent,
    AlignSelf,
    Gap,
    FlexGrow,
    FlexShrink,
    FlexBasis,
    LineHeight,
    WhiteSpace,
    Position,
    Top,
    Right,
    Bottom,
    Left,
    ZIndex,
    BorderRadius,
    Overflow,
    Opacity,
}

impl Property {
    const COUNT: usize = 52;

    const fn index(self) -> usize {
        match self {
            Self::Display => 0,
            Self::Color => 1,
            Self::BackgroundColor => 2,
            Self::FontSize => 3,
            Self::FontWeight => 4,
            Self::FontStyle => 5,
            Self::FontFamily => 6,
            Self::TextDecoration => 7,
            Self::MarginTop => 8,
            Self::MarginRight => 9,
            Self::MarginBottom => 10,
            Self::MarginLeft => 11,
            Self::PaddingTop => 12,
            Self::PaddingRight => 13,
            Self::PaddingBottom => 14,
            Self::PaddingLeft => 15,
            Self::BorderTopWidth => 16,
            Self::BorderRightWidth => 17,
            Self::BorderBottomWidth => 18,
            Self::BorderLeftWidth => 19,
            Self::BorderColor => 20,
            Self::BorderStyle => 21,
            Self::BoxSizing => 22,
            Self::ObjectFit => 23,
            Self::ObjectPosition => 24,
            Self::Width => 25,
            Self::MinWidth => 26,
            Self::MaxWidth => 27,
            Self::Height => 28,
            Self::MinHeight => 29,
            Self::MaxHeight => 30,
            Self::FlexDirection => 31,
            Self::FlexWrap => 32,
            Self::JustifyContent => 33,
            Self::AlignItems => 34,
            Self::AlignContent => 35,
            Self::AlignSelf => 36,
            Self::Gap => 37,
            Self::FlexGrow => 38,
            Self::FlexShrink => 39,
            Self::FlexBasis => 40,
            Self::LineHeight => 41,
            Self::WhiteSpace => 42,
            Self::Position => 43,
            Self::Top => 44,
            Self::Right => 45,
            Self::Bottom => 46,
            Self::Left => 47,
            Self::ZIndex => 48,
            Self::BorderRadius => 49,
            Self::Overflow => 50,
            Self::Opacity => 51,
        }
    }

    const fn is_inherited(self) -> bool {
        matches!(
            self,
            Self::Color
                | Self::FontSize
                | Self::FontWeight
                | Self::FontStyle
                | Self::FontFamily
                | Self::LineHeight
                | Self::WhiteSpace
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CssWideKeyword {
    Inherit,
    Initial,
    Unset,
}

#[derive(Debug, Clone)]
enum SpecifiedValue {
    CssWide(CssWideKeyword),
    Display(Display),
    Color(Rgba),
    BackgroundColor(Option<Rgba>),
    FontSize(SpecifiedLength),
    LineHeight(SpecifiedLineHeight),
    WhiteSpace(WhiteSpace),
    Position(Position),
    ZIndex(ZIndex),
    OffsetLength(SpecifiedLength),
    FontWeight(FontWeight),
    FontStyle(FontStyle),
    FontFamily(FontFamily),
    Underline(bool),
    MarginLength(SpecifiedLength),
    EdgeLength(SpecifiedLength),
    BorderColor(Option<Rgba>),
    BorderStyle(BorderStyle),
    BorderRadius(SpecifiedLength),
    Overflow(Overflow),
    Opacity(f32),
    BoxSizing(BoxSizing),
    ObjectFit(ObjectFit),
    ObjectPosition(ObjectPosition),
    SizeLength(SpecifiedLength),
    FlexDirection(FlexDirection),
    FlexWrap(FlexWrap),
    JustifyContent(JustifyContent),
    AlignItems(AlignItems),
    AlignContent(AlignContent),
    AlignSelf(AlignSelf),
    Number(f32),
}

#[derive(Debug, Clone, Copy)]
enum SpecifiedLineHeight {
    Normal,
    Number(f32),
    Length(SpecifiedLength),
    Percent(f32),
}

#[derive(Debug, Clone, Copy)]
enum SpecifiedLength {
    Auto,
    Px(f32),
    Em(f32),
    Rem(f32),
    Percent(f32),
}

// PHANTOM_2H3_CSS_COMPATIBILITY_I
#[derive(Debug, Clone, Default)]
struct SelectorMatchContext {
    sibling_positions: Vec<(u32, u32)>,
    type_sibling_positions: Vec<(u32, u32)>,
}

impl SelectorMatchContext {
    fn build(document: &Document) -> Self {
        let mut sibling_positions = vec![(0, 0); document.len()];
        let mut type_sibling_positions = vec![(0, 0); document.len()];

        for parent in document.nodes() {
            let element_children = parent
                .children()
                .iter()
                .filter_map(|child_id| {
                    let child = document.node(*child_id)?;
                    let NodeKind::Element(element) = child.kind() else {
                        return None;
                    };

                    Some((*child_id, element.tag_name()))
                })
                .collect::<Vec<_>>();

            let Ok(total) = u32::try_from(element_children.len()) else {
                continue;
            };

            let mut type_totals = BTreeMap::<&str, u32>::new();
            for (_, tag_name) in &element_children {
                let entry = type_totals.entry(*tag_name).or_insert(0);
                *entry = entry.saturating_add(1);
            }

            let mut type_seen = BTreeMap::<&str, u32>::new();

            for (offset, (child_id, tag_name)) in element_children.into_iter().enumerate() {
                let Ok(node_index) = usize::try_from(child_id.as_u64()) else {
                    continue;
                };
                if sibling_positions.len() <= node_index {
                    sibling_positions.resize(node_index.saturating_add(1), (0, 0));
                    type_sibling_positions.resize(node_index.saturating_add(1), (0, 0));
                }

                let Ok(index) = u32::try_from(offset.saturating_add(1)) else {
                    continue;
                };
                sibling_positions[node_index] = (index, total);

                let type_index = type_seen.entry(tag_name).or_insert(0);
                *type_index = type_index.saturating_add(1);
                let type_total = type_totals.get(tag_name).copied().unwrap_or(0);
                type_sibling_positions[node_index] = (*type_index, type_total);
            }
        }

        Self {
            sibling_positions,
            type_sibling_positions,
        }
    }

    fn sibling_position(&self, node_id: NodeId) -> Option<(u32, u32)> {
        position_for(&self.sibling_positions, node_id)
    }

    fn type_sibling_position(&self, node_id: NodeId) -> Option<(u32, u32)> {
        position_for(&self.type_sibling_positions, node_id)
    }
}

fn position_for(positions: &[(u32, u32)], node_id: NodeId) -> Option<(u32, u32)> {
    let node_index = usize::try_from(node_id.as_u64()).ok()?;
    let position = *positions.get(node_index)?;

    if position.0 == 0 {
        None
    } else {
        Some(position)
    }
}

#[derive(Debug, Clone)]
struct Selector {
    parts: Vec<SelectorPart>,
}

#[derive(Debug, Clone)]
struct SelectorPart {
    compound: CompoundSelector,
    combinator_to_left: Option<Combinator>,
}

#[derive(Debug, Clone, Copy)]
enum Combinator {
    Descendant,
    Child,
    AdjacentSibling,
}

// PHANTOM_2G8_ATTRIBUTE_SELECTOR_OPERATORS
#[derive(Debug, Clone)]
enum AttributeSelector {
    Exists(String),
    Equals { name: String, value: String },
    Includes { name: String, value: String },
    DashMatch { name: String, value: String },
    Prefix { name: String, value: String },
    Suffix { name: String, value: String },
    Substring { name: String, value: String },
}

impl AttributeSelector {
    fn matches(&self, element: &ElementData) -> bool {
        match self {
            Self::Exists(name) => element.attribute(name).is_some(),
            Self::Equals { name, value } => element.attribute(name) == Some(value.as_str()),
            Self::Includes { name, value } => element.attribute(name).is_some_and(|candidate| {
                !value.is_empty()
                    && candidate
                        .split_ascii_whitespace()
                        .any(|token| token == value)
            }),
            Self::DashMatch { name, value } => element.attribute(name).is_some_and(|candidate| {
                candidate == value
                    || candidate
                        .strip_prefix(value)
                        .is_some_and(|suffix| suffix.starts_with('-'))
            }),
            Self::Prefix { name, value } => element
                .attribute(name)
                .is_some_and(|candidate| !value.is_empty() && candidate.starts_with(value)),
            Self::Suffix { name, value } => element
                .attribute(name)
                .is_some_and(|candidate| !value.is_empty() && candidate.ends_with(value)),
            Self::Substring { name, value } => element
                .attribute(name)
                .is_some_and(|candidate| !value.is_empty() && candidate.contains(value)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum StructuralPseudoClass {
    Root,
    Empty,
    First,
    Last,
    Only,
    Nth(u32),
    FirstOfType,
    LastOfType,
    OnlyOfType,
    NthOfType(u32),
}

impl StructuralPseudoClass {
    fn matches(self, document: &Document, context: &SelectorMatchContext, node_id: NodeId) -> bool {
        match self {
            Self::Root => parent_element(document, node_id).is_none(),
            Self::Empty => element_is_empty(document, node_id),
            Self::First => context
                .sibling_position(node_id)
                .is_some_and(|(index, _)| index == 1),
            Self::Last => context
                .sibling_position(node_id)
                .is_some_and(|(index, total)| index == total),
            Self::Only => context
                .sibling_position(node_id)
                .is_some_and(|(_, total)| total == 1),
            Self::Nth(expected) => context
                .sibling_position(node_id)
                .is_some_and(|(index, _)| index == expected),
            Self::FirstOfType => context
                .type_sibling_position(node_id)
                .is_some_and(|(index, _)| index == 1),
            Self::LastOfType => context
                .type_sibling_position(node_id)
                .is_some_and(|(index, total)| index == total),
            Self::OnlyOfType => context
                .type_sibling_position(node_id)
                .is_some_and(|(_, total)| total == 1),
            Self::NthOfType(expected) => context
                .type_sibling_position(node_id)
                .is_some_and(|(index, _)| index == expected),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct CompoundSelector {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attributes: Vec<AttributeSelector>,
    pseudo_classes: Vec<StructuralPseudoClass>,
    universal: bool,
}

impl Selector {
    fn parse(source: &str) -> Option<Self> {
        let source = source.trim();

        if source.len() > MAX_SELECTOR_BYTES || source.is_empty() || source.starts_with('@') {
            return None;
        }

        let parts = parse_selector_parts(source)?;

        if parts.is_empty() {
            None
        } else {
            Some(Self { parts })
        }
    }

    fn specificity(&self) -> Specificity {
        self.parts
            .iter()
            .fold(Specificity::zero(), |specificity, part| {
                specificity.add(part.compound.specificity())
            })
    }

    fn matches(
        &self,
        document: &Document,
        node_id: NodeId,
        context: &SelectorMatchContext,
    ) -> bool {
        let Some(last_index) = self.parts.len().checked_sub(1) else {
            return false;
        };

        let mut current = node_id;

        if !self.parts[last_index]
            .compound
            .matches_node(document, current, context)
        {
            return false;
        }

        for part_index in (1..=last_index).rev() {
            let left = &self.parts[part_index - 1];
            let Some(combinator) = self.parts[part_index].combinator_to_left else {
                return false;
            };

            match combinator {
                Combinator::Child => {
                    let Some(parent) = parent_element(document, current) else {
                        return false;
                    };

                    if !left.compound.matches_node(document, parent, context) {
                        return false;
                    }

                    current = parent;
                }

                Combinator::Descendant => {
                    let Some(ancestor) =
                        matching_ancestor(document, current, &left.compound, context)
                    else {
                        return false;
                    };

                    current = ancestor;
                }

                Combinator::AdjacentSibling => {
                    let Some(sibling) = previous_element_sibling(document, current) else {
                        return false;
                    };

                    if !left.compound.matches_node(document, sibling, context) {
                        return false;
                    }

                    current = sibling;
                }
            }
        }

        true
    }
}

impl CompoundSelector {
    fn parse(source: &str) -> Option<Self> {
        let source = source.trim();

        if source.is_empty() {
            return None;
        }

        let bytes = source.as_bytes();
        let mut cursor = 0;
        let mut selector = Self::default();

        if bytes.first().is_some_and(|byte| *byte == b'*') {
            selector.universal = true;
            cursor += 1;
        } else if bytes
            .first()
            .is_some_and(|byte| !matches!(*byte, b'.' | b'#' | b'[' | b':'))
        {
            let tag = parse_selector_identifier(source, &mut cursor)?;
            selector.tag = Some(tag.to_ascii_lowercase());
        }

        while cursor < bytes.len() {
            match bytes[cursor] {
                b'.' => {
                    if selector.classes.len() >= MAX_CLASSES_PER_COMPOUND {
                        return None;
                    }

                    cursor += 1;
                    selector
                        .classes
                        .push(parse_selector_identifier(source, &mut cursor)?);
                }

                b'#' => {
                    if selector.id.is_some() {
                        return None;
                    }

                    cursor += 1;
                    selector.id = Some(parse_selector_identifier(source, &mut cursor)?);
                }

                b'[' => {
                    if selector.attributes.len() >= MAX_ATTRIBUTE_SELECTORS_PER_COMPOUND {
                        return None;
                    }

                    selector
                        .attributes
                        .push(parse_attribute_selector(source, &mut cursor)?);
                }

                b':' => {
                    if selector.pseudo_classes.len() >= MAX_PSEUDO_CLASSES_PER_COMPOUND {
                        return None;
                    }

                    selector
                        .pseudo_classes
                        .push(parse_structural_pseudo_class(source, &mut cursor)?);
                }

                _ => return None,
            }
        }

        if selector.universal
            || selector.tag.is_some()
            || selector.id.is_some()
            || !selector.classes.is_empty()
            || !selector.attributes.is_empty()
            || !selector.pseudo_classes.is_empty()
        {
            Some(selector)
        } else {
            None
        }
    }

    fn specificity(&self) -> Specificity {
        let class_like = self
            .classes
            .len()
            .saturating_add(self.attributes.len())
            .saturating_add(self.pseudo_classes.len());

        Specificity {
            ids: u16::from(self.id.is_some()),
            classes: u16::try_from(class_like).unwrap_or(u16::MAX),
            tags: u16::from(self.tag.is_some()),
        }
    }

    fn matches_node(
        &self,
        document: &Document,
        node_id: NodeId,
        context: &SelectorMatchContext,
    ) -> bool {
        let Some(node) = document.node(node_id) else {
            return false;
        };

        let NodeKind::Element(element) = node.kind() else {
            return false;
        };

        if !self.matches_element(element) {
            return false;
        }

        self.pseudo_classes
            .iter()
            .all(|pseudo| pseudo.matches(document, context, node_id))
    }

    fn matches_element(&self, element: &ElementData) -> bool {
        if let Some(tag) = &self.tag
            && element.tag_name() != tag
        {
            return false;
        }

        if let Some(id) = &self.id
            && element.attribute("id") != Some(id.as_str())
        {
            return false;
        }

        let element_classes = element.attribute("class").unwrap_or_default();

        if !self.classes.iter().all(|class| {
            element_classes
                .split_ascii_whitespace()
                .any(|candidate| candidate == class)
        }) {
            return false;
        }

        self.attributes
            .iter()
            .all(|attribute| attribute.matches(element))
    }
}

#[derive(Debug, Clone, Copy)]
struct RuleBlock<'a> {
    selector: &'a str,
    body: &'a str,
}

fn parse_selector_list(source: &str) -> Option<Vec<Selector>> {
    let source = source.trim();

    if source.is_empty() || source.len() > MAX_SELECTOR_BYTES {
        return None;
    }

    let selector_sources = split_selector_list(source)?;
    let mut selectors = Vec::with_capacity(selector_sources.len());

    for selector_source in selector_sources {
        if selectors.len() >= MAX_ACCEPTED_RULES {
            return None;
        }

        selectors.push(Selector::parse(selector_source)?);
    }

    if selectors.is_empty() {
        None
    } else {
        Some(selectors)
    }
}

fn split_selector_list(source: &str) -> Option<Vec<&str>> {
    let bytes = source.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut cursor = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut brackets = 0_u32;
    let mut parentheses = 0_u32;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'[' => brackets = brackets.saturating_add(1),
                b']' => {
                    if brackets == 0 {
                        return None;
                    }
                    brackets -= 1;
                }
                b'(' => parentheses = parentheses.saturating_add(1),
                b')' => {
                    if parentheses == 0 {
                        return None;
                    }
                    parentheses -= 1;
                }
                b',' if brackets == 0 && parentheses == 0 => {
                    let part = source[start..cursor].trim();
                    if part.is_empty() {
                        return None;
                    }
                    parts.push(part);
                    start = cursor.saturating_add(1);
                }
                _ => {}
            }
        }

        cursor += 1;
    }

    if quote.is_some() || brackets != 0 || parentheses != 0 {
        return None;
    }

    let last = source[start..].trim();
    if last.is_empty() {
        return None;
    }
    parts.push(last);

    Some(parts)
}

fn parse_selector_parts(source: &str) -> Option<Vec<SelectorPart>> {
    let mut parts = Vec::new();
    let mut buffer = String::new();
    let mut pending = None;
    let mut invalid = false;
    let mut quote = None;
    let mut escaped = false;
    let mut brackets = 0_u32;
    let mut parentheses = 0_u32;

    for character in source.chars() {
        if let Some(active_quote) = quote {
            buffer.push(character);

            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }

        match character {
            '\'' | '"' => {
                quote = Some(character);
                buffer.push(character);
            }

            '[' => {
                brackets = brackets.saturating_add(1);
                buffer.push(character);
            }

            ']' => {
                if brackets == 0 {
                    invalid = true;
                    break;
                }
                brackets -= 1;
                buffer.push(character);
            }

            '(' => {
                parentheses = parentheses.saturating_add(1);
                buffer.push(character);
            }

            ')' => {
                if parentheses == 0 {
                    invalid = true;
                    break;
                }
                parentheses -= 1;
                buffer.push(character);
            }

            '>' if brackets == 0 && parentheses == 0 => {
                if !flush_selector_part(&mut parts, &mut buffer, &mut pending) {
                    invalid = true;
                    break;
                }

                if parts.is_empty() || matches!(pending, Some(Combinator::Child)) {
                    invalid = true;
                    break;
                }

                pending = Some(Combinator::Child);
            }

            '+' if brackets == 0 && parentheses == 0 => {
                if !flush_selector_part(&mut parts, &mut buffer, &mut pending) {
                    invalid = true;
                    break;
                }

                if parts.is_empty()
                    || matches!(
                        pending,
                        Some(Combinator::Child | Combinator::AdjacentSibling)
                    )
                {
                    invalid = true;
                    break;
                }

                pending = Some(Combinator::AdjacentSibling);
            }

            '~' if brackets == 0 && parentheses == 0 => {
                invalid = true;
                break;
            }

            character if character.is_whitespace() && brackets == 0 && parentheses == 0 => {
                if !buffer.is_empty() && !flush_selector_part(&mut parts, &mut buffer, &mut pending)
                {
                    invalid = true;
                    break;
                }

                if !parts.is_empty() && pending.is_none() {
                    pending = Some(Combinator::Descendant);
                }
            }

            _ => {
                if buffer.len() >= MAX_SELECTOR_BYTES {
                    return None;
                }

                buffer.push(character);
            }
        }
    }

    if quote.is_some() || brackets != 0 || parentheses != 0 {
        return None;
    }

    if invalid || !flush_selector_part(&mut parts, &mut buffer, &mut pending) {
        return None;
    }

    if pending.is_some() || parts.len() > MAX_SELECTOR_PARTS {
        return None;
    }

    Some(parts)
}

fn flush_selector_part(
    parts: &mut Vec<SelectorPart>,
    buffer: &mut String,
    pending: &mut Option<Combinator>,
) -> bool {
    if buffer.is_empty() {
        return true;
    }

    let Some(compound) = CompoundSelector::parse(buffer) else {
        return false;
    };

    let combinator_to_left = if parts.is_empty() {
        None
    } else {
        pending.take().or(Some(Combinator::Descendant))
    };

    parts.push(SelectorPart {
        compound,
        combinator_to_left,
    });

    buffer.clear();

    true
}

fn parse_selector_identifier(source: &str, cursor: &mut usize) -> Option<String> {
    let bytes = source.as_bytes();
    let start = *cursor;

    while *cursor < bytes.len() && is_identifier_byte(bytes[*cursor]) {
        *cursor += 1;
    }

    if *cursor == start {
        None
    } else {
        Some(source[start..*cursor].to_owned())
    }
}

fn parse_attribute_selector(source: &str, cursor: &mut usize) -> Option<AttributeSelector> {
    let bytes = source.as_bytes();

    if bytes.get(*cursor) != Some(&b'[') {
        return None;
    }
    *cursor += 1;
    skip_selector_whitespace(bytes, cursor);

    let name = parse_selector_identifier(source, cursor)?.to_ascii_lowercase();
    skip_selector_whitespace(bytes, cursor);

    if bytes.get(*cursor) == Some(&b']') {
        *cursor += 1;
        return Some(AttributeSelector::Exists(name));
    }

    #[derive(Clone, Copy)]
    enum Operator {
        Equals,
        Includes,
        DashMatch,
        Prefix,
        Suffix,
        Substring,
    }

    let operator = match (
        bytes.get(*cursor).copied(),
        bytes.get(cursor.saturating_add(1)).copied(),
    ) {
        (Some(b'='), _) => {
            *cursor += 1;
            Operator::Equals
        }
        (Some(b'~'), Some(b'=')) => {
            *cursor += 2;
            Operator::Includes
        }
        (Some(b'|'), Some(b'=')) => {
            *cursor += 2;
            Operator::DashMatch
        }
        (Some(b'^'), Some(b'=')) => {
            *cursor += 2;
            Operator::Prefix
        }
        (Some(b'$'), Some(b'=')) => {
            *cursor += 2;
            Operator::Suffix
        }
        (Some(b'*'), Some(b'=')) => {
            *cursor += 2;
            Operator::Substring
        }
        _ => return None,
    };

    skip_selector_whitespace(bytes, cursor);

    let value = match bytes.get(*cursor).copied() {
        Some(b'\'') | Some(b'"') => parse_quoted_selector_value(source, cursor)?,
        Some(_) => parse_unquoted_selector_value(source, cursor)?,
        None => return None,
    };

    if matches!(operator, Operator::Includes) && value.split_ascii_whitespace().count() != 1 {
        return None;
    }

    skip_selector_whitespace(bytes, cursor);
    if bytes.get(*cursor) != Some(&b']') {
        return None;
    }
    *cursor += 1;

    Some(match operator {
        Operator::Equals => AttributeSelector::Equals { name, value },
        Operator::Includes => AttributeSelector::Includes { name, value },
        Operator::DashMatch => AttributeSelector::DashMatch { name, value },
        Operator::Prefix => AttributeSelector::Prefix { name, value },
        Operator::Suffix => AttributeSelector::Suffix { name, value },
        Operator::Substring => AttributeSelector::Substring { name, value },
    })
}

fn parse_quoted_selector_value(source: &str, cursor: &mut usize) -> Option<String> {
    let bytes = source.as_bytes();
    let quote = *bytes.get(*cursor)?;
    *cursor += 1;

    let mut value = String::new();

    while *cursor < bytes.len() {
        if bytes[*cursor] == quote {
            *cursor += 1;
            return Some(value);
        }

        if bytes[*cursor] == b'\\' {
            *cursor += 1;
            if *cursor >= bytes.len() {
                return None;
            }
        }

        let character = source[*cursor..].chars().next()?;
        value.push(character);
        *cursor += character.len_utf8();
    }

    None
}

fn parse_unquoted_selector_value(source: &str, cursor: &mut usize) -> Option<String> {
    let bytes = source.as_bytes();
    let start = *cursor;

    while *cursor < bytes.len() {
        let byte = bytes[*cursor];

        if byte.is_ascii_whitespace() || byte == b']' {
            break;
        }

        if matches!(
            byte,
            b'\'' | b'"' | b'=' | b'~' | b'|' | b'^' | b'$' | b'*' | b'[' | b'(' | b')' | b'\\'
        ) {
            return None;
        }

        *cursor += 1;
    }

    if *cursor == start {
        None
    } else {
        Some(source[start..*cursor].to_owned())
    }
}

fn parse_structural_pseudo_class(
    source: &str,
    cursor: &mut usize,
) -> Option<StructuralPseudoClass> {
    let bytes = source.as_bytes();

    if bytes.get(*cursor) != Some(&b':') {
        return None;
    }
    *cursor += 1;

    let name = parse_selector_identifier(source, cursor)?.to_ascii_lowercase();

    match name.as_str() {
        "root" => Some(StructuralPseudoClass::Root),
        "empty" => Some(StructuralPseudoClass::Empty),
        "first-child" => Some(StructuralPseudoClass::First),
        "last-child" => Some(StructuralPseudoClass::Last),
        "only-child" => Some(StructuralPseudoClass::Only),
        "first-of-type" => Some(StructuralPseudoClass::FirstOfType),
        "last-of-type" => Some(StructuralPseudoClass::LastOfType),
        "only-of-type" => Some(StructuralPseudoClass::OnlyOfType),
        "nth-child" => parse_nth_index(source, cursor).map(StructuralPseudoClass::Nth),
        "nth-of-type" => parse_nth_index(source, cursor).map(StructuralPseudoClass::NthOfType),
        _ => None,
    }
}

fn parse_nth_index(source: &str, cursor: &mut usize) -> Option<u32> {
    let bytes = source.as_bytes();

    if bytes.get(*cursor) != Some(&b'(') {
        return None;
    }
    *cursor += 1;
    let argument_start = *cursor;

    while *cursor < bytes.len() && bytes[*cursor] != b')' {
        *cursor += 1;
    }

    if bytes.get(*cursor) != Some(&b')') {
        return None;
    }

    let argument = source[argument_start..*cursor].trim();
    *cursor += 1;

    if argument.is_empty() || !argument.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    let index = argument.parse::<usize>().ok()?;
    if index == 0 || index > MAX_DOM_NODES {
        return None;
    }

    u32::try_from(index).ok()
}

fn skip_selector_whitespace(bytes: &[u8], cursor: &mut usize) {
    while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
}

fn parent_element(document: &Document, node_id: NodeId) -> Option<NodeId> {
    let mut parent = document.node(node_id)?.parent();

    while let Some(parent_id) = parent {
        let node = document.node(parent_id)?;

        if matches!(node.kind(), NodeKind::Element(_)) {
            return Some(parent_id);
        }

        parent = node.parent();
    }

    None
}

fn matching_ancestor(
    document: &Document,
    node_id: NodeId,
    selector: &CompoundSelector,
    context: &SelectorMatchContext,
) -> Option<NodeId> {
    let mut ancestor = parent_element(document, node_id);

    while let Some(ancestor_id) = ancestor {
        if selector.matches_node(document, ancestor_id, context) {
            return Some(ancestor_id);
        }

        ancestor = parent_element(document, ancestor_id);
    }

    None
}

fn previous_element_sibling(document: &Document, node_id: NodeId) -> Option<NodeId> {
    let parent_id = document.node(node_id)?.parent()?;
    let parent = document.node(parent_id)?;
    let mut previous = None;

    for child_id in parent.children() {
        if *child_id == node_id {
            return previous;
        }

        if document
            .node(*child_id)
            .is_some_and(|child| matches!(child.kind(), NodeKind::Element(_)))
        {
            previous = Some(*child_id);
        }
    }

    None
}

fn element_is_empty(document: &Document, node_id: NodeId) -> bool {
    let Some(node) = document.node(node_id) else {
        return false;
    };

    node.children().iter().all(|child_id| {
        document
            .node(*child_id)
            .is_none_or(|child| match child.kind() {
                NodeKind::Element(_) => false,
                NodeKind::Text(text) => text.is_empty(),
                _ => true,
            })
    })
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

fn apply_cascade(
    document: &Document,
    node_id: NodeId,
    stylesheet: &Stylesheet,
    selector_context: &SelectorMatchContext,
    parent_style: Option<&ComputedStyle>,
    style: &mut ComputedStyle,
    cascade_budget: &mut usize,
) {
    let mut winners: [Option<CascadeWinner>; Property::COUNT] = array::from_fn(|_| None);

    for rule in &stylesheet.rules {
        if *cascade_budget == 0 {
            break;
        }
        *cascade_budget = cascade_budget.saturating_sub(1);

        if !rule.selector.matches(document, node_id, selector_context) {
            continue;
        }

        for declaration in &rule.declarations {
            let priority = CascadePriority {
                important: declaration.important,
                inline: false,
                specificity: rule.specificity,
                source_order: rule.source_order,
                declaration_order: declaration.declaration_order,
            };

            consider_winner(
                &mut winners,
                declaration.property,
                declaration.value.clone(),
                priority,
            );
        }
    }

    if let Some(element) = document.node(node_id).and_then(|node| {
        if let NodeKind::Element(element) = node.kind() {
            Some(element)
        } else {
            None
        }
    }) && let Some(inline_style) = element.attribute("style")
    {
        if inline_style.len() > MAX_INLINE_STYLE_BYTES {
            apply_winners(style, parent_style, &winners);
            return;
        }
        let declarations = parse_declarations(inline_style);

        for declaration in declarations {
            let priority = CascadePriority {
                important: declaration.important,
                inline: true,
                specificity: Specificity::INLINE,
                source_order: u32::MAX,
                declaration_order: declaration.declaration_order,
            };

            consider_winner(
                &mut winners,
                declaration.property,
                declaration.value,
                priority,
            );
        }
    }

    apply_winners(style, parent_style, &winners);
}

fn consider_winner(
    winners: &mut [Option<CascadeWinner>; Property::COUNT],
    property: Property,
    value: SpecifiedValue,
    priority: CascadePriority,
) {
    let slot = &mut winners[property.index()];

    let should_replace = slot
        .as_ref()
        .is_none_or(|winner| priority >= winner.priority);

    if should_replace {
        *slot = Some(CascadeWinner { priority, value });
    }
}

fn apply_winners(
    style: &mut ComputedStyle,
    parent_style: Option<&ComputedStyle>,
    winners: &[Option<CascadeWinner>; Property::COUNT],
) {
    for property in [
        Property::Display,
        Property::Color,
        Property::BackgroundColor,
        Property::FontSize,
        Property::LineHeight,
        Property::WhiteSpace,
        Property::FontWeight,
        Property::FontStyle,
        Property::FontFamily,
        Property::TextDecoration,
        Property::MarginTop,
        Property::MarginRight,
        Property::MarginBottom,
        Property::MarginLeft,
        Property::PaddingTop,
        Property::PaddingRight,
        Property::PaddingBottom,
        Property::PaddingLeft,
        Property::BorderTopWidth,
        Property::BorderRightWidth,
        Property::BorderBottomWidth,
        Property::BorderLeftWidth,
        Property::BorderColor,
        Property::BorderStyle,
        Property::BorderRadius,
        Property::Overflow,
        Property::Opacity,
        Property::BoxSizing,
        Property::ObjectFit,
        Property::ObjectPosition,
        Property::Width,
        Property::MinWidth,
        Property::MaxWidth,
        Property::Height,
        Property::MinHeight,
        Property::MaxHeight,
        Property::FlexDirection,
        Property::FlexWrap,
        Property::JustifyContent,
        Property::AlignItems,
        Property::AlignContent,
        Property::AlignSelf,
        Property::Gap,
        Property::FlexGrow,
        Property::FlexShrink,
        Property::FlexBasis,
        Property::Position,
        Property::ZIndex,
        Property::Top,
        Property::Right,
        Property::Bottom,
        Property::Left,
    ] {
        let Some(value) = winner_for(winners, property) else {
            continue;
        };

        apply_value(style, parent_style, property, value);
    }
}

fn winner_for(
    winners: &[Option<CascadeWinner>; Property::COUNT],
    property: Property,
) -> Option<&SpecifiedValue> {
    winners[property.index()]
        .as_ref()
        .map(|winner| &winner.value)
}

fn apply_css_wide_keyword(
    style: &mut ComputedStyle,
    parent_style: Option<&ComputedStyle>,
    property: Property,
    keyword: CssWideKeyword,
) {
    let should_inherit = match keyword {
        CssWideKeyword::Inherit => true,
        CssWideKeyword::Initial => false,
        CssWideKeyword::Unset => property.is_inherited(),
    };

    if should_inherit && let Some(parent) = parent_style {
        copy_computed_property(style, parent, property);
        return;
    }

    let initial = ComputedStyle::default();
    copy_computed_property(style, &initial, property);
}

fn copy_computed_property(target: &mut ComputedStyle, source: &ComputedStyle, property: Property) {
    match property {
        Property::Display => target.display = source.display,
        Property::Position => target.position = source.position,
        Property::ZIndex => target.z_index = source.z_index,
        Property::Top => target.top = source.top,
        Property::Right => target.right = source.right,
        Property::Bottom => target.bottom = source.bottom,
        Property::Left => target.left = source.left,
        Property::Color => target.color = source.color,
        Property::BackgroundColor => target.background_color = source.background_color,
        Property::FontSize => target.font_size = source.font_size,
        Property::LineHeight => target.line_height = source.line_height,
        Property::WhiteSpace => target.white_space = source.white_space,
        Property::FontWeight => target.font_weight = source.font_weight,
        Property::FontStyle => target.font_style = source.font_style,
        Property::FontFamily => target.font_family = source.font_family,
        Property::TextDecoration => target.underline = source.underline,

        Property::MarginTop => {
            target.margin.top = source.margin.top;
            target.margin_auto.top = source.margin_auto.top;
        }
        Property::MarginRight => {
            target.margin.right = source.margin.right;
            target.margin_auto.right = source.margin_auto.right;
        }
        Property::MarginBottom => {
            target.margin.bottom = source.margin.bottom;
            target.margin_auto.bottom = source.margin_auto.bottom;
        }
        Property::MarginLeft => {
            target.margin.left = source.margin.left;
            target.margin_auto.left = source.margin_auto.left;
        }

        Property::PaddingTop => target.padding.top = source.padding.top,
        Property::PaddingRight => target.padding.right = source.padding.right,
        Property::PaddingBottom => target.padding.bottom = source.padding.bottom,
        Property::PaddingLeft => target.padding.left = source.padding.left,

        Property::BorderTopWidth => target.border_width.top = source.border_width.top,
        Property::BorderRightWidth => target.border_width.right = source.border_width.right,
        Property::BorderBottomWidth => target.border_width.bottom = source.border_width.bottom,
        Property::BorderLeftWidth => target.border_width.left = source.border_width.left,
        Property::BorderColor => target.border_color = source.border_color,
        Property::BorderStyle => target.border_style = source.border_style,
        Property::BorderRadius => target.border_radius = source.border_radius,
        Property::Overflow => target.overflow = source.overflow,
        Property::Opacity => target.opacity = source.opacity,
        Property::BoxSizing => target.box_sizing = source.box_sizing,
        Property::ObjectFit => target.object_fit = source.object_fit,
        Property::ObjectPosition => target.object_position = source.object_position,

        Property::Width => target.width = source.width,
        Property::MinWidth => target.min_width = source.min_width,
        Property::MaxWidth => target.max_width = source.max_width,
        Property::Height => target.height = source.height,
        Property::MinHeight => target.min_height = source.min_height,
        Property::MaxHeight => target.max_height = source.max_height,

        Property::FlexDirection => target.flex_direction = source.flex_direction,
        Property::FlexWrap => target.flex_wrap = source.flex_wrap,
        Property::JustifyContent => target.justify_content = source.justify_content,
        Property::AlignItems => target.align_items = source.align_items,
        Property::AlignContent => target.align_content = source.align_content,
        Property::AlignSelf => target.align_self = source.align_self,
        Property::Gap => target.gap = source.gap,
        Property::FlexGrow => target.flex_grow = source.flex_grow,
        Property::FlexShrink => target.flex_shrink = source.flex_shrink,
        Property::FlexBasis => target.flex_basis = source.flex_basis,
    }
}

fn apply_value(
    style: &mut ComputedStyle,
    parent_style: Option<&ComputedStyle>,
    property: Property,
    value: &SpecifiedValue,
) {
    if let SpecifiedValue::CssWide(keyword) = value {
        apply_css_wide_keyword(style, parent_style, property, *keyword);
        return;
    }

    match (property, value) {
        (Property::Display, SpecifiedValue::Display(display)) => {
            style.display = *display;
        }

        (Property::Color, SpecifiedValue::Color(color)) => {
            style.color = *color;
        }

        (Property::BackgroundColor, SpecifiedValue::BackgroundColor(background)) => {
            style.background_color = *background;
        }

        (Property::FontSize, SpecifiedValue::FontSize(length)) => {
            let inherited_size = parent_style.map_or(ROOT_FONT_SIZE_PX, |parent| parent.font_size);

            if let Some(size) = resolve_font_size(*length, inherited_size) {
                style.font_size = size.max(1.0);
            }
        }

        (Property::LineHeight, SpecifiedValue::LineHeight(line_height)) => {
            if let Some(line_height) = resolve_line_height(*line_height, style.font_size) {
                style.line_height = line_height;
            }
        }

        (Property::WhiteSpace, SpecifiedValue::WhiteSpace(white_space)) => {
            style.white_space = *white_space;
        }

        (Property::Position, SpecifiedValue::Position(position)) => {
            style.position = *position;
        }

        (Property::ZIndex, SpecifiedValue::ZIndex(z_index)) => {
            style.z_index = *z_index;
        }

        (Property::Top, SpecifiedValue::OffsetLength(length)) => {
            style.top = resolve_offset_length(*length, style.font_size);
        }

        (Property::Right, SpecifiedValue::OffsetLength(length)) => {
            style.right = resolve_offset_length(*length, style.font_size);
        }

        (Property::Bottom, SpecifiedValue::OffsetLength(length)) => {
            style.bottom = resolve_offset_length(*length, style.font_size);
        }

        (Property::Left, SpecifiedValue::OffsetLength(length)) => {
            style.left = resolve_offset_length(*length, style.font_size);
        }

        (Property::FontWeight, SpecifiedValue::FontWeight(weight)) => {
            style.font_weight = *weight;
        }

        (Property::FontStyle, SpecifiedValue::FontStyle(font_style)) => {
            style.font_style = *font_style;
        }

        (Property::FontFamily, SpecifiedValue::FontFamily(family)) => {
            style.font_family = *family;
        }

        (Property::TextDecoration, SpecifiedValue::Underline(underline)) => {
            style.underline = *underline;
        }

        (Property::MarginTop, SpecifiedValue::MarginLength(length)) => {
            let (value, automatic) = resolve_margin_length(*length, style.font_size);

            style.margin.top = value;
            style.margin_auto.top = automatic;
        }

        (Property::MarginRight, SpecifiedValue::MarginLength(length)) => {
            let (value, automatic) = resolve_margin_length(*length, style.font_size);

            style.margin.right = value;
            style.margin_auto.right = automatic;
        }

        (Property::MarginBottom, SpecifiedValue::MarginLength(length)) => {
            let (value, automatic) = resolve_margin_length(*length, style.font_size);

            style.margin.bottom = value;
            style.margin_auto.bottom = automatic;
        }

        (Property::MarginLeft, SpecifiedValue::MarginLength(length)) => {
            let (value, automatic) = resolve_margin_length(*length, style.font_size);

            style.margin.left = value;
            style.margin_auto.left = automatic;
        }

        (Property::PaddingTop, SpecifiedValue::EdgeLength(length)) => {
            if let Some(value) = resolve_edge_length(*length, style.font_size) {
                style.padding.top = value.max(0.0);
            }
        }

        (Property::PaddingRight, SpecifiedValue::EdgeLength(length)) => {
            if let Some(value) = resolve_edge_length(*length, style.font_size) {
                style.padding.right = value.max(0.0);
            }
        }

        (Property::PaddingBottom, SpecifiedValue::EdgeLength(length)) => {
            if let Some(value) = resolve_edge_length(*length, style.font_size) {
                style.padding.bottom = value.max(0.0);
            }
        }

        (Property::PaddingLeft, SpecifiedValue::EdgeLength(length)) => {
            if let Some(value) = resolve_edge_length(*length, style.font_size) {
                style.padding.left = value.max(0.0);
            }
        }

        (Property::BorderTopWidth, SpecifiedValue::EdgeLength(length)) => {
            if let Some(value) = resolve_edge_length(*length, style.font_size) {
                style.border_width.top = value.max(0.0);
            }
        }

        (Property::BorderRightWidth, SpecifiedValue::EdgeLength(length)) => {
            if let Some(value) = resolve_edge_length(*length, style.font_size) {
                style.border_width.right = value.max(0.0);
            }
        }

        (Property::BorderBottomWidth, SpecifiedValue::EdgeLength(length)) => {
            if let Some(value) = resolve_edge_length(*length, style.font_size) {
                style.border_width.bottom = value.max(0.0);
            }
        }

        (Property::BorderLeftWidth, SpecifiedValue::EdgeLength(length)) => {
            if let Some(value) = resolve_edge_length(*length, style.font_size) {
                style.border_width.left = value.max(0.0);
            }
        }

        (Property::BorderColor, SpecifiedValue::BorderColor(color)) => {
            style.border_color = *color;
        }

        (Property::BorderStyle, SpecifiedValue::BorderStyle(border_style)) => {
            style.border_style = *border_style;
        }

        (Property::BorderRadius, SpecifiedValue::BorderRadius(length)) => {
            style.border_radius = resolve_size_length(*length, style.font_size);
        }

        (Property::Overflow, SpecifiedValue::Overflow(overflow)) => {
            style.overflow = *overflow;
        }

        (Property::Opacity, SpecifiedValue::Opacity(opacity)) => {
            style.opacity = sanitize_opacity(*opacity);
        }

        (Property::BoxSizing, SpecifiedValue::BoxSizing(box_sizing)) => {
            style.box_sizing = *box_sizing;
        }

        (Property::ObjectFit, SpecifiedValue::ObjectFit(object_fit)) => {
            style.object_fit = *object_fit;
        }

        (Property::ObjectPosition, SpecifiedValue::ObjectPosition(position)) => {
            style.object_position = *position;
        }

        (Property::Width, SpecifiedValue::SizeLength(length)) => {
            style.width = resolve_size_length(*length, style.font_size);
        }

        (Property::MinWidth, SpecifiedValue::SizeLength(length)) => {
            style.min_width = resolve_size_length(*length, style.font_size);
        }

        (Property::MaxWidth, SpecifiedValue::SizeLength(length)) => {
            style.max_width = resolve_size_length(*length, style.font_size);
        }

        (Property::Height, SpecifiedValue::SizeLength(length)) => {
            style.height = resolve_size_length(*length, style.font_size);
        }

        (Property::MinHeight, SpecifiedValue::SizeLength(length)) => {
            style.min_height = resolve_size_length(*length, style.font_size);
        }

        (Property::MaxHeight, SpecifiedValue::SizeLength(length)) => {
            style.max_height = resolve_size_length(*length, style.font_size);
        }

        (Property::FlexDirection, SpecifiedValue::FlexDirection(direction)) => {
            style.flex_direction = *direction;
        }

        (Property::FlexWrap, SpecifiedValue::FlexWrap(flex_wrap)) => {
            style.flex_wrap = *flex_wrap;
        }

        (Property::JustifyContent, SpecifiedValue::JustifyContent(justify)) => {
            style.justify_content = *justify;
        }

        (Property::AlignItems, SpecifiedValue::AlignItems(align)) => {
            style.align_items = *align;
        }

        (Property::AlignContent, SpecifiedValue::AlignContent(align)) => {
            style.align_content = *align;
        }

        (Property::AlignSelf, SpecifiedValue::AlignSelf(align)) => {
            style.align_self = *align;
        }

        (Property::Gap, SpecifiedValue::SizeLength(length)) => {
            style.gap = resolve_size_length(*length, style.font_size);
        }

        (Property::FlexGrow, SpecifiedValue::Number(value)) => {
            style.flex_grow = value.max(0.0);
        }

        (Property::FlexShrink, SpecifiedValue::Number(value)) => {
            style.flex_shrink = value.max(0.0);
        }

        (Property::FlexBasis, SpecifiedValue::SizeLength(length)) => {
            style.flex_basis = resolve_size_length(*length, style.font_size);
        }

        _ => {}
    }
}

fn collect_author_css(document: &Document) -> String {
    let mut source = String::new();

    for node in document.nodes() {
        let NodeKind::Element(element) = node.kind() else {
            continue;
        };

        if element.tag_name() != "style" {
            continue;
        }

        for child_id in node.children() {
            let Some(child) = document.node(*child_id) else {
                continue;
            };

            if let NodeKind::Text(text) = child.kind() {
                let required = text.len().saturating_add(1);
                if source.len().saturating_add(required) > MAX_CSS_SOURCE_BYTES {
                    return source;
                }

                source.push_str(text);
                source.push('\n');
            }
        }
    }

    source
}

fn initial_style(node: &Node, parent: Option<&ComputedStyle>) -> ComputedStyle {
    let mut style = ComputedStyle::default();

    if let Some(parent_style) = parent {
        style.color = parent_style.color;
        style.font_size = parent_style.font_size;
        style.font_weight = parent_style.font_weight;
        style.font_style = parent_style.font_style;
        style.font_family = parent_style.font_family;
        style.line_height = parent_style.line_height;
        style.white_space = parent_style.white_space;
    }

    let NodeKind::Element(element) = node.kind() else {
        return style;
    };

    let tag = element.tag_name();

    style.display = default_display(tag);

    match tag {
        "body" => {
            style.margin = EdgeSizes::new(8.0, 8.0, 8.0, 8.0);
        }

        "h1" => {
            style.font_size = 32.0;
            style.font_weight = FontWeight::Bold;
            style.margin = EdgeSizes::new(21.0, 0.0, 21.0, 0.0);
        }

        "h2" => {
            style.font_size = 24.0;
            style.font_weight = FontWeight::Bold;
            style.margin = EdgeSizes::new(20.0, 0.0, 20.0, 0.0);
        }

        "h3" => {
            style.font_size = 19.0;
            style.font_weight = FontWeight::Bold;
            style.margin = EdgeSizes::new(18.0, 0.0, 18.0, 0.0);
        }

        "h4" | "h5" | "h6" => {
            style.font_weight = FontWeight::Bold;
            style.margin = EdgeSizes::new(16.0, 0.0, 16.0, 0.0);
        }

        "p" => {
            style.margin = EdgeSizes::new(16.0, 0.0, 16.0, 0.0);
        }

        "strong" | "b" => {
            style.font_weight = FontWeight::Bold;
        }

        "em" | "i" => {
            style.font_style = FontStyle::Italic;
        }

        "code" => {
            style.font_family = FontFamily::Monospace;
        }

        "pre" => {
            style.font_family = FontFamily::Monospace;
            style.white_space = WhiteSpace::Pre;
        }

        "a" => {
            style.color = Rgba::new(0, 0, 238, 255);
            style.underline = true;
        }

        _ => {}
    }

    style
}

// PHANTOM_2C13_FORM_UA_DISPLAY
fn default_display(tag: &str) -> Display {
    match tag {
        "head" | "style" | "script" | "meta" | "link" | "title" | "base" => Display::None,

        "html" | "body" | "form" | "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul"
        | "ol" | "li" | "section" | "article" | "main" | "header" | "footer" | "nav"
        | "blockquote" | "pre" | "hr" => Display::Block,

        _ => Display::Inline,
    }
}

fn scan_rule_blocks(source: &str) -> Vec<RuleBlock<'_>> {
    let bytes = source.as_bytes();
    let mut blocks = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        if blocks.len() >= MAX_SCANNED_RULE_BLOCKS {
            break;
        }

        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }

        if cursor >= bytes.len() {
            break;
        }

        let selector_start = cursor;

        match scan_rule_prelude(bytes, cursor) {
            RulePreludeBoundary::OpenBrace(open_brace) => {
                let body_start = open_brace.saturating_add(1);
                let Some(close_brace) = find_matching_brace(bytes, body_start) else {
                    break;
                };

                let selector = source[selector_start..open_brace].trim();

                if !selector.is_empty() {
                    blocks.push(RuleBlock {
                        selector,
                        body: &source[body_start..close_brace],
                    });
                }

                cursor = close_brace.saturating_add(1);
            }

            RulePreludeBoundary::Recover(next) => {
                cursor = next;
            }

            RulePreludeBoundary::End => break,
        }
    }

    blocks
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RulePreludeBoundary {
    OpenBrace(usize),
    Recover(usize),
    End,
}

fn scan_rule_prelude(bytes: &[u8], start: usize) -> RulePreludeBoundary {
    let mut cursor = start;
    let mut quote = None;
    let mut escaped = false;
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'(' => parentheses = parentheses.saturating_add(1),
                b')' => parentheses = parentheses.saturating_sub(1),
                b'[' => brackets = brackets.saturating_add(1),
                b']' => brackets = brackets.saturating_sub(1),
                b'{' if parentheses == 0 && brackets == 0 => {
                    return RulePreludeBoundary::OpenBrace(cursor);
                }
                b';' | b'}' if parentheses == 0 && brackets == 0 => {
                    return RulePreludeBoundary::Recover(cursor.saturating_add(1));
                }
                _ => {}
            }
        }

        cursor = cursor.saturating_add(1);
    }

    RulePreludeBoundary::End
}

fn find_matching_brace(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    let mut depth = 1_u32;
    let mut quote = None;
    let mut escaped = false;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'{' => depth = depth.saturating_add(1),
                b'}' => {
                    depth = depth.saturating_sub(1);

                    if depth == 0 {
                        return Some(cursor);
                    }
                }
                _ => {}
            }
        }

        cursor += 1;
    }

    None
}

fn parse_declarations(source: &str) -> Vec<Declaration> {
    let segments = split_top_level(source, b';');
    let mut declarations = Vec::new();
    let mut declaration_order = 0_u16;

    for segment in segments {
        if declarations.len() >= MAX_DECLARATIONS_PER_RULE {
            break;
        }
        let Some((property_source, value_source)) = split_declaration(segment) else {
            continue;
        };

        let property_name = property_source.trim().to_ascii_lowercase();
        let (value_source, important) = strip_important(value_source);

        let expanded = parse_property(&property_name, value_source);

        for (property, value) in expanded {
            if declarations.len() >= MAX_DECLARATIONS_PER_RULE {
                break;
            }
            declarations.push(Declaration {
                property,
                value,
                important,
                declaration_order,
            });
        }

        declaration_order = declaration_order.saturating_add(1);
    }

    declarations
}

fn split_top_level(source: &str, delimiter: u8) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut cursor = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut parentheses = 0_u32;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'(' => parentheses = parentheses.saturating_add(1),
                b')' => parentheses = parentheses.saturating_sub(1),
                _ if byte == delimiter && parentheses == 0 => {
                    if parts.len() >= MAX_DECLARATIONS_PER_RULE {
                        break;
                    }
                    parts.push(&source[start..cursor]);
                    start = cursor.saturating_add(1);
                }
                _ => {}
            }
        }

        cursor += 1;
    }

    if parts.len() < MAX_DECLARATIONS_PER_RULE {
        parts.push(&source[start..]);
    }

    parts
}

fn split_declaration(source: &str) -> Option<(&str, &str)> {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut parentheses = 0_u32;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'(' => parentheses = parentheses.saturating_add(1),
                b')' => parentheses = parentheses.saturating_sub(1),
                b':' if parentheses == 0 => {
                    return Some((&source[..cursor], &source[cursor + 1..]));
                }
                _ => {}
            }
        }

        cursor += 1;
    }

    None
}

fn strip_important(value: &str) -> (&str, bool) {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();

    if let Some(prefix) = lower.strip_suffix("!important") {
        let prefix_len = prefix.len();
        (&trimmed[..prefix_len], true)
    } else {
        (trimmed, false)
    }
}

fn parse_css_wide_keyword(value: &str) -> Option<CssWideKeyword> {
    match value.trim().to_ascii_lowercase().as_str() {
        "inherit" => Some(CssWideKeyword::Inherit),
        "initial" => Some(CssWideKeyword::Initial),
        "unset" => Some(CssWideKeyword::Unset),
        _ => None,
    }
}

fn css_wide_properties(property: &str) -> Option<Vec<Property>> {
    let properties = match property {
        "display" => vec![Property::Display],
        "position" => vec![Property::Position],
        "z-index" => vec![Property::ZIndex],
        "top" => vec![Property::Top],
        "right" => vec![Property::Right],
        "bottom" => vec![Property::Bottom],
        "left" => vec![Property::Left],
        "color" => vec![Property::Color],
        "background" | "background-color" => vec![Property::BackgroundColor],
        "font-size" => vec![Property::FontSize],
        "line-height" => vec![Property::LineHeight],
        "white-space" => vec![Property::WhiteSpace],
        "font-weight" => vec![Property::FontWeight],
        "font-style" => vec![Property::FontStyle],
        "font-family" => vec![Property::FontFamily],
        "text-decoration" | "text-decoration-line" => vec![Property::TextDecoration],

        "margin" => vec![
            Property::MarginTop,
            Property::MarginRight,
            Property::MarginBottom,
            Property::MarginLeft,
        ],
        "margin-top" => vec![Property::MarginTop],
        "margin-right" => vec![Property::MarginRight],
        "margin-bottom" => vec![Property::MarginBottom],
        "margin-left" => vec![Property::MarginLeft],

        "padding" => vec![
            Property::PaddingTop,
            Property::PaddingRight,
            Property::PaddingBottom,
            Property::PaddingLeft,
        ],
        "padding-top" => vec![Property::PaddingTop],
        "padding-right" => vec![Property::PaddingRight],
        "padding-bottom" => vec![Property::PaddingBottom],
        "padding-left" => vec![Property::PaddingLeft],

        "border-width" => vec![
            Property::BorderTopWidth,
            Property::BorderRightWidth,
            Property::BorderBottomWidth,
            Property::BorderLeftWidth,
        ],
        "border-top-width" => vec![Property::BorderTopWidth],
        "border-right-width" => vec![Property::BorderRightWidth],
        "border-bottom-width" => vec![Property::BorderBottomWidth],
        "border-left-width" => vec![Property::BorderLeftWidth],
        "border-color" => vec![Property::BorderColor],
        "border-style" => vec![Property::BorderStyle],
        "border-radius" => vec![Property::BorderRadius],
        "overflow" => vec![Property::Overflow],
        "opacity" => vec![Property::Opacity],
        "border" => vec![
            Property::BorderTopWidth,
            Property::BorderRightWidth,
            Property::BorderBottomWidth,
            Property::BorderLeftWidth,
            Property::BorderColor,
            Property::BorderStyle,
        ],

        "box-sizing" => vec![Property::BoxSizing],
        "object-fit" => vec![Property::ObjectFit],
        "object-position" => vec![Property::ObjectPosition],

        "width" => vec![Property::Width],
        "min-width" => vec![Property::MinWidth],
        "max-width" => vec![Property::MaxWidth],
        "height" => vec![Property::Height],
        "min-height" => vec![Property::MinHeight],
        "max-height" => vec![Property::MaxHeight],

        "flex-direction" => vec![Property::FlexDirection],
        "flex-wrap" => vec![Property::FlexWrap],
        "flex-flow" => vec![Property::FlexDirection, Property::FlexWrap],
        "justify-content" => vec![Property::JustifyContent],
        "align-items" => vec![Property::AlignItems],
        "align-content" => vec![Property::AlignContent],
        "align-self" => vec![Property::AlignSelf],
        "gap" => vec![Property::Gap],
        "flex-grow" => vec![Property::FlexGrow],
        "flex-shrink" => vec![Property::FlexShrink],
        "flex-basis" => vec![Property::FlexBasis],
        "flex" => vec![
            Property::FlexGrow,
            Property::FlexShrink,
            Property::FlexBasis,
        ],

        _ => return None,
    };

    Some(properties)
}

fn parse_property(property: &str, value: &str) -> Vec<(Property, SpecifiedValue)> {
    if let Some(keyword) = parse_css_wide_keyword(value)
        && let Some(properties) = css_wide_properties(property)
    {
        return properties
            .into_iter()
            .map(|property| (property, SpecifiedValue::CssWide(keyword)))
            .collect();
    }

    match property {
        "display" => parse_display(value)
            .map(|display| vec![(Property::Display, SpecifiedValue::Display(display))])
            .unwrap_or_default(),

        "position" => parse_position(value)
            .map(|position| vec![(Property::Position, SpecifiedValue::Position(position))])
            .unwrap_or_default(),

        "z-index" => parse_z_index(value)
            .map(|z_index| vec![(Property::ZIndex, SpecifiedValue::ZIndex(z_index))])
            .unwrap_or_default(),

        "top" => parse_offset_property(Property::Top, value),
        "right" => parse_offset_property(Property::Right, value),
        "bottom" => parse_offset_property(Property::Bottom, value),
        "left" => parse_offset_property(Property::Left, value),

        "color" => parse_color(value)
            .map(|color| vec![(Property::Color, SpecifiedValue::Color(color))])
            .unwrap_or_default(),

        "background-color" => parse_background_color(value)
            .map(|color| {
                vec![(
                    Property::BackgroundColor,
                    SpecifiedValue::BackgroundColor(color),
                )]
            })
            .unwrap_or_default(),

        "background" => parse_background_shorthand(value),

        "font-size" => parse_font_size_length(value)
            .map(|length| vec![(Property::FontSize, SpecifiedValue::FontSize(length))])
            .unwrap_or_default(),

        "line-height" => parse_line_height(value)
            .map(|line_height| {
                vec![(
                    Property::LineHeight,
                    SpecifiedValue::LineHeight(line_height),
                )]
            })
            .unwrap_or_default(),

        "white-space" => parse_white_space(value)
            .map(|white_space| {
                vec![(
                    Property::WhiteSpace,
                    SpecifiedValue::WhiteSpace(white_space),
                )]
            })
            .unwrap_or_default(),

        "font-weight" => parse_font_weight(value)
            .map(|weight| vec![(Property::FontWeight, SpecifiedValue::FontWeight(weight))])
            .unwrap_or_default(),

        "font-style" => parse_font_style(value)
            .map(|font_style| vec![(Property::FontStyle, SpecifiedValue::FontStyle(font_style))])
            .unwrap_or_default(),

        "font-family" => {
            let family = if value.to_ascii_lowercase().contains("mono") {
                FontFamily::Monospace
            } else {
                FontFamily::SansSerif
            };

            vec![(Property::FontFamily, SpecifiedValue::FontFamily(family))]
        }

        "text-decoration" | "text-decoration-line" => {
            let underline = value
                .to_ascii_lowercase()
                .split_ascii_whitespace()
                .any(|token| token == "underline");

            vec![(
                Property::TextDecoration,
                SpecifiedValue::Underline(underline),
            )]
        }

        "margin" => expand_margin_edges(
            value,
            [
                Property::MarginTop,
                Property::MarginRight,
                Property::MarginBottom,
                Property::MarginLeft,
            ],
        ),

        "padding" => expand_edges(
            value,
            [
                Property::PaddingTop,
                Property::PaddingRight,
                Property::PaddingBottom,
                Property::PaddingLeft,
            ],
        ),

        "margin-top" => parse_margin_property(Property::MarginTop, value),
        "margin-right" => parse_margin_property(Property::MarginRight, value),
        "margin-bottom" => parse_margin_property(Property::MarginBottom, value),
        "margin-left" => parse_margin_property(Property::MarginLeft, value),

        "padding-top" => parse_edge_property(Property::PaddingTop, value),
        "padding-right" => parse_edge_property(Property::PaddingRight, value),
        "padding-bottom" => parse_edge_property(Property::PaddingBottom, value),
        "padding-left" => parse_edge_property(Property::PaddingLeft, value),

        "border-width" => expand_border_width_edges(
            value,
            [
                Property::BorderTopWidth,
                Property::BorderRightWidth,
                Property::BorderBottomWidth,
                Property::BorderLeftWidth,
            ],
        ),

        "border-top-width" => parse_border_width_property(Property::BorderTopWidth, value),

        "border-right-width" => parse_border_width_property(Property::BorderRightWidth, value),

        "border-bottom-width" => parse_border_width_property(Property::BorderBottomWidth, value),

        "border-left-width" => parse_border_width_property(Property::BorderLeftWidth, value),

        "border-color" => parse_border_color(value)
            .map(|color| vec![(Property::BorderColor, SpecifiedValue::BorderColor(color))])
            .unwrap_or_default(),

        "border-style" => parse_border_style(value)
            .map(|border_style| {
                vec![(
                    Property::BorderStyle,
                    SpecifiedValue::BorderStyle(border_style),
                )]
            })
            .unwrap_or_default(),

        "border" => parse_border_shorthand(value),

        "border-radius" => parse_border_radius(value)
            .map(|radius| vec![(Property::BorderRadius, SpecifiedValue::BorderRadius(radius))])
            .unwrap_or_default(),

        "overflow" => parse_overflow(value)
            .map(|overflow| vec![(Property::Overflow, SpecifiedValue::Overflow(overflow))])
            .unwrap_or_default(),

        "opacity" => parse_opacity(value)
            .map(|opacity| vec![(Property::Opacity, SpecifiedValue::Opacity(opacity))])
            .unwrap_or_default(),

        "box-sizing" => parse_box_sizing(value)
            .map(|box_sizing| vec![(Property::BoxSizing, SpecifiedValue::BoxSizing(box_sizing))])
            .unwrap_or_default(),

        "object-fit" => parse_object_fit(value)
            .map(|fit| vec![(Property::ObjectFit, SpecifiedValue::ObjectFit(fit))])
            .unwrap_or_default(),

        "object-position" => parse_object_position(value)
            .map(|position| {
                vec![(
                    Property::ObjectPosition,
                    SpecifiedValue::ObjectPosition(position),
                )]
            })
            .unwrap_or_default(),

        "width" => parse_size_property(Property::Width, value),
        "min-width" => parse_constraint_property(Property::MinWidth, value, false),
        "max-width" => parse_constraint_property(Property::MaxWidth, value, true),

        "height" => parse_size_property(Property::Height, value),
        "min-height" => parse_constraint_property(Property::MinHeight, value, false),
        "max-height" => parse_constraint_property(Property::MaxHeight, value, true),

        "flex-direction" => parse_flex_direction(value)
            .map(|direction| {
                vec![(
                    Property::FlexDirection,
                    SpecifiedValue::FlexDirection(direction),
                )]
            })
            .unwrap_or_default(),

        "flex-wrap" => parse_flex_wrap(value)
            .map(|flex_wrap| vec![(Property::FlexWrap, SpecifiedValue::FlexWrap(flex_wrap))])
            .unwrap_or_default(),

        "flex-flow" => parse_flex_flow_shorthand(value),

        "justify-content" => parse_justify_content(value)
            .map(|justify| {
                vec![(
                    Property::JustifyContent,
                    SpecifiedValue::JustifyContent(justify),
                )]
            })
            .unwrap_or_default(),

        "align-items" => parse_align_items(value)
            .map(|align| vec![(Property::AlignItems, SpecifiedValue::AlignItems(align))])
            .unwrap_or_default(),

        "align-content" => parse_align_content(value)
            .map(|align| vec![(Property::AlignContent, SpecifiedValue::AlignContent(align))])
            .unwrap_or_default(),

        "align-self" => parse_align_self(value)
            .map(|align| vec![(Property::AlignSelf, SpecifiedValue::AlignSelf(align))])
            .unwrap_or_default(),

        "gap" => parse_non_negative_size_property(Property::Gap, value),

        "flex-grow" => parse_non_negative_number(Property::FlexGrow, value),

        "flex-shrink" => parse_non_negative_number(Property::FlexShrink, value),

        "flex-basis" => parse_size_property(Property::FlexBasis, value),

        "flex" => parse_flex_shorthand(value),

        _ => Vec::new(),
    }
}

fn parse_flex_direction(value: &str) -> Option<FlexDirection> {
    match value.trim().to_ascii_lowercase().as_str() {
        "row" => Some(FlexDirection::Row),
        "row-reverse" => Some(FlexDirection::RowReverse),
        "column" => Some(FlexDirection::Column),
        "column-reverse" => Some(FlexDirection::ColumnReverse),
        _ => None,
    }
}

fn parse_flex_wrap(value: &str) -> Option<FlexWrap> {
    match value.trim().to_ascii_lowercase().as_str() {
        "nowrap" => Some(FlexWrap::NoWrap),
        "wrap" => Some(FlexWrap::Wrap),
        "wrap-reverse" => Some(FlexWrap::WrapReverse),
        _ => None,
    }
}

fn parse_flex_flow_shorthand(value: &str) -> Vec<(Property, SpecifiedValue)> {
    let normalized = value.trim().to_ascii_lowercase();

    let tokens: Vec<&str> = normalized.split_ascii_whitespace().collect();

    if tokens.is_empty() || tokens.len() > 2 {
        return Vec::new();
    }

    let mut direction = None;
    let mut flex_wrap = None;

    for token in tokens {
        if let Some(parsed) = parse_flex_direction(token) {
            if direction.is_some() {
                return Vec::new();
            }

            direction = Some(parsed);
            continue;
        }

        if let Some(parsed) = parse_flex_wrap(token) {
            if flex_wrap.is_some() {
                return Vec::new();
            }

            flex_wrap = Some(parsed);
            continue;
        }

        return Vec::new();
    }

    vec![
        (
            Property::FlexDirection,
            SpecifiedValue::FlexDirection(direction.unwrap_or(FlexDirection::Row)),
        ),
        (
            Property::FlexWrap,
            SpecifiedValue::FlexWrap(flex_wrap.unwrap_or(FlexWrap::NoWrap)),
        ),
    ]
}

fn parse_justify_content(value: &str) -> Option<JustifyContent> {
    match value.trim().to_ascii_lowercase().as_str() {
        "flex-start" | "start" => Some(JustifyContent::FlexStart),
        "center" => Some(JustifyContent::Center),
        "flex-end" | "end" => Some(JustifyContent::FlexEnd),
        "space-between" => Some(JustifyContent::SpaceBetween),
        "space-around" => Some(JustifyContent::SpaceAround),
        _ => None,
    }
}

fn parse_align_items(value: &str) -> Option<AlignItems> {
    match value.trim().to_ascii_lowercase().as_str() {
        "stretch" => Some(AlignItems::Stretch),
        "flex-start" | "start" => Some(AlignItems::FlexStart),
        "center" => Some(AlignItems::Center),
        "flex-end" | "end" => Some(AlignItems::FlexEnd),
        _ => None,
    }
}

fn parse_align_content(value: &str) -> Option<AlignContent> {
    match value.trim().to_ascii_lowercase().as_str() {
        "stretch" => Some(AlignContent::Stretch),
        "flex-start" | "start" => Some(AlignContent::FlexStart),
        "center" => Some(AlignContent::Center),
        "flex-end" | "end" => Some(AlignContent::FlexEnd),
        "space-between" => Some(AlignContent::SpaceBetween),
        "space-around" => Some(AlignContent::SpaceAround),
        _ => None,
    }
}

fn parse_align_self(value: &str) -> Option<AlignSelf> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(AlignSelf::Auto),
        "stretch" => Some(AlignSelf::Stretch),
        "flex-start" | "start" => Some(AlignSelf::FlexStart),
        "center" => Some(AlignSelf::Center),
        "flex-end" | "end" => Some(AlignSelf::FlexEnd),
        _ => None,
    }
}

fn parse_non_negative_size_property(
    property: Property,
    value: &str,
) -> Vec<(Property, SpecifiedValue)> {
    parse_specified_length(value, true)
        .filter(|length| {
            !matches!(length, SpecifiedLength::Auto) && specified_length_is_non_negative(*length)
        })
        .map(|length| vec![(property, SpecifiedValue::SizeLength(length))])
        .unwrap_or_default()
}

fn parse_non_negative_number(property: Property, value: &str) -> Vec<(Property, SpecifiedValue)> {
    parse_non_negative_float(value.trim())
        .map(|number| vec![(property, SpecifiedValue::Number(number))])
        .unwrap_or_default()
}

fn parse_flex_shorthand(value: &str) -> Vec<(Property, SpecifiedValue)> {
    let normalized = value.trim().to_ascii_lowercase();

    match normalized.as_str() {
        "none" => {
            return flex_shorthand_values(0.0, 0.0, SpecifiedLength::Auto);
        }

        "auto" => {
            return flex_shorthand_values(1.0, 1.0, SpecifiedLength::Auto);
        }

        "initial" => {
            return flex_shorthand_values(0.0, 1.0, SpecifiedLength::Auto);
        }

        _ => {}
    }

    let tokens: Vec<&str> = normalized.split_ascii_whitespace().collect();

    if tokens.is_empty() || tokens.len() > 3 {
        return Vec::new();
    }

    if tokens.len() == 1 {
        if let Some(grow) = parse_non_negative_float(tokens[0]) {
            return flex_shorthand_values(grow, 1.0, SpecifiedLength::Px(0.0));
        }

        return parse_flex_basis_component(tokens[0])
            .map(|basis| flex_shorthand_values(1.0, 1.0, basis))
            .unwrap_or_default();
    }

    let Some(grow) = parse_non_negative_float(tokens[0]) else {
        return Vec::new();
    };

    if tokens.len() == 2 {
        if let Some(shrink) = parse_non_negative_float(tokens[1]) {
            return flex_shorthand_values(grow, shrink, SpecifiedLength::Px(0.0));
        }

        return parse_flex_basis_component(tokens[1])
            .map(|basis| flex_shorthand_values(grow, 1.0, basis))
            .unwrap_or_default();
    }

    let Some(shrink) = parse_non_negative_float(tokens[1]) else {
        return Vec::new();
    };

    let Some(basis) = parse_flex_basis_component(tokens[2]) else {
        return Vec::new();
    };

    flex_shorthand_values(grow, shrink, basis)
}

fn parse_flex_basis_component(value: &str) -> Option<SpecifiedLength> {
    parse_specified_length(value, true).filter(|length| {
        matches!(length, SpecifiedLength::Auto) || specified_length_is_non_negative(*length)
    })
}

fn flex_shorthand_values(
    grow: f32,
    shrink: f32,
    basis: SpecifiedLength,
) -> Vec<(Property, SpecifiedValue)> {
    vec![
        (Property::FlexGrow, SpecifiedValue::Number(grow)),
        (Property::FlexShrink, SpecifiedValue::Number(shrink)),
        (Property::FlexBasis, SpecifiedValue::SizeLength(basis)),
    ]
}

fn parse_non_negative_float(value: &str) -> Option<f32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|number| number.is_finite() && *number >= 0.0 && *number <= MAX_NUMERIC_MAGNITUDE)
}

fn parse_margin_property(property: Property, value: &str) -> Vec<(Property, SpecifiedValue)> {
    parse_specified_length(value, true)
        .filter(|length| !matches!(length, SpecifiedLength::Percent(_)))
        .map(|length| vec![(property, SpecifiedValue::MarginLength(length))])
        .unwrap_or_default()
}

fn parse_edge_property(property: Property, value: &str) -> Vec<(Property, SpecifiedValue)> {
    parse_specified_length(value, false)
        .filter(|length| {
            !matches!(length, SpecifiedLength::Auto | SpecifiedLength::Percent(_))
                && specified_length_is_non_negative(*length)
        })
        .map(|length| vec![(property, SpecifiedValue::EdgeLength(length))])
        .unwrap_or_default()
}

fn parse_size_property(property: Property, value: &str) -> Vec<(Property, SpecifiedValue)> {
    parse_specified_length(value, true)
        .filter(|length| {
            matches!(length, SpecifiedLength::Auto) || specified_length_is_non_negative(*length)
        })
        .map(|length| vec![(property, SpecifiedValue::SizeLength(length))])
        .unwrap_or_default()
}

fn parse_constraint_property(
    property: Property,
    value: &str,
    allow_none: bool,
) -> Vec<(Property, SpecifiedValue)> {
    let normalized = value.trim().to_ascii_lowercase();

    if allow_none && normalized == "none" {
        return vec![(property, SpecifiedValue::SizeLength(SpecifiedLength::Auto))];
    }

    parse_specified_length(value, true)
        .filter(|length| {
            matches!(length, SpecifiedLength::Auto) || specified_length_is_non_negative(*length)
        })
        .map(|length| vec![(property, SpecifiedValue::SizeLength(length))])
        .unwrap_or_default()
}

fn parse_border_width_value(value: &str) -> Option<SpecifiedLength> {
    match value.trim().to_ascii_lowercase().as_str() {
        "thin" => Some(SpecifiedLength::Px(1.0)),
        "medium" => Some(SpecifiedLength::Px(3.0)),
        "thick" => Some(SpecifiedLength::Px(5.0)),
        _ => parse_specified_length(value, false).filter(|length| {
            !matches!(length, SpecifiedLength::Auto | SpecifiedLength::Percent(_))
                && specified_length_is_non_negative(*length)
        }),
    }
}

fn parse_border_width_property(property: Property, value: &str) -> Vec<(Property, SpecifiedValue)> {
    parse_border_width_value(value)
        .map(|length| vec![(property, SpecifiedValue::EdgeLength(length))])
        .unwrap_or_default()
}

fn expand_border_width_edges(
    value: &str,
    properties: [Property; 4],
) -> Vec<(Property, SpecifiedValue)> {
    let parsed = value
        .split_ascii_whitespace()
        .map(parse_border_width_value)
        .collect::<Option<Vec<_>>>();

    let Some(values) = parsed else {
        return Vec::new();
    };

    let edges = match values.as_slice() {
        [all] => [*all, *all, *all, *all],
        [vertical, horizontal] => [*vertical, *horizontal, *vertical, *horizontal],
        [top, horizontal, bottom] => [*top, *horizontal, *bottom, *horizontal],
        [top, right, bottom, left] => [*top, *right, *bottom, *left],
        _ => return Vec::new(),
    };

    properties
        .into_iter()
        .zip(edges)
        .map(|(property, length)| (property, SpecifiedValue::EdgeLength(length)))
        .collect()
}

fn parse_border_color(value: &str) -> Option<Option<Rgba>> {
    if value.trim().eq_ignore_ascii_case("currentcolor") {
        return Some(None);
    }

    parse_color(value).map(Some)
}

fn parse_border_style(value: &str) -> Option<BorderStyle> {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => Some(BorderStyle::None),
        "solid" => Some(BorderStyle::Solid),
        _ => None,
    }
}

fn parse_border_radius(value: &str) -> Option<SpecifiedLength> {
    let radius = parse_specified_length(value, true)?;

    if matches!(radius, SpecifiedLength::Auto) || !specified_length_is_non_negative(radius) {
        None
    } else {
        Some(radius)
    }
}

fn parse_overflow(value: &str) -> Option<Overflow> {
    match value.trim().to_ascii_lowercase().as_str() {
        "visible" => Some(Overflow::Visible),
        "hidden" | "clip" => Some(Overflow::Hidden),
        _ => None,
    }
}

fn parse_opacity(value: &str) -> Option<f32> {
    let normalized = value.trim().to_ascii_lowercase();

    if let Some(percent) = normalized.strip_suffix('%') {
        return parse_number(percent).map(|number| sanitize_opacity(number / 100.0));
    }

    parse_number(&normalized).map(sanitize_opacity)
}

fn parse_box_sizing(value: &str) -> Option<BoxSizing> {
    match value.trim().to_ascii_lowercase().as_str() {
        "content-box" => Some(BoxSizing::ContentBox),
        "border-box" => Some(BoxSizing::BorderBox),
        _ => None,
    }
}

fn parse_border_shorthand(value: &str) -> Vec<(Property, SpecifiedValue)> {
    let normalized = value.trim();

    if normalized.is_empty() {
        return Vec::new();
    }

    let mut width = None;
    let mut border_style = None;
    let mut color = None;

    for token in normalized.split_ascii_whitespace() {
        if width.is_none()
            && let Some(length) = parse_border_width_value(token)
        {
            width = Some(length);
            continue;
        }

        if border_style.is_none()
            && let Some(parsed_style) = parse_border_style(token)
        {
            border_style = Some(parsed_style);
            continue;
        }

        if color.is_none()
            && let Some(parsed_color) = parse_border_color(token)
        {
            color = Some(parsed_color);
            continue;
        }

        return Vec::new();
    }

    let width = width.unwrap_or(SpecifiedLength::Px(3.0));
    let border_style = border_style.unwrap_or(BorderStyle::None);
    let color = color.unwrap_or(None);
    let mut declarations = Vec::with_capacity(6);

    for property in [
        Property::BorderTopWidth,
        Property::BorderRightWidth,
        Property::BorderBottomWidth,
        Property::BorderLeftWidth,
    ] {
        declarations.push((property, SpecifiedValue::EdgeLength(width)));
    }

    declarations.push((
        Property::BorderStyle,
        SpecifiedValue::BorderStyle(border_style),
    ));
    declarations.push((Property::BorderColor, SpecifiedValue::BorderColor(color)));

    declarations
}

fn expand_margin_edges(value: &str, properties: [Property; 4]) -> Vec<(Property, SpecifiedValue)> {
    let parsed = value
        .split_ascii_whitespace()
        .map(|part| parse_specified_length(part, true))
        .collect::<Option<Vec<_>>>();

    let Some(values) = parsed else {
        return Vec::new();
    };

    if values
        .iter()
        .any(|value| matches!(value, SpecifiedLength::Percent(_)))
    {
        return Vec::new();
    }

    let edges = match values.as_slice() {
        [all] => [*all, *all, *all, *all],

        [vertical, horizontal] => [*vertical, *horizontal, *vertical, *horizontal],

        [top, horizontal, bottom] => [*top, *horizontal, *bottom, *horizontal],

        [top, right, bottom, left] => [*top, *right, *bottom, *left],

        _ => return Vec::new(),
    };

    properties
        .into_iter()
        .zip(edges)
        .map(|(property, length)| (property, SpecifiedValue::MarginLength(length)))
        .collect()
}

fn expand_edges(value: &str, properties: [Property; 4]) -> Vec<(Property, SpecifiedValue)> {
    let parsed = value
        .split_ascii_whitespace()
        .map(|part| parse_specified_length(part, false))
        .collect::<Option<Vec<_>>>();

    let Some(values) = parsed else {
        return Vec::new();
    };

    if values.iter().any(|value| {
        matches!(value, SpecifiedLength::Auto | SpecifiedLength::Percent(_))
            || !specified_length_is_non_negative(*value)
    }) {
        return Vec::new();
    }

    let edges = match values.as_slice() {
        [all] => [*all, *all, *all, *all],

        [vertical, horizontal] => [*vertical, *horizontal, *vertical, *horizontal],

        [top, horizontal, bottom] => [*top, *horizontal, *bottom, *horizontal],

        [top, right, bottom, left] => [*top, *right, *bottom, *left],

        _ => return Vec::new(),
    };

    properties
        .into_iter()
        .zip(edges)
        .map(|(property, length)| (property, SpecifiedValue::EdgeLength(length)))
        .collect()
}

fn parse_object_fit(value: &str) -> Option<ObjectFit> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fill" => Some(ObjectFit::Fill),
        "contain" => Some(ObjectFit::Contain),
        "cover" => Some(ObjectFit::Cover),
        "none" => Some(ObjectFit::None),
        "scale-down" => Some(ObjectFit::ScaleDown),
        _ => None,
    }
}

// PHANTOM_2H5_IMAGE_TEXT_COMPOSITION_I
fn parse_object_position(value: &str) -> Option<ObjectPosition> {
    let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();

    match tokens.as_slice() {
        [single] => parse_object_position_single(single),
        [first, second] => parse_object_position_pair(first, second),
        [first, second, third] => parse_object_position_three(first, second, third),
        [first, second, third, fourth] => {
            let first = parse_object_position_edge_offset(first, second)?;
            let second = parse_object_position_edge_offset(third, fourth)?;
            object_position_from_axes(first, second)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObjectPositionAxis {
    Horizontal,
    Vertical,
}

fn parse_object_position_percentage(token: &str) -> Option<f32> {
    token
        .strip_suffix('%')
        .and_then(parse_number)
        .map(|percent| (percent / 100.0).clamp(0.0, 1.0))
}

fn parse_object_position_keyword(token: &str) -> Option<(ObjectPositionAxis, f32)> {
    match token.to_ascii_lowercase().as_str() {
        "left" => Some((ObjectPositionAxis::Horizontal, 0.0)),
        "right" => Some((ObjectPositionAxis::Horizontal, 1.0)),
        "top" => Some((ObjectPositionAxis::Vertical, 0.0)),
        "bottom" => Some((ObjectPositionAxis::Vertical, 1.0)),
        _ => None,
    }
}

fn parse_object_position_component(token: &str, horizontal: bool) -> Option<f32> {
    let lower = token.to_ascii_lowercase();
    match lower.as_str() {
        "left" if horizontal => Some(0.0),
        "right" if horizontal => Some(1.0),
        "top" if !horizontal => Some(0.0),
        "bottom" if !horizontal => Some(1.0),
        "center" => Some(0.5),
        _ => parse_object_position_percentage(&lower),
    }
}

fn parse_object_position_single(token: &str) -> Option<ObjectPosition> {
    let lower = token.to_ascii_lowercase();
    if lower == "top" || lower == "bottom" {
        Some(ObjectPosition::new(
            0.5,
            parse_object_position_component(token, false)?,
        ))
    } else {
        Some(ObjectPosition::new(
            parse_object_position_component(token, true)?,
            0.5,
        ))
    }
}

fn parse_object_position_pair(first: &str, second: &str) -> Option<ObjectPosition> {
    if let (Some(x), Some(y)) = (
        parse_object_position_component(first, true),
        parse_object_position_component(second, false),
    ) {
        return Some(ObjectPosition::new(x, y));
    }

    Some(ObjectPosition::new(
        parse_object_position_component(second, true)?,
        parse_object_position_component(first, false)?,
    ))
}

fn parse_object_position_edge_offset(
    edge: &str,
    offset: &str,
) -> Option<(ObjectPositionAxis, f32)> {
    let (axis, anchor) = parse_object_position_keyword(edge)?;
    let offset = parse_object_position_percentage(offset)?;
    let value = if anchor <= f32::EPSILON {
        offset
    } else {
        1.0 - offset
    };

    Some((axis, value.clamp(0.0, 1.0)))
}

fn parse_object_position_three(first: &str, second: &str, third: &str) -> Option<ObjectPosition> {
    if let Some(pair) = parse_object_position_edge_offset(first, second)
        && let Some(single) = parse_object_position_keyword(third)
    {
        return object_position_from_axes(pair, single);
    }

    let single = parse_object_position_keyword(first)?;
    let pair = parse_object_position_edge_offset(second, third)?;
    object_position_from_axes(single, pair)
}

fn object_position_from_axes(
    first: (ObjectPositionAxis, f32),
    second: (ObjectPositionAxis, f32),
) -> Option<ObjectPosition> {
    match (first, second) {
        ((ObjectPositionAxis::Horizontal, x), (ObjectPositionAxis::Vertical, y))
        | ((ObjectPositionAxis::Vertical, y), (ObjectPositionAxis::Horizontal, x)) => {
            Some(ObjectPosition::new(x, y))
        }
        _ => None,
    }
}

fn parse_display(value: &str) -> Option<Display> {
    match value.trim().to_ascii_lowercase().as_str() {
        "block" => Some(Display::Block),
        "inline" | "inline-block" => Some(Display::Inline),
        "none" => Some(Display::None),
        "flex" | "inline-flex" => Some(Display::Flex),
        _ => None,
    }
}

fn parse_position(value: &str) -> Option<Position> {
    match value.trim().to_ascii_lowercase().as_str() {
        "static" => Some(Position::Static),
        "relative" => Some(Position::Relative),
        "absolute" => Some(Position::Absolute),
        "fixed" => Some(Position::Fixed),
        _ => None,
    }
}

fn parse_z_index(value: &str) -> Option<ZIndex> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("auto") {
        return Some(ZIndex::Auto);
    }

    let integer = trimmed.parse::<i32>().ok()?;
    if (integer as f32).abs() > MAX_NUMERIC_MAGNITUDE {
        return None;
    }

    Some(ZIndex::Integer(integer))
}

fn parse_offset_property(property: Property, value: &str) -> Vec<(Property, SpecifiedValue)> {
    parse_specified_length(value, true)
        .map(|length| vec![(property, SpecifiedValue::OffsetLength(length))])
        .unwrap_or_default()
}

fn parse_white_space(value: &str) -> Option<WhiteSpace> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(WhiteSpace::Normal),
        "nowrap" => Some(WhiteSpace::NoWrap),
        "pre" => Some(WhiteSpace::Pre),
        "pre-wrap" => Some(WhiteSpace::PreWrap),
        "pre-line" => Some(WhiteSpace::PreLine),
        _ => None,
    }
}

fn parse_line_height(value: &str) -> Option<SpecifiedLineHeight> {
    let normalized = value.trim().to_ascii_lowercase();

    if normalized == "normal" {
        return Some(SpecifiedLineHeight::Normal);
    }

    if let Some(percent) = normalized.strip_suffix('%') {
        return parse_non_negative_float(percent).map(SpecifiedLineHeight::Percent);
    }

    if let Some(length) = parse_specified_length(&normalized, false)
        && specified_length_is_non_negative(length)
    {
        return Some(SpecifiedLineHeight::Length(length));
    }

    parse_non_negative_float(&normalized).map(SpecifiedLineHeight::Number)
}

fn resolve_line_height(line_height: SpecifiedLineHeight, font_size: f32) -> Option<LineHeight> {
    match line_height {
        SpecifiedLineHeight::Normal => Some(LineHeight::Normal),
        SpecifiedLineHeight::Number(multiplier) => Some(LineHeight::Number(multiplier)),
        SpecifiedLineHeight::Length(SpecifiedLength::Px(value)) => {
            Some(LineHeight::Px(value.max(0.0)))
        }
        SpecifiedLineHeight::Length(SpecifiedLength::Em(value)) => {
            bounded_computed(value * font_size).map(|value| LineHeight::Px(value.max(0.0)))
        }
        SpecifiedLineHeight::Length(SpecifiedLength::Rem(value)) => {
            bounded_computed(value * ROOT_FONT_SIZE_PX).map(|value| LineHeight::Px(value.max(0.0)))
        }
        SpecifiedLineHeight::Percent(percent) => bounded_computed(font_size * percent / 100.0)
            .map(|value| LineHeight::Px(value.max(0.0))),
        SpecifiedLineHeight::Length(SpecifiedLength::Auto | SpecifiedLength::Percent(_)) => None,
    }
}

fn parse_font_weight(value: &str) -> Option<FontWeight> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" | "400" | "500" => Some(FontWeight::Normal),
        "bold" | "600" | "700" | "800" | "900" => Some(FontWeight::Bold),
        _ => None,
    }
}

fn parse_font_style(value: &str) -> Option<FontStyle> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(FontStyle::Normal),
        "italic" | "oblique" => Some(FontStyle::Italic),
        _ => None,
    }
}

fn parse_font_size_length(value: &str) -> Option<SpecifiedLength> {
    let length = parse_specified_length(value, true)?;

    if matches!(length, SpecifiedLength::Auto) || !specified_length_is_non_negative(length) {
        None
    } else {
        Some(length)
    }
}

fn parse_specified_length(value: &str, allow_auto_percent: bool) -> Option<SpecifiedLength> {
    let normalized = value.trim().to_ascii_lowercase();

    if allow_auto_percent && normalized == "auto" {
        return Some(SpecifiedLength::Auto);
    }

    if !normalized
        .bytes()
        .any(|byte| byte.is_ascii_alphabetic() || byte == b'%')
    {
        let number = parse_number(&normalized)?;
        return (number == 0.0).then_some(SpecifiedLength::Px(0.0));
    }

    if let Some(number) = normalized.strip_suffix("px") {
        return parse_number(number).map(SpecifiedLength::Px);
    }

    if let Some(number) = normalized.strip_suffix("rem") {
        return parse_number(number).map(SpecifiedLength::Rem);
    }

    if let Some(number) = normalized.strip_suffix("em") {
        return parse_number(number).map(SpecifiedLength::Em);
    }

    for (suffix, css_px_per_unit) in [
        ("in", 96.0_f32),
        ("cm", 96.0_f32 / 2.54_f32),
        ("mm", 96.0_f32 / 25.4_f32),
        ("q", 96.0_f32 / 101.6_f32),
        ("pt", 96.0_f32 / 72.0_f32),
        ("pc", 16.0_f32),
    ] {
        if let Some(number) = normalized.strip_suffix(suffix) {
            return parse_number(number)
                .and_then(|value| bounded_computed(value * css_px_per_unit))
                .map(SpecifiedLength::Px);
        }
    }

    if allow_auto_percent && let Some(number) = normalized.strip_suffix('%') {
        return parse_number(number).map(SpecifiedLength::Percent);
    }

    None
}

fn specified_length_is_non_negative(length: SpecifiedLength) -> bool {
    match length {
        SpecifiedLength::Auto => true,
        SpecifiedLength::Px(value)
        | SpecifiedLength::Em(value)
        | SpecifiedLength::Rem(value)
        | SpecifiedLength::Percent(value) => value >= 0.0,
    }
}

fn resolve_font_size(length: SpecifiedLength, inherited_size: f32) -> Option<f32> {
    match length {
        SpecifiedLength::Px(value) => Some(value),
        SpecifiedLength::Em(value) => bounded_computed(value * inherited_size),
        SpecifiedLength::Rem(value) => bounded_computed(value * ROOT_FONT_SIZE_PX),
        SpecifiedLength::Percent(value) => bounded_computed(inherited_size * value / 100.0),
        SpecifiedLength::Auto => None,
    }
}

fn resolve_margin_length(length: SpecifiedLength, font_size: f32) -> (f32, bool) {
    match length {
        SpecifiedLength::Auto => (0.0, true),

        _ => (
            resolve_edge_length(length, font_size).unwrap_or_default(),
            false,
        ),
    }
}

fn resolve_edge_length(length: SpecifiedLength, font_size: f32) -> Option<f32> {
    match length {
        SpecifiedLength::Px(value) => Some(value),
        SpecifiedLength::Em(value) => bounded_computed(value * font_size),
        SpecifiedLength::Rem(value) => bounded_computed(value * ROOT_FONT_SIZE_PX),
        SpecifiedLength::Auto | SpecifiedLength::Percent(_) => None,
    }
}

fn resolve_offset_length(length: SpecifiedLength, font_size: f32) -> Length {
    match length {
        SpecifiedLength::Auto => Length::Auto,
        SpecifiedLength::Px(value) => Length::Px(value),
        SpecifiedLength::Em(value) => {
            Length::Px(bounded_computed(value * font_size).unwrap_or_default())
        }
        SpecifiedLength::Rem(value) => {
            Length::Px(bounded_computed(value * ROOT_FONT_SIZE_PX).unwrap_or_default())
        }
        SpecifiedLength::Percent(value) => Length::Percent(value),
    }
}

fn resolve_size_length(length: SpecifiedLength, font_size: f32) -> Length {
    match length {
        SpecifiedLength::Auto => Length::Auto,
        SpecifiedLength::Px(value) => Length::Px(value.max(0.0)),
        SpecifiedLength::Em(value) => Length::Px(
            bounded_computed(value * font_size)
                .unwrap_or_default()
                .max(0.0),
        ),
        SpecifiedLength::Rem(value) => Length::Px(
            bounded_computed(value * ROOT_FONT_SIZE_PX)
                .unwrap_or_default()
                .max(0.0),
        ),
        SpecifiedLength::Percent(value) => Length::Percent(value),
    }
}

fn parse_number(value: &str) -> Option<f32> {
    value
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|number| number.is_finite() && number.abs() <= MAX_NUMERIC_MAGNITUDE)
}

fn bounded_computed(value: f32) -> Option<f32> {
    if value.is_finite() {
        Some(value.clamp(-MAX_NUMERIC_MAGNITUDE, MAX_NUMERIC_MAGNITUDE))
    } else {
        None
    }
}

fn sanitize_unit_interval(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.5
    }
}

fn sanitize_opacity(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

fn parse_background_shorthand(value: &str) -> Vec<(Property, SpecifiedValue)> {
    let normalized = value.trim();

    if normalized.eq_ignore_ascii_case("none") {
        return vec![(
            Property::BackgroundColor,
            SpecifiedValue::BackgroundColor(None),
        )];
    }

    parse_background_color(normalized)
        .map(|color| {
            vec![(
                Property::BackgroundColor,
                SpecifiedValue::BackgroundColor(color),
            )]
        })
        .unwrap_or_default()
}

fn parse_background_color(value: &str) -> Option<Option<Rgba>> {
    if value.trim().eq_ignore_ascii_case("transparent") {
        Some(None)
    } else {
        parse_color(value).map(Some)
    }
}

fn parse_color(value: &str) -> Option<Rgba> {
    let normalized = value.trim().to_ascii_lowercase();

    match normalized.as_str() {
        "black" => return Some(Rgba::new(0, 0, 0, 255)),
        "silver" => return Some(Rgba::new(192, 192, 192, 255)),
        "gray" | "grey" => return Some(Rgba::new(128, 128, 128, 255)),
        "white" => return Some(Rgba::new(255, 255, 255, 255)),
        "maroon" => return Some(Rgba::new(128, 0, 0, 255)),
        "red" => return Some(Rgba::new(255, 0, 0, 255)),
        "purple" => return Some(Rgba::new(128, 0, 128, 255)),
        "fuchsia" | "magenta" => return Some(Rgba::new(255, 0, 255, 255)),
        "green" => return Some(Rgba::new(0, 128, 0, 255)),
        "lime" => return Some(Rgba::new(0, 255, 0, 255)),
        "olive" => return Some(Rgba::new(128, 128, 0, 255)),
        "yellow" => return Some(Rgba::new(255, 255, 0, 255)),
        "navy" => return Some(Rgba::new(0, 0, 128, 255)),
        "blue" => return Some(Rgba::new(0, 0, 255, 255)),
        "teal" => return Some(Rgba::new(0, 128, 128, 255)),
        "aqua" | "cyan" => return Some(Rgba::new(0, 255, 255, 255)),
        "transparent" => return Some(Rgba::new(0, 0, 0, 0)),
        _ => {}
    }

    if let Some(hex) = normalized.strip_prefix('#') {
        return parse_hex_color(hex);
    }

    if let Some(inner) = normalized
        .strip_prefix("rgb(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return parse_rgb_function(inner, 255);
    }

    if let Some(inner) = normalized
        .strip_prefix("rgba(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let channels: Vec<&str> = inner.split(',').map(str::trim).collect();

        if channels.len() != 4 {
            return None;
        }

        let red = parse_rgb_channel(channels[0])?;
        let green = parse_rgb_channel(channels[1])?;
        let blue = parse_rgb_channel(channels[2])?;
        let alpha = parse_alpha(channels[3])?;

        return Some(Rgba::new(red, green, blue, alpha));
    }

    None
}

fn parse_rgb_function(inner: &str, alpha: u8) -> Option<Rgba> {
    let channels: Vec<u8> = inner
        .split(',')
        .map(parse_rgb_channel)
        .collect::<Option<Vec<_>>>()?;

    if let [red, green, blue] = channels.as_slice() {
        Some(Rgba::new(*red, *green, *blue, alpha))
    } else {
        None
    }
}

fn parse_rgb_channel(value: &str) -> Option<u8> {
    let normalized = value.trim();

    if let Some(percent) = normalized.strip_suffix('%') {
        let percent = parse_number(percent)?;
        if !(0.0..=100.0).contains(&percent) {
            return None;
        }

        return Some((percent * 255.0 / 100.0).round() as u8);
    }

    normalized.parse::<u8>().ok()
}

fn parse_alpha(value: &str) -> Option<u8> {
    let normalized = value.trim();

    let alpha = if let Some(percent) = normalized.strip_suffix('%') {
        (parse_number(percent)? / 100.0).clamp(0.0, 1.0)
    } else {
        parse_number(normalized)?.clamp(0.0, 1.0)
    };

    Some((alpha * 255.0).round() as u8)
}

fn parse_hex_color(hex: &str) -> Option<Rgba> {
    match hex.len() {
        3 => {
            let mut chars = hex.chars();
            let red = expand_hex_nibble(chars.next()?)?;
            let green = expand_hex_nibble(chars.next()?)?;
            let blue = expand_hex_nibble(chars.next()?)?;

            Some(Rgba::new(red, green, blue, 255))
        }

        4 => {
            let mut chars = hex.chars();
            let red = expand_hex_nibble(chars.next()?)?;
            let green = expand_hex_nibble(chars.next()?)?;
            let blue = expand_hex_nibble(chars.next()?)?;
            let alpha = expand_hex_nibble(chars.next()?)?;

            Some(Rgba::new(red, green, blue, alpha))
        }

        6 => {
            let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;

            Some(Rgba::new(red, green, blue, 255))
        }

        8 => {
            let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let alpha = u8::from_str_radix(&hex[6..8], 16).ok()?;

            Some(Rgba::new(red, green, blue, alpha))
        }

        _ => None,
    }
}

fn expand_hex_nibble(character: char) -> Option<u8> {
    let value = character.to_digit(16)?;
    u8::try_from(value.saturating_mul(17)).ok()
}

fn remove_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    let mut quote = None;
    let mut escaped = false;

    while cursor < bytes.len() {
        let byte = bytes[cursor];

        if let Some(active_quote) = quote {
            output.push(char::from(byte));

            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }

            cursor += 1;
            continue;
        }

        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
            output.push(char::from(byte));
            cursor += 1;
            continue;
        }

        if byte == b'/' && bytes.get(cursor.saturating_add(1)) == Some(&b'*') {
            cursor = cursor.saturating_add(2);

            while cursor < bytes.len() {
                if bytes[cursor] == b'*' && bytes.get(cursor.saturating_add(1)) == Some(&b'/') {
                    cursor = cursor.saturating_add(2);
                    break;
                }

                cursor += 1;
            }

            continue;
        }

        if byte.is_ascii() {
            output.push(char::from(byte));
            cursor += 1;
        } else {
            let Some(character) = source[cursor..].chars().next() else {
                break;
            };

            output.push(character);
            cursor += character.len_utf8();
        }
    }

    output
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct StyleKey {
    display: Display,
    position: Position,
    z_index: ZIndex,
    top: LengthKey,
    right: LengthKey,
    bottom: LengthKey,
    left: LengthKey,
    color: Rgba,
    background_color: Option<Rgba>,
    font_size: u32,
    line_height: LineHeightKey,
    white_space: WhiteSpace,
    font_weight: FontWeight,
    font_style: FontStyle,
    font_family: FontFamily,
    underline: bool,
    margin: EdgeKey,
    margin_auto: AutoEdges,
    padding: EdgeKey,
    border_width: EdgeKey,
    border_color: Option<Rgba>,
    border_style: BorderStyle,
    border_radius: LengthKey,
    overflow: Overflow,
    opacity: u32,
    box_sizing: BoxSizing,
    object_fit: ObjectFit,
    object_position_x: u32,
    object_position_y: u32,
    width: LengthKey,
    min_width: LengthKey,
    max_width: LengthKey,
    height: LengthKey,
    min_height: LengthKey,
    max_height: LengthKey,
    flex_direction: FlexDirection,
    flex_wrap: FlexWrap,
    justify_content: JustifyContent,
    align_items: AlignItems,
    align_content: AlignContent,
    align_self: AlignSelf,
    gap: LengthKey,
    flex_grow: u32,
    flex_shrink: u32,
    flex_basis: LengthKey,
}

impl From<&ComputedStyle> for StyleKey {
    fn from(style: &ComputedStyle) -> Self {
        Self {
            display: style.display,
            position: style.position,
            z_index: style.z_index,
            top: LengthKey::from(style.top),
            right: LengthKey::from(style.right),
            bottom: LengthKey::from(style.bottom),
            left: LengthKey::from(style.left),
            color: style.color,
            background_color: style.background_color,
            font_size: style.font_size.to_bits(),
            line_height: LineHeightKey::from(style.line_height),
            white_space: style.white_space,
            font_weight: style.font_weight,
            font_style: style.font_style,
            font_family: style.font_family,
            underline: style.underline,
            margin: EdgeKey::from(style.margin),
            margin_auto: style.margin_auto,
            padding: EdgeKey::from(style.padding),
            border_width: EdgeKey::from(style.border_width),
            border_color: style.border_color,
            border_style: style.border_style,
            border_radius: LengthKey::from(style.border_radius),
            overflow: style.overflow,
            opacity: style.opacity.to_bits(),
            box_sizing: style.box_sizing,
            object_fit: style.object_fit,
            object_position_x: style.object_position.x.to_bits(),
            object_position_y: style.object_position.y.to_bits(),
            width: LengthKey::from(style.width),
            min_width: LengthKey::from(style.min_width),
            max_width: LengthKey::from(style.max_width),
            height: LengthKey::from(style.height),
            min_height: LengthKey::from(style.min_height),
            max_height: LengthKey::from(style.max_height),
            flex_direction: style.flex_direction,
            flex_wrap: style.flex_wrap,
            justify_content: style.justify_content,
            align_items: style.align_items,
            align_content: style.align_content,
            align_self: style.align_self,
            gap: LengthKey::from(style.gap),
            flex_grow: style.flex_grow.to_bits(),
            flex_shrink: style.flex_shrink.to_bits(),
            flex_basis: LengthKey::from(style.flex_basis),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LineHeightKey {
    Normal,
    Number(u32),
    Px(u32),
}

impl From<LineHeight> for LineHeightKey {
    fn from(line_height: LineHeight) -> Self {
        match line_height {
            LineHeight::Normal => Self::Normal,
            LineHeight::Number(value) => Self::Number(value.to_bits()),
            LineHeight::Px(value) => Self::Px(value.to_bits()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeKey {
    top: u32,
    right: u32,
    bottom: u32,
    left: u32,
}

impl From<EdgeSizes> for EdgeKey {
    fn from(edges: EdgeSizes) -> Self {
        Self {
            top: edges.top.to_bits(),
            right: edges.right.to_bits(),
            bottom: edges.bottom.to_bits(),
            left: edges.left.to_bits(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LengthKey {
    Auto,
    Px(u32),
    Percent(u32),
}

impl From<Length> for LengthKey {
    fn from(length: Length) -> Self {
        match length {
            Length::Auto => Self::Auto,
            Length::Px(value) => Self::Px(value.to_bits()),
            Length::Percent(value) => Self::Percent(value.to_bits()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use phantom_dom::{Document, ElementData, NodeKind};

    use super::{
        AlignContent, AlignSelf, ComputedStyle, Display, FlexDirection, FlexWrap, ObjectFit,
        ObjectPosition, Rgba, Stylesheet, compute_styles,
    };

    #[test]
    fn parses_compound_descendant_and_child_selectors() {
        let stylesheet = Stylesheet::parse(
            "main .card > p.note { color: red; }\
             #hero { display: block; }",
        );

        assert_eq!(stylesheet.len(), 2);
    }

    #[test]
    fn descendant_child_and_specificity_are_applied() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let style = document.append_child(root, NodeKind::Element(ElementData::new("style")))?;

        document.append_child(
            style,
            NodeKind::Text(
                "div p { color: red; }\
                 div > p.note { color: green; }\
                 #hero { color: blue; }"
                    .to_owned(),
            ),
        )?;

        let div = document.append_child(root, NodeKind::Element(ElementData::new("div")))?;

        let mut attributes = BTreeMap::new();
        attributes.insert("id".to_owned(), "hero".to_owned());
        attributes.insert("class".to_owned(), "note".to_owned());

        let paragraph = document.append_child(
            div,
            NodeKind::Element(ElementData::with_attributes("p", attributes)),
        )?;

        let styles = compute_styles(&document);

        assert_eq!(
            styles.get(paragraph).map(|style| style.color()),
            Some(Rgba::new(0, 0, 255, 255))
        );

        Ok(())
    }

    #[test]
    fn important_author_rule_beats_normal_inline_style() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let style = document.append_child(root, NodeKind::Element(ElementData::new("style")))?;

        document.append_child(
            style,
            NodeKind::Text("#hero { color: red !important; }".to_owned()),
        )?;

        let mut attributes = BTreeMap::new();
        attributes.insert("id".to_owned(), "hero".to_owned());
        attributes.insert("style".to_owned(), "color: blue".to_owned());

        let paragraph = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("p", attributes)),
        )?;

        let styles = compute_styles(&document);

        assert_eq!(
            styles.get(paragraph).map(|style| style.color()),
            Some(Rgba::new(255, 0, 0, 255))
        );

        Ok(())
    }

    #[test]
    fn inline_important_beats_author_important() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let style = document.append_child(root, NodeKind::Element(ElementData::new("style")))?;

        document.append_child(
            style,
            NodeKind::Text("#hero { color: red !important; }".to_owned()),
        )?;

        let mut attributes = BTreeMap::new();
        attributes.insert("id".to_owned(), "hero".to_owned());
        attributes.insert("style".to_owned(), "color: blue !important".to_owned());

        let paragraph = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("p", attributes)),
        )?;

        let styles = compute_styles(&document);

        assert_eq!(
            styles.get(paragraph).map(|style| style.color()),
            Some(Rgba::new(0, 0, 255, 255))
        );

        Ok(())
    }

    #[test]
    fn shorthand_expands_before_cascade() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let mut attributes = BTreeMap::new();
        attributes.insert(
            "style".to_owned(),
            "margin: 10px 20px; margin-left: 30px".to_owned(),
        );

        let element = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("div", attributes)),
        )?;

        let styles = compute_styles(&document);
        let margin = styles.get(element).map(|style| style.margin());

        assert_eq!(margin.map(|edges| edges.top()), Some(10.0));
        assert_eq!(margin.map(|edges| edges.right()), Some(20.0));
        assert_eq!(margin.map(|edges| edges.bottom()), Some(10.0));
        assert_eq!(margin.map(|edges| edges.left()), Some(30.0));

        Ok(())
    }

    #[test]
    fn computed_styles_are_interned() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        for _ in 0..32 {
            document.append_child(root, NodeKind::Element(ElementData::new("span")))?;
        }

        let styles = compute_styles(&document);

        assert_eq!(styles.len(), 33);
        assert!(styles.unique_len() < styles.len());

        Ok(())
    }

    #[test]
    fn display_none_is_still_computed() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let mut attributes = BTreeMap::new();
        attributes.insert("style".to_owned(), "display: none".to_owned());

        let element = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("div", attributes)),
        )?;

        let styles = compute_styles(&document);

        assert_eq!(
            styles.get(element).map(|style| style.display()),
            Some(Display::None)
        );

        Ok(())
    }

    #[test]
    fn box_model_properties_are_computed() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let mut attributes = BTreeMap::new();
        attributes.insert(
            "style".to_owned(),
            "box-sizing: border-box; \
             border: 4px solid #123456; \
             min-width: 120px; \
             max-width: 240px; \
             min-height: 40px; \
             max-height: 80px"
                .to_owned(),
        );

        let element = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("div", attributes)),
        )?;

        let styles = compute_styles(&document);
        let computed = styles.get(element);

        assert_eq!(
            computed.map(|style| style.box_sizing()),
            Some(super::BoxSizing::BorderBox)
        );

        assert_eq!(
            computed.map(|style| style.border_style()),
            Some(super::BorderStyle::Solid)
        );

        assert_eq!(computed.map(|style| style.border_width().top()), Some(4.0));

        assert_eq!(
            computed.map(|style| style.border_color()),
            Some(Rgba::new(18, 52, 86, 255))
        );

        assert_eq!(
            computed.map(|style| style.min_width()),
            Some(super::Length::Px(120.0))
        );

        assert_eq!(
            computed.map(|style| style.max_width()),
            Some(super::Length::Px(240.0))
        );

        Ok(())
    }

    #[test]
    fn box_visual_properties_are_computed_and_bounded() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let mut attributes = BTreeMap::new();
        attributes.insert(
            "style".to_owned(),
            "border-radius: 25%; overflow: hidden; opacity: 150%".to_owned(),
        );

        let element = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("div", attributes)),
        )?;

        let styles = compute_styles(&document);
        let computed = styles.get(element);

        assert_eq!(
            computed.map(|style| style.border_radius()),
            Some(super::Length::Percent(25.0))
        );
        assert_eq!(
            computed.map(|style| style.overflow()),
            Some(super::Overflow::Hidden)
        );
        assert_eq!(computed.map(|style| style.opacity()), Some(1.0));

        Ok(())
    }

    #[test]
    fn border_none_removes_effective_border_width() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let mut attributes = BTreeMap::new();
        attributes.insert(
            "style".to_owned(),
            "border-width: 10px; border-style: none".to_owned(),
        );

        let element = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("div", attributes)),
        )?;

        let styles = compute_styles(&document);

        assert_eq!(
            styles.get(element).map(|style| style.border_width().left()),
            Some(0.0)
        );

        Ok(())
    }

    #[test]
    fn flex_core_properties_are_computed() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let mut attributes = BTreeMap::new();
        attributes.insert(
            "style".to_owned(),
            "display: flex; \
             flex-direction: column; \
             justify-content: space-between; \
             align-items: center; \
             gap: 12px; \
             flex-grow: 2; \
             flex-shrink: 0.5; \
             flex-basis: 80px"
                .to_owned(),
        );

        let element = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("div", attributes)),
        )?;

        let styles = compute_styles(&document);
        let computed = styles.get(element);

        assert_eq!(computed.map(|style| style.display()), Some(Display::Flex));

        assert_eq!(
            computed.map(|style| style.flex_direction()),
            Some(super::FlexDirection::Column)
        );

        assert_eq!(
            computed.map(|style| style.justify_content()),
            Some(super::JustifyContent::SpaceBetween)
        );

        assert_eq!(
            computed.map(|style| style.align_items()),
            Some(super::AlignItems::Center)
        );

        assert_eq!(
            computed.map(|style| style.gap()),
            Some(super::Length::Px(12.0))
        );

        assert_eq!(computed.map(|style| style.flex_grow()), Some(2.0));

        assert_eq!(computed.map(|style| style.flex_shrink()), Some(0.5));

        assert_eq!(
            computed.map(|style| style.flex_basis()),
            Some(super::Length::Px(80.0))
        );

        Ok(())
    }

    #[test]
    fn flex_shorthand_expands_to_longhands() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let mut attributes = BTreeMap::new();
        attributes.insert("style".to_owned(), "flex: 2 0.5 25%".to_owned());

        let element = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("div", attributes)),
        )?;

        let styles = compute_styles(&document);
        let computed = styles.get(element);

        assert_eq!(computed.map(|style| style.flex_grow()), Some(2.0));

        assert_eq!(computed.map(|style| style.flex_shrink()), Some(0.5));

        assert_eq!(
            computed.map(|style| style.flex_basis()),
            Some(super::Length::Percent(25.0))
        );

        Ok(())
    }

    #[test]
    fn flex_wrapping_and_alignment_properties_are_computed() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let mut attributes = BTreeMap::new();
        attributes.insert(
            "style".to_owned(),
            "display:flex;\
             flex-direction:row-reverse;\
             flex-wrap:wrap;\
             align-content:space-between;\
             align-self:center"
                .to_owned(),
        );

        let element = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("div", attributes)),
        )?;

        let styles = compute_styles(&document);
        let computed = styles.get(element);

        assert_eq!(
            computed.map(ComputedStyle::flex_direction),
            Some(FlexDirection::RowReverse)
        );

        assert_eq!(computed.map(ComputedStyle::flex_wrap), Some(FlexWrap::Wrap));

        assert_eq!(
            computed.map(ComputedStyle::align_content),
            Some(AlignContent::SpaceBetween)
        );

        assert_eq!(
            computed.map(ComputedStyle::align_self),
            Some(AlignSelf::Center)
        );

        Ok(())
    }

    #[test]
    fn flex_flow_sets_direction_and_wrap_reverse() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let mut attributes = BTreeMap::new();
        attributes.insert(
            "style".to_owned(),
            "display:flex;\
             flex-flow:column-reverse wrap-reverse"
                .to_owned(),
        );

        let element = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("div", attributes)),
        )?;

        let styles = compute_styles(&document);
        let computed = styles.get(element);

        assert_eq!(
            computed.map(ComputedStyle::flex_direction,),
            Some(FlexDirection::ColumnReverse)
        );

        assert_eq!(
            computed.map(ComputedStyle::flex_wrap,),
            Some(FlexWrap::WrapReverse)
        );

        Ok(())
    }

    #[test]
    fn flex_flow_single_component_resets_other_axis() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let mut attributes = BTreeMap::new();
        attributes.insert(
            "style".to_owned(),
            "flex-direction:column;\
             flex-wrap:wrap;\
             flex-flow:row-reverse"
                .to_owned(),
        );

        let element = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("div", attributes)),
        )?;

        let styles = compute_styles(&document);
        let computed = styles.get(element);

        assert_eq!(
            computed.map(ComputedStyle::flex_direction,),
            Some(FlexDirection::RowReverse)
        );

        assert_eq!(
            computed.map(ComputedStyle::flex_wrap,),
            Some(FlexWrap::NoWrap)
        );

        Ok(())
    }

    #[test]
    fn margin_auto_is_preserved_as_semantic_state() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();

        let mut attributes = BTreeMap::new();
        attributes.insert(
            "style".to_owned(),
            "margin: 4px auto 8px; display: block".to_owned(),
        );

        let element = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("div", attributes)),
        )?;

        let styles = compute_styles(&document);
        let style = styles.get(element).cloned().unwrap_or_default();

        assert_eq!(style.margin().top(), 4.0);
        assert_eq!(style.margin().right(), 0.0);
        assert_eq!(style.margin().bottom(), 8.0);
        assert_eq!(style.margin().left(), 0.0);
        assert!(!style.margin_auto().top());
        assert!(style.margin_auto().right());
        assert!(!style.margin_auto().bottom());
        assert!(style.margin_auto().left());

        Ok(())
    }

    #[test]
    fn object_sizing_properties_are_computed() -> Result<(), phantom_dom::DomError> {
        let mut document = Document::new();
        let root = document.root();
        let mut attributes = BTreeMap::new();
        attributes.insert(
            "style".to_owned(),
            "object-fit: cover; object-position: 25% 75%".to_owned(),
        );
        let image = document.append_child(
            root,
            NodeKind::Element(ElementData::with_attributes("img", attributes)),
        )?;
        let styles = compute_styles(&document);
        assert_eq!(
            styles.get(image).map(ComputedStyle::object_fit),
            Some(ObjectFit::Cover),
        );
        assert_eq!(
            styles.get(image).map(ComputedStyle::object_position),
            Some(ObjectPosition::new(0.25, 0.75)),
        );
        Ok(())
    }
}
