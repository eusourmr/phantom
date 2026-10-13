//! Phantom 2G-9 CSS white-space II regression corpus.

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
fn parses_white_space_ii_values() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        ("nowrap", WhiteSpace::NoWrap),
        ("pre-wrap", WhiteSpace::PreWrap),
        ("pre-line", WhiteSpace::PreLine),
    ];

    for (source, expected) in cases {
        let (document, node) = element_with_style("div", &format!("white-space: {source}"))?;
        let styles = compute_styles(&document);
        let style = styles
            .get(node)
            .ok_or_else(|| io::Error::other(format!("style missing for {source}")))?;

        assert_eq!(style.white_space(), expected);
    }

    Ok(())
}

#[test]
fn white_space_ii_values_are_inherited() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();

    let mut parent_attributes = BTreeMap::new();
    parent_attributes.insert("style".to_owned(), "white-space: pre-wrap".to_owned());
    let parent = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", parent_attributes)),
    )?;

    let child = document.append_child(parent, NodeKind::Element(ElementData::new("span")))?;
    let styles = compute_styles(&document);
    let child_style = styles
        .get(child)
        .ok_or_else(|| io::Error::other("inherited child style missing"))?;

    assert_eq!(child_style.white_space(), WhiteSpace::PreWrap);
    Ok(())
}

#[test]
fn inline_author_override_can_switch_between_white_space_modes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();

    let mut parent_attributes = BTreeMap::new();
    parent_attributes.insert("style".to_owned(), "white-space: nowrap".to_owned());
    let parent = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", parent_attributes)),
    )?;

    let mut child_attributes = BTreeMap::new();
    child_attributes.insert("style".to_owned(), "white-space: pre-line".to_owned());
    let child = document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes("span", child_attributes)),
    )?;

    let styles = compute_styles(&document);

    assert_eq!(
        styles
            .get(parent)
            .ok_or_else(|| io::Error::other("parent style missing"))?
            .white_space(),
        WhiteSpace::NoWrap
    );
    assert_eq!(
        styles
            .get(child)
            .ok_or_else(|| io::Error::other("child style missing"))?
            .white_space(),
        WhiteSpace::PreLine
    );

    Ok(())
}

#[test]
fn css_wide_initial_still_resolves_to_normal() -> Result<(), Box<dyn std::error::Error>> {
    let (document, node) = element_with_style("div", "white-space: initial")?;
    let styles = compute_styles(&document);
    let style = styles
        .get(node)
        .ok_or_else(|| io::Error::other("initial style missing"))?;

    assert_eq!(style.white_space(), WhiteSpace::Normal);
    Ok(())
}
