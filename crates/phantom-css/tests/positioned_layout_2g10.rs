//! Phantom 2G-10 CSS Positioned Layout I regression corpus.

use std::collections::BTreeMap;
use std::io;

use phantom_css::{Length, Position, compute_styles};
use phantom_dom::{Document, ElementData, NodeKind};

fn element_with_style(
    tag: &str,
    style: &str,
) -> Result<(Document, phantom_dom::NodeId), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();
    let mut attributes = BTreeMap::new();
    attributes.insert("style".to_owned(), style.to_owned());

    let node = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes(tag, attributes)),
    )?;

    Ok((document, node))
}

#[test]
fn parses_relative_position_and_physical_insets() -> Result<(), Box<dyn std::error::Error>> {
    let (document, node) = element_with_style(
        "div",
        "position:relative;top:-8px;right:5%;bottom:auto;left:2em;font-size:10px",
    )?;
    let styles = compute_styles(&document);
    let style = styles
        .get(node)
        .ok_or_else(|| io::Error::other("relative style missing"))?;

    assert_eq!(style.position(), Position::Relative);
    assert_eq!(style.top(), Length::Px(-8.0));
    assert_eq!(style.right(), Length::Percent(5.0));
    assert_eq!(style.bottom(), Length::Auto);
    assert_eq!(style.left(), Length::Px(20.0));
    Ok(())
}

#[test]
fn position_and_insets_are_not_inherited() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();

    let mut parent_attributes = BTreeMap::new();
    parent_attributes.insert(
        "style".to_owned(),
        "position:relative;left:40px;top:20px".to_owned(),
    );
    let parent = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", parent_attributes)),
    )?;

    let child = document.append_child(parent, NodeKind::Element(ElementData::new("div")))?;
    let styles = compute_styles(&document);
    let child_style = styles
        .get(child)
        .ok_or_else(|| io::Error::other("child style missing"))?;

    assert_eq!(child_style.position(), Position::Static);
    assert_eq!(child_style.left(), Length::Auto);
    assert_eq!(child_style.top(), Length::Auto);
    Ok(())
}

#[test]
fn css_wide_keywords_restore_position_initials() -> Result<(), Box<dyn std::error::Error>> {
    let (document, node) = element_with_style(
        "div",
        "position:relative;left:20px;position:initial;left:initial",
    )?;
    let styles = compute_styles(&document);
    let style = styles
        .get(node)
        .ok_or_else(|| io::Error::other("initial style missing"))?;

    assert_eq!(style.position(), Position::Static);
    assert_eq!(style.left(), Length::Auto);
    Ok(())
}
