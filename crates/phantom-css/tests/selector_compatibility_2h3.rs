//! Phantom 2H-3 CSS Compatibility I regression corpus.
//!
//! This slice expands selector compatibility without changing layout,
//! JavaScript, networking, or security budgets.

use phantom_css::{Rgba, Stylesheet, compute_styles};
use phantom_dom::{Document, ElementData, NodeId, NodeKind};
use std::collections::BTreeMap;

fn element(
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

fn install_style(
    document: &mut Document,
    parent: NodeId,
    css: &str,
) -> Result<(), phantom_dom::DomError> {
    let style = document.append_child(parent, NodeKind::Element(ElementData::new("style")))?;
    document.append_child(style, NodeKind::Text(css.to_owned()))?;
    Ok(())
}

#[test]
fn adjacent_sibling_combinator_ignores_non_element_siblings()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let html = element(&mut document, root, "html", &[])?;
    let head = element(&mut document, html, "head", &[])?;
    install_style(&mut document, head, ".lead + .adjacent { color: red; }")?;
    let body = element(&mut document, html, "body", &[])?;
    let parent = element(&mut document, body, "div", &[])?;
    let _lead = element(&mut document, parent, "span", &[("class", "lead")])?;
    document.append_child(parent, NodeKind::Text(" \n ".to_owned()))?;
    let adjacent = element(&mut document, parent, "p", &[("class", "adjacent later")])?;
    let _middle = element(&mut document, parent, "em", &[])?;
    let later = element(&mut document, parent, "p", &[("class", "adjacent")])?;

    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(adjacent).map(|style| style.color()),
        Some(Rgba::new(255, 0, 0, 255))
    );
    assert_eq!(
        styles.get(later).map(|style| style.color()),
        Some(Rgba::new(0, 0, 0, 255))
    );

    Ok(())
}

#[test]
fn root_empty_and_only_child_pseudos_match_structurally() -> Result<(), Box<dyn std::error::Error>>
{
    let mut document = Document::new();
    let root = document.root();
    let html = element(&mut document, root, "html", &[])?;
    let head = element(&mut document, html, "head", &[])?;
    install_style(
        &mut document,
        head,
        ":root { color: blue; } \
         .empty:empty { background-color: red; } \
         .only:only-child { color: green; }",
    )?;
    let body = element(&mut document, html, "body", &[])?;
    let empty = element(&mut document, body, "section", &[("class", "empty")])?;
    let non_empty = element(&mut document, body, "section", &[("class", "empty")])?;
    document.append_child(non_empty, NodeKind::Text(" ".to_owned()))?;
    let wrapper = element(&mut document, body, "div", &[])?;
    let only = element(&mut document, wrapper, "span", &[("class", "only")])?;

    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(html).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );
    assert_eq!(
        styles.get(empty).and_then(|style| style.background_color()),
        Some(Rgba::new(255, 0, 0, 255))
    );
    assert_eq!(
        styles
            .get(non_empty)
            .and_then(|style| style.background_color()),
        None
    );
    assert_eq!(
        styles.get(only).map(|style| style.color()),
        Some(Rgba::new(0, 128, 0, 255))
    );

    Ok(())
}

#[test]
fn of_type_pseudos_use_tag_local_positions() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let html = element(&mut document, root, "html", &[])?;
    let head = element(&mut document, html, "head", &[])?;
    install_style(
        &mut document,
        head,
        "span:first-of-type { color: red; } \
         span:last-of-type { background-color: blue; } \
         span:nth-of-type(2) { border: 1px solid green; } \
         em:only-of-type { color: blue; }",
    )?;
    let body = element(&mut document, html, "body", &[])?;
    let parent = element(&mut document, body, "div", &[])?;
    let first = element(&mut document, parent, "span", &[])?;
    let only_em = element(&mut document, parent, "em", &[])?;
    let second = element(&mut document, parent, "span", &[])?;
    let last = element(&mut document, parent, "span", &[])?;

    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(first).map(|style| style.color()),
        Some(Rgba::new(255, 0, 0, 255))
    );
    assert_eq!(
        styles.get(second).map(|style| style.border_color()),
        Some(Rgba::new(0, 128, 0, 255))
    );
    assert_eq!(
        styles.get(last).and_then(|style| style.background_color()),
        Some(Rgba::new(0, 0, 255, 255))
    );
    assert_eq!(
        styles.get(only_em).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );

    Ok(())
}

#[test]
fn malformed_adjacent_combinators_are_rejected_without_panicking() {
    assert!(Stylesheet::parse("div + + p { color: red; }").is_empty());
    assert!(Stylesheet::parse("+ p { color: red; }").is_empty());
}

#[test]
fn general_sibling_remains_outside_this_slice() {
    assert!(Stylesheet::parse("div ~ p { color: red; }").is_empty());
}
