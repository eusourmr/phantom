//! Public Flexbox CSS-surface corpus for Phantom cycle 2F-6.
//!
//! Scope: the Engine-Beta flex subset only. The corpus validates parser and
//! computed-style behavior without adding dependencies to `phantom-css`.

use std::collections::BTreeMap;

use phantom_css::{AlignContent, ComputedStyle, JustifyContent, Length, StyleMap, compute_styles};
use phantom_dom::{Document, ElementData, NodeId, NodeKind};

fn append_element(
    document: &mut Document,
    parent: NodeId,
    tag: &str,
    style: &str,
) -> Result<NodeId, phantom_dom::DomError> {
    let mut attributes = BTreeMap::new();
    attributes.insert("style".to_owned(), style.to_owned());

    document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes(tag, attributes)),
    )
}

fn style_for(styles: &StyleMap, node: NodeId) -> ComputedStyle {
    styles.get(node).cloned().unwrap_or_default()
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.001,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn space_around_is_computed_for_flex_distribution() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let node = append_element(
        &mut document,
        root,
        "div",
        "display:flex;justify-content:space-around;align-content:space-around",
    )?;

    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_eq!(style.justify_content(), JustifyContent::SpaceAround);
    assert_eq!(style.align_content(), AlignContent::SpaceAround);
    Ok(())
}

#[test]
fn flex_basis_only_length_uses_shorthand_defaults() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let node = append_element(&mut document, root, "div", "flex:25%")?;

    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_close(style.flex_grow(), 1.0);
    assert_close(style.flex_shrink(), 1.0);
    assert_eq!(style.flex_basis(), Length::Percent(25.0));
    Ok(())
}

#[test]
fn flex_two_value_grow_basis_form_remains_supported() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let node = append_element(&mut document, root, "div", "flex:2 25%")?;

    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_close(style.flex_grow(), 2.0);
    assert_close(style.flex_shrink(), 1.0);
    assert_eq!(style.flex_basis(), Length::Percent(25.0));
    Ok(())
}

#[test]
fn unitless_zero_keeps_flex_factor_interpretation() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let node = append_element(&mut document, root, "div", "flex:0 2")?;

    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_close(style.flex_grow(), 0.0);
    assert_close(style.flex_shrink(), 2.0);
    assert_eq!(style.flex_basis(), Length::Px(0.0));
    Ok(())
}

#[test]
fn negative_flex_basis_in_shorthand_is_rejected() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let node = append_element(&mut document, root, "div", "flex:2 0 20px;flex:3 1 -10px")?;

    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_close(style.flex_grow(), 2.0);
    assert_close(style.flex_shrink(), 0.0);
    assert_eq!(style.flex_basis(), Length::Px(20.0));
    Ok(())
}
