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

use phantom_dom::{Document, ElementData, Node, NodeId, NodeKind};

const ROOT_FONT_SIZE_PX: f32 = 16.0;
const NO_STYLE: usize = usize::MAX;

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

/// Border painting style supported by the first Phantom box model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BorderStyle {
    /// Border does not generate visible edge geometry.
    None,

    /// Solid border.
    Solid,
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
    color: Rgba,
    background_color: Option<Rgba>,
    font_size: f32,
    font_weight: FontWeight,
    font_style: FontStyle,
    font_family: FontFamily,
    underline: bool,
    margin: EdgeSizes,
    margin_auto: AutoEdges,
    padding: EdgeSizes,
    border_width: EdgeSizes,
    border_color: Option<Rgba>,
    border_style: BorderStyle,
    box_sizing: BoxSizing,
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
            color: Rgba::new(0, 0, 0, 255),
            background_color: None,
            font_size: ROOT_FONT_SIZE_PX,
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            font_family: FontFamily::SansSerif,
            underline: false,
            margin: EdgeSizes::zero(),
            margin_auto: AutoEdges::none(),
            padding: EdgeSizes::zero(),
            border_width: EdgeSizes::new(3.0, 3.0, 3.0, 3.0),
            border_color: None,
            border_style: BorderStyle::None,
            box_sizing: BoxSizing::ContentBox,
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

    /// Returns the box-sizing model used by layout.
    #[must_use]
    pub const fn box_sizing(&self) -> BoxSizing {
        self.box_sizing
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
    /// Unsupported at-rules, selectors, properties, and values are ignored.
    /// Syntax errors in one rule do not invalidate the entire stylesheet.
    #[must_use]
    pub fn parse(source: &str) -> Self {
        let cleaned = remove_comments(source);
        let blocks = scan_rule_blocks(&cleaned);
        let mut rules = Vec::new();
        let mut source_order = 0_u32;

        for block in blocks {
            if block.selector.trim_start().starts_with('@') {
                source_order = source_order.saturating_add(1);
                continue;
            }

            let declarations = parse_declarations(block.body);

            if declarations.is_empty() {
                source_order = source_order.saturating_add(1);
                continue;
            }

            for selector_source in block.selector.split(',') {
                let Some(selector) = Selector::parse(selector_source) else {
                    continue;
                };

                rules.push(StyleRule {
                    specificity: selector.specificity(),
                    selector,
                    declarations: declarations.clone(),
                    source_order,
                });
            }

            source_order = source_order.saturating_add(1);
        }

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
#[must_use]
pub fn compute_styles(document: &Document) -> StyleMap {
    let author_css = collect_author_css(document);
    let stylesheet = Stylesheet::parse(&author_css);

    let mut style_map = StyleMap::default();
    let mut interner = BTreeMap::new();
    let mut stack = vec![document.root()];

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
                parent_style.as_ref(),
                &mut style,
            );
        }

        style_map.insert(node_id, style, &mut interner);

        for child in node.children().iter().rev() {
            stack.push(*child);
        }
    }

    style_map
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
}

impl Property {
    const COUNT: usize = 39;

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
            Self::Width => 23,
            Self::MinWidth => 24,
            Self::MaxWidth => 25,
            Self::Height => 26,
            Self::MinHeight => 27,
            Self::MaxHeight => 28,
            Self::FlexDirection => 29,
            Self::FlexWrap => 30,
            Self::JustifyContent => 31,
            Self::AlignItems => 32,
            Self::AlignContent => 33,
            Self::AlignSelf => 34,
            Self::Gap => 35,
            Self::FlexGrow => 36,
            Self::FlexShrink => 37,
            Self::FlexBasis => 38,
        }
    }
}

