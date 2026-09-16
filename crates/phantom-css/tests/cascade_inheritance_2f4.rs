//! Public cascade-and-inheritance corpus for Phantom cycle 2F-4.
//!
//! Scope: CSS-wide keywords, inherited vs non-inherited property behavior,
//! shorthand keyword expansion, cascade priority, and correct parent-relative
//! font-size resolution.
//!
//! The corpus builds DOM fixtures directly through `phantom-dom`; it does not
//! add a test-only dependency from `phantom-css` to the HTML parser.

use std::collections::BTreeMap;

use phantom_css::{ComputedStyle, Display, FontWeight, Length, Rgba, StyleMap, compute_styles};
use phantom_dom::{Document, ElementData, NodeId, NodeKind};

fn append_element(
    document: &mut Document,
    parent: NodeId,
    tag: &str,
    id: Option<&str>,
    class: Option<&str>,
    style: Option<&str>,
) -> Result<NodeId, phantom_dom::DomError> {
    let mut attributes = BTreeMap::new();

    if let Some(id) = id {
        attributes.insert("id".to_owned(), id.to_owned());
    }

    if let Some(class) = class {
        attributes.insert("class".to_owned(), class.to_owned());
    }

    if let Some(style) = style {
        attributes.insert("style".to_owned(), style.to_owned());
    }

    document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes(tag, attributes)),
    )
}

fn append_stylesheet(document: &mut Document, css: &str) -> Result<(), phantom_dom::DomError> {
    let root = document.root();
    let style = document.append_child(root, NodeKind::Element(ElementData::new("style")))?;
    document.append_child(style, NodeKind::Text(css.to_owned()))?;
    Ok(())
}

fn style_for(styles: &StyleMap, node: NodeId) -> ComputedStyle {
    styles.get(node).cloned().unwrap_or_default()
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn inherit_forces_non_inherited_width_from_parent() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let parent = append_element(
        &mut document,
        root,
        "div",
        Some("parent"),
        None,
        Some("width:120px"),
    )?;
    let child = append_element(
        &mut document,
        parent,
        "span",
        Some("child"),
        None,
        Some("width:inherit"),
    )?;

    let styles = compute_styles(&document);
    assert_eq!(style_for(&styles, child).width(), Length::Px(120.0));
    Ok(())
}

#[test]
fn initial_resets_ua_heading_defaults() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let heading = append_element(
        &mut document,
        root,
        "h1",
        Some("heading"),
        None,
        Some("display:initial;font-size:initial;font-weight:initial;margin:initial"),
    )?;

    let styles = compute_styles(&document);
    let heading = style_for(&styles, heading);

    assert_eq!(heading.display(), Display::Inline);
    assert_close(heading.font_size(), 16.0);
    assert_eq!(heading.font_weight(), FontWeight::Normal);
    assert_close(heading.margin().top(), 0.0);
    assert_close(heading.margin().right(), 0.0);
    assert_close(heading.margin().bottom(), 0.0);
    assert_close(heading.margin().left(), 0.0);
    Ok(())
}

#[test]
fn unset_inherits_inherited_properties_and_initializes_others() -> Result<(), phantom_dom::DomError>
{
    let mut document = Document::new();
    let root = document.root();

    let parent = append_element(
        &mut document,
        root,
        "div",
        Some("parent"),
        None,
        Some("color:rgb(10,20,30);width:120px"),
    )?;
    let child = append_element(
        &mut document,
        parent,
        "span",
        Some("child"),
        None,
        Some("color:unset;width:40px;width:unset;margin-left:12px;margin-left:unset"),
    )?;

    let styles = compute_styles(&document);
    let child = style_for(&styles, child);

    assert_eq!(child.color(), Rgba::new(10, 20, 30, 255));
    assert_eq!(child.width(), Length::Auto);
    assert_close(child.margin().left(), 0.0);
    Ok(())
}

#[test]
fn inherit_and_unset_font_size_override_heading_ua_default() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let parent = append_element(
        &mut document,
        root,
        "div",
        None,
        None,
        Some("font-size:20px"),
    )?;
    let inherited = append_element(
        &mut document,
        parent,
        "h1",
        Some("inherit"),
        None,
        Some("font-size:inherit"),
    )?;
    let unset = append_element(
        &mut document,
        parent,
        "h1",
        Some("unset"),
        None,
        Some("font-size:unset"),
    )?;

    let styles = compute_styles(&document);

    assert_close(style_for(&styles, inherited).font_size(), 20.0);
    assert_close(style_for(&styles, unset).font_size(), 20.0);
    Ok(())
}

