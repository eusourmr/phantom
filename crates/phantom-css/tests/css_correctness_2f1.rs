//! Public correctness corpus for Phantom cycle 2F-1.
//!
//! These tests intentionally exercise only the CSS behavior already claimed by
//! the public `phantom-css` API. They establish a deterministic gate before the
//! selector/value surface is expanded in later 2F milestones.

use std::collections::BTreeMap;

use phantom_css::{Display, FontWeight, Length, Rgba, Stylesheet, compute_styles};
use phantom_dom::{Document, ElementData, NodeId, NodeKind};

fn append_style(document: &mut Document, css: &str) -> Result<(), phantom_dom::DomError> {
    let root = document.root();
    let style = document.append_child(root, NodeKind::Element(ElementData::new("style")))?;
    document.append_child(style, NodeKind::Text(css.to_owned()))?;
    Ok(())
}

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

#[test]
fn higher_specificity_beats_later_lower_specificity() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(
        &mut document,
        "#target { color: red; } div { color: blue; }",
    )?;

    let root = document.root();
    let target = append_element(&mut document, root, "div", &[("id", "target")])?;
    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(target).map(|style| style.color()),
        Some(Rgba::new(255, 0, 0, 255))
    );
    Ok(())
}

#[test]
fn equal_specificity_uses_source_order() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(
        &mut document,
        ".target { color: red; } .target { color: blue; }",
    )?;

    let root = document.root();
    let target = append_element(&mut document, root, "div", &[("class", "target")])?;
    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(target).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );
    Ok(())
}

#[test]
fn later_declaration_wins_within_same_rule() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(&mut document, ".target { color: red; color: blue; }")?;

    let root = document.root();
    let target = append_element(&mut document, root, "div", &[("class", "target")])?;
    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(target).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );
    Ok(())
}

#[test]
fn invalid_later_value_does_not_erase_valid_winner() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(
        &mut document,
        ".target { color: red; color: definitely-not-a-color; }",
    )?;

    let root = document.root();
    let target = append_element(&mut document, root, "div", &[("class", "target")])?;
    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(target).map(|style| style.color()),
        Some(Rgba::new(255, 0, 0, 255))
    );
    Ok(())
}

#[test]
fn invalid_selector_and_declaration_do_not_poison_following_rule()
-> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(
        &mut document,
        ".broken:hover { color: red; } .target { broken; color: blue; }",
    )?;

    let root = document.root();
    let target = append_element(&mut document, root, "div", &[("class", "target")])?;
    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(target).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );
    Ok(())
}

#[test]
fn unsupported_at_rule_is_skipped_without_losing_following_rule()
-> Result<(), phantom_dom::DomError> {
    let stylesheet =
        Stylesheet::parse("@media screen { .inside { color: red; } } .target { color: blue; }");
    assert_eq!(stylesheet.len(), 1);

    let mut document = Document::new();
    append_style(
        &mut document,
        "@media screen { .inside { color: red; } } .target { color: blue; }",
    )?;

    let root = document.root();
    let target = append_element(&mut document, root, "div", &[("class", "target")])?;
    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(target).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );
    Ok(())
}

#[test]
fn author_important_beats_inline_normal() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(&mut document, "#target { color: red !important; }")?;

    let root = document.root();
    let target = append_element(
        &mut document,
        root,
        "div",
        &[("id", "target"), ("style", "color: blue")],
    )?;
    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(target).map(|style| style.color()),
        Some(Rgba::new(255, 0, 0, 255))
    );
    Ok(())
}

#[test]
fn inline_important_beats_author_important() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(&mut document, "#target { color: red !important; }")?;

    let root = document.root();
    let target = append_element(
        &mut document,
        root,
        "div",
        &[("id", "target"), ("style", "color: blue !important")],
    )?;
    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(target).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );
    Ok(())
}

#[test]
fn shorthand_and_longhand_follow_declaration_order() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let longhand_last = append_element(
        &mut document,
        root,
        "div",
        &[("style", "margin: 10px 20px; margin-left: 30px")],
    )?;
    let shorthand_last = append_element(
        &mut document,
        root,
        "div",
        &[("style", "margin-left: 30px; margin: 10px 20px")],
    )?;

    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(longhand_last).map(|style| style.margin().left()),
        Some(30.0)
    );
    assert_eq!(
        styles
            .get(shorthand_last)
            .map(|style| style.margin().left()),
        Some(20.0)
    );
    Ok(())
}

#[test]
fn supported_inherited_properties_inherit_while_box_properties_do_not()
-> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let parent = append_element(
        &mut document,
        root,
        "div",
        &[(
            "style",
            "color: red; font-size: 20px; font-weight: bold; \
             background-color: blue; width: 200px",
        )],
    )?;
    let child = append_element(&mut document, parent, "span", &[])?;

    let styles = compute_styles(&document);
    let child_style = styles.get(child).cloned().unwrap_or_default();

    assert_eq!(child_style.color(), Rgba::new(255, 0, 0, 255));
    assert_eq!(child_style.font_size(), 20.0);
    assert_eq!(child_style.font_weight(), FontWeight::Bold);
    assert_eq!(child_style.background_color(), None);
    assert_eq!(child_style.width(), Length::Auto);
    assert_eq!(child_style.display(), Display::Inline);
    Ok(())
}

#[test]
fn descendant_and_child_combinators_keep_distinct_meaning() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    append_style(
        &mut document,
        "main .target { color: red; } main > .target { color: blue; }",
    )?;

    let root = document.root();
    let main = append_element(&mut document, root, "main", &[])?;
    let direct = append_element(&mut document, main, "div", &[("class", "target")])?;
    let wrapper = append_element(&mut document, main, "section", &[])?;
    let nested = append_element(&mut document, wrapper, "div", &[("class", "target")])?;

    let styles = compute_styles(&document);

    assert_eq!(
        styles.get(direct).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );
    assert_eq!(
        styles.get(nested).map(|style| style.color()),
        Some(Rgba::new(255, 0, 0, 255))
    );
    Ok(())
}
