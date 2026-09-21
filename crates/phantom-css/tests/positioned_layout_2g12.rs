//! Phantom 2G-12 CSS Positioned Layout III regression corpus.

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
fn absolute_auto_width_preserves_definite_physical_insets() -> Result<(), Box<dyn std::error::Error>>
{
    let (document, node) =
        element_with_style("div", "position:absolute;left:10%;right:20px;width:auto")?;
    let styles = compute_styles(&document);
    let style = styles
        .get(node)
        .ok_or_else(|| io::Error::other("absolute style missing"))?;

    assert_eq!(style.position(), Position::Absolute);
    assert_eq!(style.left(), Length::Percent(10.0));
    assert_eq!(style.right(), Length::Px(20.0));
    assert_eq!(style.width(), Length::Auto);
    Ok(())
}

#[test]
fn explicit_width_remains_distinct_from_auto_inset_solving()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, node) =
        element_with_style("div", "position:absolute;left:10px;right:20px;width:40px")?;
    let styles = compute_styles(&document);
    let style = styles
        .get(node)
        .ok_or_else(|| io::Error::other("absolute style missing"))?;

    assert_eq!(style.width(), Length::Px(40.0));
    Ok(())
}
