//! Security regression tests for the Phantom 2D-6 security gate.

use std::collections::BTreeMap;

use phantom_css::{Length, ObjectPosition, compute_styles};
use phantom_dom::{Document, ElementData, NodeKind};

fn element_with_style(
    style: &str,
) -> Result<(Document, phantom_dom::NodeId), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();
    let mut attributes = BTreeMap::new();
    attributes.insert("style".to_owned(), style.to_owned());
    let node = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", attributes)),
    )?;
    Ok((document, node))
}

#[test]
fn public_object_position_sanitizes_non_finite_inputs() {
    let position = ObjectPosition::new(f32::NAN, f32::INFINITY);
    assert_eq!(position.x(), 0.5);
    assert_eq!(position.y(), 0.5);
}

#[test]
fn non_finite_css_lengths_are_rejected() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("width: NaNpx; height: infpx")?;
    let styles = compute_styles(&document);
    let style = styles.get(node).cloned().unwrap_or_default();

    assert_eq!(style.width(), Length::Auto);
    assert_eq!(style.height(), Length::Auto);
    Ok(())
}

#[test]
fn numeric_magnitude_above_security_bound_is_rejected() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("width: 1000001px; flex-grow: 1000001")?;
    let styles = compute_styles(&document);
    let style = styles.get(node).cloned().unwrap_or_default();

    assert_eq!(style.width(), Length::Auto);
    assert_eq!(style.flex_grow(), 0.0);
    Ok(())
}

#[test]
fn computed_products_are_bounded_after_relative_unit_resolution()
-> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("font-size: 1000000px; width: 1000000em")?;
    let styles = compute_styles(&document);
    let style = styles.get(node).cloned().unwrap_or_default();

    assert_eq!(style.width(), Length::Px(1_000_000.0));
    Ok(())
}

#[test]
fn non_finite_alpha_is_rejected() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("color: rgba(1,2,3,NaN)")?;
    let styles = compute_styles(&document);
    let style = styles.get(node).cloned().unwrap_or_default();

    assert_eq!(style.color().red(), 0);
    assert_eq!(style.color().green(), 0);
    assert_eq!(style.color().blue(), 0);
    Ok(())
}
