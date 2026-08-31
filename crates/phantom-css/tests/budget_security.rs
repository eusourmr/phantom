//! Security regression tests for the Phantom 2D-6 security gate.

use std::collections::BTreeMap;

use phantom_css::{Display, Rgba, Stylesheet, compute_styles};
use phantom_dom::{Document, ElementData, NodeKind};

#[test]
fn stylesheet_source_over_one_mib_is_rejected_without_partial_parse() {
    let source = "a{color:red}".repeat(90_000);
    let stylesheet = Stylesheet::parse(&source);
    assert!(stylesheet.is_empty());
}

#[test]
fn accepted_rule_count_is_bounded() {
    let mut source = String::new();
    for index in 0..1_100 {
        source.push_str(&format!(".c{index}{{color:red}}"));
    }
    let stylesheet = Stylesheet::parse(&source);
    assert!(stylesheet.len() <= 1_024);
}

#[test]
fn selector_part_count_is_bounded() {
    let selector = std::iter::repeat_n("div", 33).collect::<Vec<_>>().join(" ");
    let source = format!("{selector}{{color:red}}");
    assert!(Stylesheet::parse(&source).is_empty());
}

#[test]
fn classes_per_compound_selector_are_bounded() {
    let selector = (0..33)
        .map(|index| format!(".c{index}"))
        .collect::<String>();
    let source = format!("{selector}{{color:red}}");
    assert!(Stylesheet::parse(&source).is_empty());
}

#[test]
fn declarations_per_rule_are_bounded() -> Result<(), phantom_dom::DomError> {
    let mut css = String::from(".x{");
    css.push_str(&"color:red;".repeat(64));
    css.push_str("display:none}");

    let mut document = Document::new();
    let root = document.root();
    let style_node = document.append_child(root, NodeKind::Element(ElementData::new("style")))?;
    document.append_child(style_node, NodeKind::Text(css))?;

    let mut attributes = BTreeMap::new();
    attributes.insert("class".to_owned(), "x".to_owned());
    let target = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", attributes)),
    )?;

    let styles = compute_styles(&document);
    let computed = styles.get(target).cloned().unwrap_or_default();
    assert_eq!(computed.display(), Display::Block);
    assert_eq!(computed.color(), Rgba::new(255, 0, 0, 255));
    Ok(())
}

#[test]
fn oversized_inline_style_is_ignored() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();
    let mut attributes = BTreeMap::new();
    let mut inline = String::from("color:red;");
    inline.push_str(&" ".repeat(64 * 1024));
    attributes.insert("style".to_owned(), inline);

    let target = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", attributes)),
    )?;

    let styles = compute_styles(&document);
    let computed = styles.get(target).cloned().unwrap_or_default();
    assert_eq!(computed.color(), Rgba::new(0, 0, 0, 255));
    Ok(())
}
