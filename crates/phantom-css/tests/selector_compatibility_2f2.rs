//! Public selector-compatibility corpus for Phantom cycle 2F-2.
//!
//! Scope: selector lists, attribute presence/equality selectors, structural
//! pseudo-classes, specificity, composition with existing combinators, and
//! bounded failure/recovery. Sibling combinators and `an+b` formulas remain
//! explicitly outside this slice.

use std::collections::BTreeMap;

use phantom_css::{Rgba, Stylesheet, compute_styles};
use phantom_dom::{Document, ElementData, NodeId, NodeKind};

fn append_style(document: &mut Document, css: &str) -> Result<(), phantom_dom::DomError> {
    let root = document.root();
    let style = document.append_child(root, NodeKind::Element(ElementData::new("style")))?;
    document.append_child(style, NodeKind::Text(css.to_owned()))?;
    Ok(())
}

fn append_element(
    document: &mut Document,
    parent: NodeId,
    tag: &str,
    attributes: &[(&str, &str)],
) -> Result<NodeId, phantom_dom::DomError> {
    let attributes = attributes
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect::<BTreeMap<_, _>>();

    document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes(tag, attributes)),
    )
}

fn color_of(document: &Document, node: NodeId) -> Rgba {
    compute_styles(document)
        .get(node)
        .map(|style| style.color())
        .unwrap_or(Rgba::new(0, 0, 0, 255))
}

#[test]
fn selector_list_applies_each_member_with_shared_source_order() -> Result<(), phantom_dom::DomError>
{
    let stylesheet = Stylesheet::parse("#a, .b { color: red; } .b { color: blue; }");
    assert_eq!(stylesheet.len(), 3);

    let mut document = Document::new();
    append_style(&mut document, "#a, .b { color: red; } .b { color: blue; }")?;

    let root = document.root();
    let a = append_element(&mut document, root, "div", &[("id", "a")])?;
    let b = append_element(&mut document, root, "div", &[("class", "b")])?;

    assert_eq!(color_of(&document, a), Rgba::new(255, 0, 0, 255));
    assert_eq!(color_of(&document, b), Rgba::new(0, 0, 255, 255));
    Ok(())
}

#[test]
fn selector_list_does_not_split_comma_inside_quoted_attribute_value()
-> Result<(), phantom_dom::DomError> {
    let stylesheet = Stylesheet::parse("[data-token=\"a,b\"], #other { color: red; }");
    assert_eq!(stylesheet.len(), 2);

    let mut document = Document::new();
    append_style(
        &mut document,
        "[data-token=\"a,b\"], #other { color: red; }",
    )?;

    let root = document.root();
    let token = append_element(&mut document, root, "div", &[("data-token", "a,b")])?;
    let other = append_element(&mut document, root, "div", &[("id", "other")])?;

    assert_eq!(color_of(&document, token), Rgba::new(255, 0, 0, 255));
    assert_eq!(color_of(&document, other), Rgba::new(255, 0, 0, 255));
    Ok(())
}

#[test]
fn attribute_presence_and_equality_selectors_match_exactly() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(
        &mut document,
        "[disabled] { color: red; } input[type=text] { color: blue; }",
    )?;

    let root = document.root();
    let disabled = append_element(&mut document, root, "button", &[("disabled", "")])?;
    let text = append_element(&mut document, root, "input", &[("type", "text")])?;
    let password = append_element(&mut document, root, "input", &[("type", "password")])?;

    assert_eq!(color_of(&document, disabled), Rgba::new(255, 0, 0, 255));
    assert_eq!(color_of(&document, text), Rgba::new(0, 0, 255, 255));
    assert_eq!(color_of(&document, password), Rgba::new(0, 0, 0, 255));
    Ok(())
}

#[test]
fn attribute_and_structural_pseudo_contribute_class_specificity()
-> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(
        &mut document,
        "li { color: red; } [data-state] { color: blue; } li:first-child { color: green; }",
    )?;

    let root = document.root();
    let list = append_element(&mut document, root, "ul", &[])?;
    let first = append_element(&mut document, list, "li", &[("data-state", "open")])?;
    let second = append_element(&mut document, list, "li", &[("data-state", "open")])?;

    assert_eq!(color_of(&document, first), Rgba::new(0, 128, 0, 255));
    assert_eq!(color_of(&document, second), Rgba::new(0, 0, 255, 255));
    Ok(())
}

