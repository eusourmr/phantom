//! Phantom 2G-6 CSS line-height regression corpus.

use std::collections::BTreeMap;

use phantom_css::{LineHeight, compute_styles};
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
fn parses_normal_number_percentage_and_lengths() -> Result<(), phantom_dom::DomError> {
    let (normal_document, normal_node) = element_with_style("line-height: normal")?;
    let normal_styles = compute_styles(&normal_document);
    assert_eq!(
        normal_styles
            .get(normal_node)
            .map(|style| style.line_height()),
        Some(LineHeight::Normal)
    );

    let (number_document, number_node) = element_with_style("font-size: 20px; line-height: 1.5")?;
    let number_styles = compute_styles(&number_document);
    let number_style = number_styles.get(number_node).cloned().unwrap_or_default();
    assert_eq!(number_style.line_height(), LineHeight::Number(1.5));
    assert!((number_style.used_line_height(24.0) - 30.0).abs() < 0.01);

    let (percent_document, percent_node) =
        element_with_style("font-size: 20px; line-height: 150%")?;
    let percent_styles = compute_styles(&percent_document);
    let percent_style = percent_styles
        .get(percent_node)
        .cloned()
        .unwrap_or_default();
    assert_eq!(percent_style.line_height(), LineHeight::Px(30.0));

    let (em_document, em_node) = element_with_style("font-size: 20px; line-height: 1.5em")?;
    let em_styles = compute_styles(&em_document);
    assert_eq!(
        em_styles.get(em_node).map(|style| style.line_height()),
        Some(LineHeight::Px(30.0))
    );

    let (rem_document, rem_node) = element_with_style("line-height: 2rem")?;
    let rem_styles = compute_styles(&rem_document);
    assert_eq!(
        rem_styles.get(rem_node).map(|style| style.line_height()),
        Some(LineHeight::Px(32.0))
    );

    Ok(())
}

#[test]
fn unitless_line_height_is_inherited_before_child_font_resolution()
-> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let mut parent_attributes = BTreeMap::new();
    parent_attributes.insert("style".to_owned(), "line-height: 2".to_owned());
    let parent = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", parent_attributes)),
    )?;

    let mut child_attributes = BTreeMap::new();
    child_attributes.insert("style".to_owned(), "font-size: 10px".to_owned());
    let child = document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes("span", child_attributes)),
    )?;

    let styles = compute_styles(&document);
    let child_style = styles.get(child).cloned().unwrap_or_default();
    assert_eq!(child_style.line_height(), LineHeight::Number(2.0));
    assert!((child_style.used_line_height(12.5) - 20.0).abs() < 0.01);
    Ok(())
}

#[test]
fn css_wide_keywords_respect_line_height_inheritance() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let mut parent_attributes = BTreeMap::new();
    parent_attributes.insert("style".to_owned(), "line-height: 2".to_owned());
    let parent = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", parent_attributes)),
    )?;

    for (value, expected) in [
        ("inherit", LineHeight::Number(2.0)),
        ("unset", LineHeight::Number(2.0)),
        ("initial", LineHeight::Normal),
    ] {
        let mut attributes = BTreeMap::new();
        attributes.insert("style".to_owned(), format!("line-height: {value}"));
        let child = document.append_child(
            parent,
            NodeKind::Element(ElementData::with_attributes("span", attributes)),
        )?;

        let styles = compute_styles(&document);
        assert_eq!(
            styles.get(child).map(|style| style.line_height()),
            Some(expected)
        );
    }

    Ok(())
}

#[test]
fn negative_and_non_finite_line_heights_are_rejected() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("line-height: -2; line-height: NaN")?;
    let styles = compute_styles(&document);
    assert_eq!(
        styles.get(node).map(|style| style.line_height()),
        Some(LineHeight::Normal)
    );
    Ok(())
}