#[test]
fn relative_font_size_uses_parent_computed_size_not_ua_default() -> Result<(), phantom_dom::DomError>
{
    let mut document = Document::new();
    let root = document.root();

    let parent = append_element(
        &mut document,
        root,
        "div",
        None,
        None,
        Some("font-size:20px"),
    )?;
    let heading = append_element(
        &mut document,
        parent,
        "h1",
        Some("heading"),
        None,
        Some("font-size:2em"),
    )?;

    let styles = compute_styles(&document);
    assert_close(style_for(&styles, heading).font_size(), 40.0);
    Ok(())
}

#[test]
fn initial_keyword_on_margin_shorthand_resets_all_edges() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let node = append_element(
        &mut document,
        root,
        "div",
        Some("box"),
        None,
        Some("margin:1px 2px 3px 4px;margin:initial"),
    )?;

    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_close(style.margin().top(), 0.0);
    assert_close(style.margin().right(), 0.0);
    assert_close(style.margin().bottom(), 0.0);
    assert_close(style.margin().left(), 0.0);
    assert_eq!(style.margin_auto().horizontal_count(), 0);
    assert_eq!(style.margin_auto().vertical_count(), 0);
    Ok(())
}

#[test]
fn flex_initial_resets_all_supported_flex_shorthand_components() -> Result<(), phantom_dom::DomError>
{
    let mut document = Document::new();
    let root = document.root();

    let node = append_element(
        &mut document,
        root,
        "div",
        Some("box"),
        None,
        Some("flex-grow:3;flex-shrink:4;flex-basis:20px;flex:initial"),
    )?;

    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_close(style.flex_grow(), 0.0);
    assert_close(style.flex_shrink(), 1.0);
    assert_eq!(style.flex_basis(), Length::Auto);
    Ok(())
}

#[test]
fn important_inline_css_wide_keywords_keep_normal_cascade_priority()
-> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_stylesheet(
        &mut document,
        ".target { color:red !important; width:50px !important; }",
    )?;

    let root = document.root();
    let parent = append_element(&mut document, root, "div", None, None, Some("color:blue"))?;
    let target = append_element(
        &mut document,
        parent,
        "span",
        Some("target"),
        Some("target"),
        Some("color:inherit !important;width:initial !important"),
    )?;

    let styles = compute_styles(&document);
    let target = style_for(&styles, target);

    assert_eq!(target.color(), Rgba::new(0, 0, 255, 255));
    assert_eq!(target.width(), Length::Auto);
    Ok(())
}

#[test]
fn normal_inline_inherit_does_not_beat_author_important() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_stylesheet(&mut document, ".target { color:red !important; }")?;

    let root = document.root();
    let parent = append_element(&mut document, root, "div", None, None, Some("color:blue"))?;
    let target = append_element(
        &mut document,
        parent,
        "span",
        Some("target"),
        Some("target"),
        Some("color:inherit"),
    )?;

    let styles = compute_styles(&document);

    assert_eq!(
        style_for(&styles, target).color(),
        Rgba::new(255, 0, 0, 255)
    );
    Ok(())
}

#[test]
fn explicit_inherit_works_for_normally_non_inherited_text_decoration()
-> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let parent = append_element(
        &mut document,
        root,
        "a",
        None,
        None,
        Some("text-decoration:underline"),
    )?;
    let child = append_element(
        &mut document,
        parent,
        "span",
        Some("child"),
        None,
        Some("text-decoration:inherit"),
    )?;

    let styles = compute_styles(&document);
    assert!(style_for(&styles, child).underline());
    Ok(())
}

#[test]
fn malformed_css_wide_value_does_not_replace_prior_valid_declaration()
-> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let node = append_element(
        &mut document,
        root,
        "div",
        Some("box"),
        None,
        Some("width:40px;width:inherit extra;color:green;color:unset extra"),
    )?;

    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_eq!(style.width(), Length::Px(40.0));
    assert_eq!(style.color(), Rgba::new(0, 128, 0, 255));
    Ok(())
}
