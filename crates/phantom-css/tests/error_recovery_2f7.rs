//! Public CSS recovery and compatibility corpus for Phantom cycle 2F-7.
//!
//! This corpus focuses on stylesheet-level recovery boundaries. Broken or
//! unsupported top-level input must not swallow a following valid rule, while
//! delimiters inside strings, functions, or attribute selectors must remain
//! part of that construct.

use std::collections::BTreeMap;

use phantom_css::{ComputedStyle, Rgba, StyleMap, compute_styles};
use phantom_dom::{Document, ElementData, NodeId, NodeKind};

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

fn append_stylesheet(document: &mut Document, css: &str) -> Result<(), phantom_dom::DomError> {
    let root = document.root();
    let style = document.append_child(root, NodeKind::Element(ElementData::new("style")))?;
    document.append_child(style, NodeKind::Text(css.to_owned()))?;
    Ok(())
}

fn style_for(styles: &StyleMap, node: NodeId) -> ComputedStyle {
    styles.get(node).cloned().unwrap_or_default()
}

fn document_with_target(
    css: &str,
    attributes: &[(&str, &str)],
) -> Result<(Document, NodeId), phantom_dom::DomError> {
    let mut document = Document::new();
    append_stylesheet(&mut document, css)?;
    let root = document.root();
    let target = append_element(&mut document, root, "div", attributes)?;
    Ok((document, target))
}

#[test]
fn semicolon_at_rule_does_not_swallow_following_rule() -> Result<(), phantom_dom::DomError> {
    let (document, target) = document_with_target(
        r#"@charset "UTF-8"; .target { color: red; }"#,
        &[("class", "target")],
    )?;

    let styles = compute_styles(&document);
    assert_eq!(
        style_for(&styles, target).color(),
        Rgba::new(255, 0, 0, 255)
    );
    Ok(())
}

#[test]
fn semicolon_inside_at_rule_function_is_not_a_recovery_boundary()
-> Result<(), phantom_dom::DomError> {
    let (document, target) = document_with_target(
        r#"@import url("theme;a.css"); .target { color: blue; }"#,
        &[("class", "target")],
    )?;

    let styles = compute_styles(&document);
    assert_eq!(
        style_for(&styles, target).color(),
        Rgba::new(0, 0, 255, 255)
    );
    Ok(())
}

#[test]
fn stray_top_level_declaration_recovers_before_following_rule() -> Result<(), phantom_dom::DomError>
{
    let (document, target) = document_with_target(
        "color: red; .target { color: green; }",
        &[("class", "target")],
    )?;

    let styles = compute_styles(&document);
    assert_eq!(
        style_for(&styles, target).color(),
        Rgba::new(0, 128, 0, 255)
    );
    Ok(())
}

#[test]
fn stray_closing_brace_recovers_before_following_rule() -> Result<(), phantom_dom::DomError> {
    let (document, target) =
        document_with_target("} .target { color: purple; }", &[("class", "target")])?;

    let styles = compute_styles(&document);
    assert_eq!(
        style_for(&styles, target).color(),
        Rgba::new(128, 0, 128, 255)
    );
    Ok(())
}

#[test]
fn quoted_semicolon_in_attribute_selector_stays_inside_selector()
-> Result<(), phantom_dom::DomError> {
    let (document, target) = document_with_target(
        r#"[data-note="a;b"] { color: teal; }"#,
        &[("data-note", "a;b")],
    )?;

    let styles = compute_styles(&document);
    assert_eq!(
        style_for(&styles, target).color(),
        Rgba::new(0, 128, 128, 255)
    );
    Ok(())
}

#[test]
fn unsupported_block_at_rule_is_isolated_from_neighboring_rule() -> Result<(), phantom_dom::DomError>
{
    let (document, target) = document_with_target(
        "@unknown screen { .target { color: red; } } .target { color: blue; }",
        &[("class", "target")],
    )?;

    let styles = compute_styles(&document);
    assert_eq!(
        style_for(&styles, target).color(),
        Rgba::new(0, 0, 255, 255)
    );
    Ok(())
}

#[test]
fn malformed_selector_rule_does_not_poison_following_rule() -> Result<(), phantom_dom::DomError> {
    let (document, target) = document_with_target(
        ".broken:unsupported { color: red; } .target { color: lime; }",
        &[("class", "target")],
    )?;

    let styles = compute_styles(&document);
    assert_eq!(
        style_for(&styles, target).color(),
        Rgba::new(0, 255, 0, 255)
    );
    Ok(())
}

#[test]
fn invalid_later_declarations_do_not_erase_prior_valid_value() -> Result<(), phantom_dom::DomError>
{
    let (document, target) = document_with_target(
        ".target { color: red; color rgb(1,2,3); color: definitely-not-a-color; }",
        &[("class", "target")],
    )?;

    let styles = compute_styles(&document);
    assert_eq!(
        style_for(&styles, target).color(),
        Rgba::new(255, 0, 0, 255)
    );
    Ok(())
}

#[test]
fn unterminated_trailing_rule_keeps_previously_accepted_rule() -> Result<(), phantom_dom::DomError>
{
    let (document, target) = document_with_target(
        ".target { color: green; } .broken { color: red;",
        &[("class", "target")],
    )?;

    let styles = compute_styles(&document);
    assert_eq!(
        style_for(&styles, target).color(),
        Rgba::new(0, 128, 0, 255)
    );
    Ok(())
}