#[test]
fn first_and_last_child_ignore_text_and_comment_siblings() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(
        &mut document,
        "li:first-child { color: red; } li:last-child { color: blue; }",
    )?;

    let root = document.root();
    let list = append_element(&mut document, root, "ul", &[])?;
    document.append_child(list, NodeKind::Text("before".to_owned()))?;
    let first = append_element(&mut document, list, "li", &[])?;
    document.append_child(list, NodeKind::Comment("middle".to_owned()))?;
    let last = append_element(&mut document, list, "li", &[])?;
    document.append_child(list, NodeKind::Text("after".to_owned()))?;

    assert_eq!(color_of(&document, first), Rgba::new(255, 0, 0, 255));
    assert_eq!(color_of(&document, last), Rgba::new(0, 0, 255, 255));
    Ok(())
}

#[test]
fn nth_child_integer_counts_only_element_siblings() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(&mut document, ".item:nth-child(2) { color: green; }")?;

    let root = document.root();
    let parent = append_element(&mut document, root, "div", &[])?;
    document.append_child(parent, NodeKind::Text("gap".to_owned()))?;
    let first = append_element(&mut document, parent, "span", &[("class", "item")])?;
    document.append_child(parent, NodeKind::Comment("gap".to_owned()))?;
    let second = append_element(&mut document, parent, "span", &[("class", "item")])?;
    let third = append_element(&mut document, parent, "span", &[("class", "item")])?;

    assert_eq!(color_of(&document, first), Rgba::new(0, 0, 0, 255));
    assert_eq!(color_of(&document, second), Rgba::new(0, 128, 0, 255));
    assert_eq!(color_of(&document, third), Rgba::new(0, 0, 0, 255));
    Ok(())
}

#[test]
fn structural_pseudo_composes_with_existing_child_and_descendant_combinators()
-> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(
        &mut document,
        "ul > li:first-child .target { color: red; } ul > li:last-child > .target { color: blue; }",
    )?;

    let root = document.root();
    let list = append_element(&mut document, root, "ul", &[])?;
    let first = append_element(&mut document, list, "li", &[])?;
    let first_wrapper = append_element(&mut document, first, "div", &[])?;
    let first_target =
        append_element(&mut document, first_wrapper, "span", &[("class", "target")])?;
    let last = append_element(&mut document, list, "li", &[])?;
    let last_target = append_element(&mut document, last, "span", &[("class", "target")])?;

    assert_eq!(color_of(&document, first_target), Rgba::new(255, 0, 0, 255));
    assert_eq!(color_of(&document, last_target), Rgba::new(0, 0, 255, 255));
    Ok(())
}

#[test]
fn compound_selector_combines_tag_id_class_attribute_and_pseudo()
-> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(
        &mut document,
        "li#target.note[data-state=open]:first-child { color: red; }",
    )?;

    let root = document.root();
    let list = append_element(&mut document, root, "ul", &[])?;
    let target = append_element(
        &mut document,
        list,
        "li",
        &[("id", "target"), ("class", "note"), ("data-state", "open")],
    )?;

    assert_eq!(color_of(&document, target), Rgba::new(255, 0, 0, 255));
    Ok(())
}

#[test]
fn invalid_selector_list_rejects_the_rule_but_neighboring_rule_survives()
-> Result<(), phantom_dom::DomError> {
    let stylesheet = Stylesheet::parse("#target, :hover { color: red; } #target { color: blue; }");
    assert_eq!(stylesheet.len(), 1);

    let mut document = Document::new();
    append_style(
        &mut document,
        "#target, :hover { color: red; } #target { color: blue; }",
    )?;

    let root = document.root();
    let target = append_element(&mut document, root, "div", &[("id", "target")])?;

    assert_eq!(color_of(&document, target), Rgba::new(0, 0, 255, 255));
    Ok(())
}

#[test]
fn unsupported_nth_child_formula_is_rejected_without_neighbor_damage()
-> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(
        &mut document,
        ".item:nth-child(2n+1) { color: red; } .item { color: blue; }",
    )?;

    let root = document.root();
    let item = append_element(&mut document, root, "div", &[("class", "item")])?;

    assert_eq!(color_of(&document, item), Rgba::new(0, 0, 255, 255));
    Ok(())
}

#[test]
fn attribute_selector_count_is_bounded() {
    let selector = "[data-x]".repeat(33);
    let source = format!("{selector} {{ color: red; }}");
    assert!(Stylesheet::parse(&source).is_empty());
}
