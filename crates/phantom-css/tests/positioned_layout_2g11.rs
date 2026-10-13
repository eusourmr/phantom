//! Phantom 2G-11 CSS Positioned Layout II regression corpus.

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
fn parses_absolute_position_with_existing_physical_insets() -> Result<(), Box<dyn std::error::Error>>
{
    let (document, node) = element_with_style(
        "div",
        "position:absolute;top:10%;right:12px;bottom:auto;left:-4px",
    )?;
    let styles = compute_styles(&document);
    let style = styles
        .get(node)
        .ok_or_else(|| io::Error::other("absolute style missing"))?;

    assert_eq!(style.position(), Position::Absolute);
    assert_eq!(style.top(), Length::Percent(10.0));
    assert_eq!(style.right(), Length::Px(12.0));
    assert_eq!(style.bottom(), Length::Auto);
    assert_eq!(style.left(), Length::Px(-4.0));
    Ok(())
}

#[test]
fn absolute_position_is_not_inherited() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let mut attributes = BTreeMap::new();
    attributes.insert("style".to_owned(), "position:absolute;left:20px".to_owned());
    let parent = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", attributes)),
    )?;
    let child = document.append_child(parent, NodeKind::Element(ElementData::new("div")))?;
    let styles = compute_styles(&document);
    let child_style = styles
        .get(child)
        .ok_or_else(|| io::Error::other("child style missing"))?;

    assert_eq!(child_style.position(), Position::Static);
    assert_eq!(child_style.left(), Length::Auto);
    Ok(())
}
