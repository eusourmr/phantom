//! Phantom 2G-7 CSS white-space regression corpus.

use std::collections::BTreeMap;
use std::io;

use phantom_css::{WhiteSpace, compute_styles};
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
fn parses_normal_and_pre_values() -> Result<(), Box<dyn std::error::Error>> {
    let (normal_document, normal_node) = element_with_style("div", "white-space: normal")?;
    let normal_styles = compute_styles(&normal_document);
    let normal = normal_styles
        .get(normal_node)
        .ok_or_else(|| io::Error::other("normal style missing"))?;

    assert_eq!(normal.white_space(), WhiteSpace::Normal);

    let (pre_document, pre_node) = element_with_style("div", "white-space: pre")?;
    let pre_styles = compute_styles(&pre_document);
    let pre = pre_styles
        .get(pre_node)
        .ok_or_else(|| io::Error::other("pre style missing"))?;

    assert_eq!(pre.white_space(), WhiteSpace::Pre);
    Ok(())
}

#[test]
fn white_space_is_inherited() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();

    let mut parent_attributes = BTreeMap::new();
    parent_attributes.insert("style".to_owned(), "white-space: pre".to_owned());

    let parent = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", parent_attributes)),
    )?;

    let child = document.append_child(parent, NodeKind::Element(ElementData::new("span")))?;

    let styles = compute_styles(&document);
    let child_style = styles
        .get(child)
        .ok_or_else(|| io::Error::other("child style missing"))?;

    assert_eq!(child_style.white_space(), WhiteSpace::Pre);
    Ok(())
}

#[test]
fn pre_ua_style_preserves_whitespace_but_author_normal_can_override()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();

    let pre = document.append_child(root, NodeKind::Element(ElementData::new("pre")))?;

    let mut override_attributes = BTreeMap::new();
    override_attributes.insert("style".to_owned(), "white-space: normal".to_owned());

    let overridden = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("pre", override_attributes)),
    )?;

    let styles = compute_styles(&document);

    assert_eq!(
        styles
            .get(pre)
            .ok_or_else(|| io::Error::other("pre style missing"))?
            .white_space(),
        WhiteSpace::Pre
    );

    assert_eq!(
        styles
            .get(overridden)
            .ok_or_else(|| io::Error::other("overridden pre style missing"))?
            .white_space(),
        WhiteSpace::Normal
    );

    Ok(())
}

#[test]
fn css_wide_initial_and_inherit_apply_to_white_space() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();

    let mut parent_attributes = BTreeMap::new();
    parent_attributes.insert("style".to_owned(), "white-space: pre".to_owned());
    let parent = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", parent_attributes)),
    )?;

    let mut inherited_attributes = BTreeMap::new();
    inherited_attributes.insert("style".to_owned(), "white-space: inherit".to_owned());
    let inherited = document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes("span", inherited_attributes)),
    )?;

    let mut initial_attributes = BTreeMap::new();
    initial_attributes.insert("style".to_owned(), "white-space: initial".to_owned());
    let initial = document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes("span", initial_attributes)),
    )?;

    let styles = compute_styles(&document);

    assert_eq!(
        styles
            .get(inherited)
            .ok_or_else(|| io::Error::other("inherit style missing"))?
            .white_space(),
        WhiteSpace::Pre
    );

    assert_eq!(
        styles
            .get(initial)
            .ok_or_else(|| io::Error::other("initial style missing"))?
            .white_space(),
        WhiteSpace::Normal
    );

    Ok(())
}
