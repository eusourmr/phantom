//! Phantom 2G-15 CSS Stacking & Z-Index I regression corpus.

use std::collections::BTreeMap;
use std::io;

use phantom_css::{Position, ZIndex, compute_styles};
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
fn parses_auto_positive_zero_and_negative_z_index() -> Result<(), Box<dyn std::error::Error>> {
    for (source, expected) in [
        ("z-index:auto", ZIndex::Auto),
        ("z-index:0", ZIndex::Integer(0)),
        ("z-index:17", ZIndex::Integer(17)),
        ("z-index:-8", ZIndex::Integer(-8)),
    ] {
        let (document, node) = element_with_style("div", source)?;
        let styles = compute_styles(&document);
        let style = styles
            .get(node)
            .ok_or_else(|| io::Error::other("z-index style missing"))?;
        assert_eq!(style.z_index(), expected);
    }

    Ok(())
}

#[test]
fn z_index_is_not_inherited() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let mut attributes = BTreeMap::new();
    attributes.insert("style".to_owned(), "position:relative;z-index:9".to_owned());
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
    assert_eq!(child_style.z_index(), ZIndex::Auto);
    Ok(())
}

#[test]
fn css_wide_initial_and_inherit_apply_to_z_index() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();

    let mut parent_attributes = BTreeMap::new();
    parent_attributes.insert("style".to_owned(), "z-index:7".to_owned());
    let parent = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", parent_attributes)),
    )?;

    let mut inherited_attributes = BTreeMap::new();
    inherited_attributes.insert("style".to_owned(), "z-index:inherit".to_owned());
    let inherited = document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes("div", inherited_attributes)),
    )?;

    let mut initial_attributes = BTreeMap::new();
    initial_attributes.insert("style".to_owned(), "z-index:initial".to_owned());
    let initial = document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes("div", initial_attributes)),
    )?;

    let styles = compute_styles(&document);
    assert_eq!(
        styles
            .get(inherited)
            .ok_or_else(|| io::Error::other("inherited style missing"))?
            .z_index(),
        ZIndex::Integer(7)
    );
    assert_eq!(
        styles
            .get(initial)
            .ok_or_else(|| io::Error::other("initial style missing"))?
            .z_index(),
        ZIndex::Auto
    );
    Ok(())
}

#[test]
fn z_index_respects_existing_numeric_security_bound() -> Result<(), Box<dyn std::error::Error>> {
    let (document, node) = element_with_style("div", "z-index:1000001")?;
    let styles = compute_styles(&document);
    let style = styles
        .get(node)
        .ok_or_else(|| io::Error::other("bounded style missing"))?;

    assert_eq!(style.z_index(), ZIndex::Auto);
    Ok(())
}
