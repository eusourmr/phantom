//! Phantom 2H-2 Box Visual Fidelity I CSS regression corpus.

use std::collections::BTreeMap;
use std::io;

use phantom_css::{Length, Overflow, compute_styles};
use phantom_dom::{Document, ElementData, NodeKind};

fn element_with_style(
    document: &mut Document,
    parent: phantom_dom::NodeId,
    style: &str,
) -> Result<phantom_dom::NodeId, phantom_dom::DomError> {
    let mut attributes = BTreeMap::new();
    attributes.insert("style".to_owned(), style.to_owned());
    document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes("div", attributes)),
    )
}

#[test]
fn computes_uniform_radius_overflow_and_bounded_opacity() -> Result<(), Box<dyn std::error::Error>>
{
    let mut document = Document::new();
    let root = document.root();
    let node = element_with_style(
        &mut document,
        root,
        "border-radius:25%;overflow:hidden;opacity:150%",
    )?;

    let styles = compute_styles(&document);
    let style = styles
        .get(node)
        .ok_or_else(|| io::Error::other("box visual style missing"))?;

    assert_eq!(style.border_radius(), Length::Percent(25.0));
    assert_eq!(style.overflow(), Overflow::Hidden);
    assert_eq!(style.opacity(), 1.0);
    Ok(())
}

#[test]
fn opacity_accepts_percent_and_clamps_negative_values() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let half = element_with_style(&mut document, root, "opacity:50%")?;
    let zero = element_with_style(&mut document, root, "opacity:-4")?;

    let styles = compute_styles(&document);
    assert_eq!(styles.get(half).map(|style| style.opacity()), Some(0.5));
    assert_eq!(styles.get(zero).map(|style| style.opacity()), Some(0.0));
    Ok(())
}

#[test]
fn visual_properties_participate_in_style_interning() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let first = element_with_style(
        &mut document,
        root,
        "border-radius:4px;overflow:visible;opacity:1",
    )?;
    let second = element_with_style(
        &mut document,
        root,
        "border-radius:12px;overflow:hidden;opacity:.5",
    )?;

    let styles = compute_styles(&document);
    let first = styles
        .get(first)
        .ok_or_else(|| io::Error::other("first visual style missing"))?;
    let second = styles
        .get(second)
        .ok_or_else(|| io::Error::other("second visual style missing"))?;

    assert_ne!(first, second);
    assert_eq!(first.border_radius(), Length::Px(4.0));
    assert_eq!(second.border_radius(), Length::Px(12.0));
    assert_eq!(first.overflow(), Overflow::Visible);
    assert_eq!(second.overflow(), Overflow::Hidden);
    assert_eq!(first.opacity(), 1.0);
    assert_eq!(second.opacity(), 0.5);
    Ok(())
}

#[test]
fn css_wide_keywords_use_non_inherited_visual_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let parent = element_with_style(
        &mut document,
        root,
        "border-radius:20px;overflow:hidden;opacity:.25",
    )?;
    let child = element_with_style(
        &mut document,
        parent,
        "border-radius:unset;overflow:unset;opacity:unset",
    )?;
    let inherited = element_with_style(
        &mut document,
        parent,
        "border-radius:inherit;overflow:inherit;opacity:inherit",
    )?;

    let styles = compute_styles(&document);
    let child = styles
        .get(child)
        .ok_or_else(|| io::Error::other("unset child style missing"))?;
    let inherited = styles
        .get(inherited)
        .ok_or_else(|| io::Error::other("inherited child style missing"))?;

    assert_eq!(child.border_radius(), Length::Px(0.0));
    assert_eq!(child.overflow(), Overflow::Visible);
    assert_eq!(child.opacity(), 1.0);

    assert_eq!(inherited.border_radius(), Length::Px(20.0));
    assert_eq!(inherited.overflow(), Overflow::Hidden);
    assert_eq!(inherited.opacity(), 0.25);
    Ok(())
}
