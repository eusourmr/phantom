//! Phantom 2H-5 Image/Text Composition I CSS regression corpus.

use std::collections::BTreeMap;

use phantom_css::{ObjectPosition, compute_styles};
use phantom_dom::{Document, ElementData, NodeId, NodeKind};

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

fn position_for(style: &str) -> Result<ObjectPosition, phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();
    let image = append_element(
        &mut document,
        root,
        "img",
        &[("src", "image.png"), ("style", style)],
    )?;

    let styles = compute_styles(&document);
    Ok(styles
        .get(image)
        .map_or_else(ObjectPosition::default, |style| style.object_position()))
}

#[test]
fn four_value_edge_offsets_are_normalized() -> Result<(), phantom_dom::DomError> {
    let position = position_for("object-position: right 10% bottom 20%")?;

    assert!((position.x() - 0.9).abs() < f32::EPSILON);
    assert!((position.y() - 0.8).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn reversed_axis_edge_offsets_are_supported() -> Result<(), phantom_dom::DomError> {
    let position = position_for("object-position: top 25% left 10%")?;

    assert!((position.x() - 0.1).abs() < f32::EPSILON);
    assert!((position.y() - 0.25).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn three_value_edge_offset_keeps_other_axis_edge() -> Result<(), phantom_dom::DomError> {
    let position = position_for("object-position: right 15% top")?;

    assert!((position.x() - 0.85).abs() < f32::EPSILON);
    assert!((position.y() - 0.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn existing_one_and_two_value_forms_remain_compatible() -> Result<(), phantom_dom::DomError> {
    let centered = position_for("object-position: center")?;
    assert!((centered.x() - 0.5).abs() < f32::EPSILON);
    assert!((centered.y() - 0.5).abs() < f32::EPSILON);

    let swapped = position_for("object-position: top right")?;
    assert!((swapped.x() - 1.0).abs() < f32::EPSILON);
    assert!((swapped.y() - 0.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn malformed_advanced_position_does_not_replace_prior_valid_value()
-> Result<(), phantom_dom::DomError> {
    let position = position_for("object-position:left top; object-position:left 10% right 20%")?;

    assert!((position.x() - 0.0).abs() < f32::EPSILON);
    assert!((position.y() - 0.0).abs() < f32::EPSILON);
    Ok(())
}