#[derive(Debug, Clone)]
enum SpecifiedValue {
    Display(Display),
    Color(Rgba),
    BackgroundColor(Option<Rgba>),
    FontSize(SpecifiedLength),
    FontWeight(FontWeight),
    FontStyle(FontStyle),
    FontFamily(FontFamily),
    Underline(bool),
    MarginLength(SpecifiedLength),
    EdgeLength(SpecifiedLength),
    BorderColor(Option<Rgba>),
    BorderStyle(BorderStyle),
    BoxSizing(BoxSizing),
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
enum SpecifiedLength {
    Auto,
    Px(f32),
    Em(f32),
    Rem(f32),
    Percent(f32),
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
}

#[derive(Debug, Clone, Default)]
struct CompoundSelector {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    universal: bool,
}

impl Selector {
    fn parse(source: &str) -> Option<Self> {
        let source = source.trim();

        if source.is_empty()
            || source.starts_with('@')
            || source.contains('+')
            || source.contains('~')
            || source.contains(':')
            || source.contains('[')
        {
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

    fn matches(&self, document: &Document, node_id: NodeId) -> bool {
        let Some(last_index) = self.parts.len().checked_sub(1) else {
            return false;
        };

        let mut current = node_id;

        if !self.parts[last_index]
            .compound
            .matches_node(document, current)
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

                    if !left.compound.matches_node(document, parent) {
                        return false;
                    }

                    current = parent;
                }

                Combinator::Descendant => {
                    let Some(ancestor) = matching_ancestor(document, current, &left.compound)
                    else {
                        return false;
                    };

                    current = ancestor;
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
            .is_some_and(|byte| *byte != b'.' && *byte != b'#')
        {
            let start = cursor;

            while cursor < bytes.len() && bytes[cursor] != b'.' && bytes[cursor] != b'#' {
                if !is_identifier_byte(bytes[cursor]) {
                    return None;
                }

                cursor += 1;
            }

            if cursor == start {
                return None;
            }

            selector.tag = Some(source[start..cursor].to_ascii_lowercase());
        }

        while cursor < bytes.len() {
            let prefix = bytes[cursor];

            if prefix != b'.' && prefix != b'#' {
                return None;
            }

            cursor += 1;
            let start = cursor;

            while cursor < bytes.len() && bytes[cursor] != b'.' && bytes[cursor] != b'#' {
                if !is_identifier_byte(bytes[cursor]) {
                    return None;
                }

                cursor += 1;
            }

            if cursor == start {
                return None;
            }

            let value = source[start..cursor].to_owned();

            if prefix == b'#' {
                if selector.id.is_some() {
                    return None;
                }

                selector.id = Some(value);
            } else {
                selector.classes.push(value);
            }
        }

        if selector.universal
            || selector.tag.is_some()
            || selector.id.is_some()
            || !selector.classes.is_empty()
        {
            Some(selector)
        } else {
            None
        }
    }

    fn specificity(&self) -> Specificity {
        Specificity {
            ids: u16::from(self.id.is_some()),
            classes: u16::try_from(self.classes.len()).unwrap_or(u16::MAX),
            tags: u16::from(self.tag.is_some()),
        }
    }

    fn matches_node(&self, document: &Document, node_id: NodeId) -> bool {
        let Some(node) = document.node(node_id) else {
            return false;
        };

        let NodeKind::Element(element) = node.kind() else {
            return false;
        };

        self.matches_element(element)
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

        self.classes.iter().all(|class| {
            element_classes
                .split_ascii_whitespace()
                .any(|candidate| candidate == class)
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct RuleBlock<'a> {
    selector: &'a str,
    body: &'a str,
}

fn parse_selector_parts(source: &str) -> Option<Vec<SelectorPart>> {
    let mut parts = Vec::new();
    let mut buffer = String::new();
    let mut pending = None;
    let mut invalid = false;

    for character in source.chars() {
        match character {
            '>' => {
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

            character if character.is_whitespace() => {
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
                buffer.push(character);
            }
        }
    }

    if invalid || !flush_selector_part(&mut parts, &mut buffer, &mut pending) {
        return None;
    }

    if pending.is_some() {
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
) -> Option<NodeId> {
    let mut ancestor = parent_element(document, node_id);

    while let Some(ancestor_id) = ancestor {
        if selector.matches_node(document, ancestor_id) {
            return Some(ancestor_id);
        }

        ancestor = parent_element(document, ancestor_id);
    }

    None
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

fn apply_cascade(
    document: &Document,
    node_id: NodeId,
    stylesheet: &Stylesheet,
    parent_style: Option<&ComputedStyle>,
    style: &mut ComputedStyle,
) {
    let mut winners: [Option<CascadeWinner>; Property::COUNT] = array::from_fn(|_| None);

    for rule in &stylesheet.rules {
        if !rule.selector.matches(document, node_id) {
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
    if let Some(winner) = winner_for(winners, Property::FontSize)
        && let SpecifiedValue::FontSize(length) = winner
        && let Some(size) = resolve_font_size(*length, style.font_size)
    {
        style.font_size = size.max(1.0);
    }

    for property in [
        Property::Display,
        Property::Color,
        Property::BackgroundColor,
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
        Property::BoxSizing,
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

fn apply_value(
    style: &mut ComputedStyle,
    _parent_style: Option<&ComputedStyle>,
    property: Property,
    value: &SpecifiedValue,
) {
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

        (Property::BoxSizing, SpecifiedValue::BoxSizing(box_sizing)) => {
            style.box_sizing = *box_sizing;
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

        "code" | "pre" => {
            style.font_family = FontFamily::Monospace;
        }

        "a" => {
            style.color = Rgba::new(0, 0, 238, 255);
            style.underline = true;
        }

        _ => {}
    }

    style
}

fn default_display(tag: &str) -> Display {
    match tag {
        "head" | "style" | "script" | "meta" | "link" | "title" | "base" => Display::None,

        "html" | "body" | "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol"
        | "li" | "section" | "article" | "main" | "header" | "footer" | "nav" | "blockquote"
        | "pre" | "hr" => Display::Block,

        _ => Display::Inline,
    }
}

fn scan_rule_blocks(source: &str) -> Vec<RuleBlock<'_>> {
    let bytes = source.as_bytes();
    let mut blocks = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }

        if cursor >= bytes.len() {
            break;
        }

        let selector_start = cursor;
        let Some(open_brace) = find_unquoted_byte(bytes, cursor, b'{') else {
            break;
        };

        let selector = &source[selector_start..open_brace];
        let body_start = open_brace.saturating_add(1);
        let Some(close_brace) = find_matching_brace(bytes, body_start) else {
            break;
        };

        blocks.push(RuleBlock {
            selector,
            body: &source[body_start..close_brace],
        });

        cursor = close_brace.saturating_add(1);
    }

    blocks
}

fn find_unquoted_byte(bytes: &[u8], start: usize, needle: u8) -> Option<usize> {
    let mut cursor = start;
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
        } else if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
        } else if byte == needle {
            return Some(cursor);
        }

        cursor += 1;
    }

    None
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
        let Some((property_source, value_source)) = split_declaration(segment) else {
            continue;
        };

        let property_name = property_source.trim().to_ascii_lowercase();
        let (value_source, important) = strip_important(value_source);

        let expanded = parse_property(&property_name, value_source);

        for (property, value) in expanded {
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
                    parts.push(&source[start..cursor]);
                    start = cursor.saturating_add(1);
                }
                _ => {}
            }
        }

        cursor += 1;
    }

    parts.push(&source[start..]);

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

fn parse_property(property: &str, value: &str) -> Vec<(Property, SpecifiedValue)> {
    match property {
        "display" => parse_display(value)
            .map(|display| vec![(Property::Display, SpecifiedValue::Display(display))])
            .unwrap_or_default(),

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

        "background" => parse_background_color(value)
            .map(|color| {
                vec![(
                    Property::BackgroundColor,
                    SpecifiedValue::BackgroundColor(color),
                )]
            })
            .unwrap_or_default(),

        "font-size" => parse_specified_length(value, false)
            .map(|length| vec![(Property::FontSize, SpecifiedValue::FontSize(length))])
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

        "border-width" => expand_edges(
            value,
            [
                Property::BorderTopWidth,
                Property::BorderRightWidth,
                Property::BorderBottomWidth,
                Property::BorderLeftWidth,
            ],
        ),

        "border-top-width" => parse_edge_property(Property::BorderTopWidth, value),

        "border-right-width" => parse_edge_property(Property::BorderRightWidth, value),

        "border-bottom-width" => parse_edge_property(Property::BorderBottomWidth, value),

        "border-left-width" => parse_edge_property(Property::BorderLeftWidth, value),

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

        "box-sizing" => parse_box_sizing(value)
            .map(|box_sizing| vec![(Property::BoxSizing, SpecifiedValue::BoxSizing(box_sizing))])
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
        .filter(|length| !matches!(length, SpecifiedLength::Auto))
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
            return vec![
                (Property::FlexGrow, SpecifiedValue::Number(0.0)),
                (Property::FlexShrink, SpecifiedValue::Number(0.0)),
                (
                    Property::FlexBasis,
                    SpecifiedValue::SizeLength(SpecifiedLength::Auto),
                ),
            ];
        }

        "auto" => {
            return vec![
                (Property::FlexGrow, SpecifiedValue::Number(1.0)),
                (Property::FlexShrink, SpecifiedValue::Number(1.0)),
                (
                    Property::FlexBasis,
                    SpecifiedValue::SizeLength(SpecifiedLength::Auto),
                ),
            ];
        }

        "initial" => {
            return vec![
                (Property::FlexGrow, SpecifiedValue::Number(0.0)),
                (Property::FlexShrink, SpecifiedValue::Number(1.0)),
                (
                    Property::FlexBasis,
                    SpecifiedValue::SizeLength(SpecifiedLength::Auto),
                ),
            ];
        }

        _ => {}
    }

    let tokens: Vec<&str> = normalized.split_ascii_whitespace().collect();

    if tokens.is_empty() || tokens.len() > 3 {
        return Vec::new();
    }

    let Some(grow) = parse_non_negative_float(tokens[0]) else {
        return Vec::new();
    };

    let mut shrink = 1.0;
    let mut basis = SpecifiedLength::Px(0.0);

    match tokens.as_slice() {
        [_] => {}

        [_, second] => {
            if let Some(number) = parse_non_negative_float(second) {
                shrink = number;
            } else if let Some(length) = parse_specified_length(second, true) {
                basis = length;
            } else {
                return Vec::new();
            }
        }

        [_, second, third] => {
            let Some(number) = parse_non_negative_float(second) else {
                return Vec::new();
            };

            let Some(length) = parse_specified_length(third, true) else {
                return Vec::new();
            };

            shrink = number;
            basis = length;
        }

        _ => {}
    }

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
        .filter(|number| number.is_finite() && *number >= 0.0)
}

fn parse_margin_property(property: Property, value: &str) -> Vec<(Property, SpecifiedValue)> {
    parse_specified_length(value, true)
        .filter(|length| !matches!(length, SpecifiedLength::Percent(_)))
        .map(|length| vec![(property, SpecifiedValue::MarginLength(length))])
        .unwrap_or_default()
}

fn parse_edge_property(property: Property, value: &str) -> Vec<(Property, SpecifiedValue)> {
    parse_specified_length(value, false)
        .filter(|length| !matches!(length, SpecifiedLength::Auto | SpecifiedLength::Percent(_)))
        .map(|length| vec![(property, SpecifiedValue::EdgeLength(length))])
        .unwrap_or_default()
}

fn parse_size_property(property: Property, value: &str) -> Vec<(Property, SpecifiedValue)> {
    parse_specified_length(value, true)
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
        .map(|length| vec![(property, SpecifiedValue::SizeLength(length))])
        .unwrap_or_default()
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

fn parse_box_sizing(value: &str) -> Option<BoxSizing> {
    match value.trim().to_ascii_lowercase().as_str() {
        "content-box" => Some(BoxSizing::ContentBox),
        "border-box" => Some(BoxSizing::BorderBox),
        _ => None,
    }
}

fn parse_border_shorthand(value: &str) -> Vec<(Property, SpecifiedValue)> {
    let normalized = value.trim();

    if normalized.eq_ignore_ascii_case("none") {
        return vec![(
            Property::BorderStyle,
            SpecifiedValue::BorderStyle(BorderStyle::None),
        )];
    }

    let mut width = None;
    let mut border_style = None;
    let mut color = None;

    for token in normalized.split_ascii_whitespace() {
        if width.is_none()
            && let Some(length) = parse_specified_length(token, false)
            && !matches!(length, SpecifiedLength::Auto | SpecifiedLength::Percent(_))
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

    let mut declarations = Vec::new();

    if let Some(length) = width {
        for property in [
            Property::BorderTopWidth,
            Property::BorderRightWidth,
            Property::BorderBottomWidth,
            Property::BorderLeftWidth,
        ] {
            declarations.push((property, SpecifiedValue::EdgeLength(length)));
        }
    }

    if let Some(parsed_style) = border_style {
        declarations.push((
            Property::BorderStyle,
            SpecifiedValue::BorderStyle(parsed_style),
        ));
    }

    if let Some(parsed_color) = color {
        declarations.push((
            Property::BorderColor,
            SpecifiedValue::BorderColor(parsed_color),
        ));
    }

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

    if values
        .iter()
        .any(|value| matches!(value, SpecifiedLength::Auto | SpecifiedLength::Percent(_)))
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
        .map(|(property, length)| (property, SpecifiedValue::EdgeLength(length)))
        .collect()
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

fn parse_specified_length(value: &str, allow_auto_percent: bool) -> Option<SpecifiedLength> {
    let normalized = value.trim().to_ascii_lowercase();

    if allow_auto_percent && normalized == "auto" {
        return Some(SpecifiedLength::Auto);
    }

    if normalized == "0" {
        return Some(SpecifiedLength::Px(0.0));
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

    if allow_auto_percent && let Some(number) = normalized.strip_suffix('%') {
        return parse_number(number).map(SpecifiedLength::Percent);
    }

    None
}

fn resolve_font_size(length: SpecifiedLength, inherited_size: f32) -> Option<f32> {
    match length {
        SpecifiedLength::Px(value) => Some(value),
        SpecifiedLength::Em(value) => Some(value * inherited_size),
        SpecifiedLength::Rem(value) => Some(value * ROOT_FONT_SIZE_PX),
        SpecifiedLength::Percent(value) => Some(inherited_size * value / 100.0),
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
        SpecifiedLength::Em(value) => Some(value * font_size),
        SpecifiedLength::Rem(value) => Some(value * ROOT_FONT_SIZE_PX),
        SpecifiedLength::Auto | SpecifiedLength::Percent(_) => None,
    }
}

fn resolve_size_length(length: SpecifiedLength, font_size: f32) -> Length {
    match length {
        SpecifiedLength::Auto => Length::Auto,
        SpecifiedLength::Px(value) => Length::Px(value.max(0.0)),
        SpecifiedLength::Em(value) => Length::Px((value * font_size).max(0.0)),
        SpecifiedLength::Rem(value) => Length::Px((value * ROOT_FONT_SIZE_PX).max(0.0)),
        SpecifiedLength::Percent(value) => Length::Percent(value),
    }
}

fn parse_number(value: &str) -> Option<f32> {
    value
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|number| number.is_finite())
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
        "white" => return Some(Rgba::new(255, 255, 255, 255)),
        "red" => return Some(Rgba::new(255, 0, 0, 255)),
        "green" => return Some(Rgba::new(0, 128, 0, 255)),
        "blue" => return Some(Rgba::new(0, 0, 255, 255)),
        "gray" | "grey" => return Some(Rgba::new(128, 128, 128, 255)),
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

        let red = channels[0].parse::<u8>().ok()?;
        let green = channels[1].parse::<u8>().ok()?;
        let blue = channels[2].parse::<u8>().ok()?;
        let alpha = parse_alpha(channels[3])?;

        return Some(Rgba::new(red, green, blue, alpha));
    }

    None
}

fn parse_rgb_function(inner: &str, alpha: u8) -> Option<Rgba> {
    let channels: Vec<u8> = inner
        .split(',')
        .map(|channel| channel.trim().parse::<u8>().ok())
        .collect::<Option<Vec<_>>>()?;

    if let [red, green, blue] = channels.as_slice() {
        Some(Rgba::new(*red, *green, *blue, alpha))
    } else {
        None
    }
}

fn parse_alpha(value: &str) -> Option<u8> {
    let alpha = value.parse::<f32>().ok()?.clamp(0.0, 1.0);
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

        6 => {
            let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;

            Some(Rgba::new(red, green, blue, 255))
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
    color: Rgba,
    background_color: Option<Rgba>,
    font_size: u32,
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
    box_sizing: BoxSizing,
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
            color: style.color,
            background_color: style.background_color,
            font_size: style.font_size.to_bits(),
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
            box_sizing: style.box_sizing,
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
        AlignContent, AlignSelf, ComputedStyle, Display, FlexDirection, FlexWrap, Rgba, Stylesheet,
        compute_styles,
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
}
