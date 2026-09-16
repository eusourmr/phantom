//! Public values-and-units corpus for Phantom cycle 2F-3.
//!
//! Scope: CSS absolute length normalization, existing relative-unit behavior,
//! unitless zero, percentage font sizes, property-aware negative-length
//! rejection, and preservation of prior valid declarations.

use std::collections::BTreeMap;

use phantom_css::{Length, compute_styles};
use phantom_dom::{Document, ElementData, NodeId, NodeKind};

fn element_with_style(style: &str) -> Result<(Document, NodeId), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();
    let mut attributes = BTreeMap::new();
    attributes.insert("style".to_owned(), style.to_owned());
    let node = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", attributes)),
    )?;
    Ok((document, node))
}

fn px(length: Length) -> f32 {
    let Length::Px(value) = length else {
        return f32::NAN;
    };

    value
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn css_absolute_units_normalize_to_css_pixels() -> Result<(), phantom_dom::DomError> {
    let cases = [
        ("1in", 96.0),
        ("2.54cm", 96.0),
        ("25.4mm", 96.0),
        ("101.6q", 96.0),
        ("72pt", 96.0),
        ("6pc", 96.0),
    ];

    for (value, expected) in cases {
        let (document, node) = element_with_style(&format!("width:{value}"))?;
        let styles = compute_styles(&document);
        let width = styles
            .get(node)
            .map(|style| style.width())
            .unwrap_or(Length::Auto);
        assert_close(px(width), expected);
    }

    Ok(())
}

#[test]
fn absolute_units_are_ascii_case_insensitive() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("width:1IN;height:72PT")?;
    let styles = compute_styles(&document);
    let style = styles.get(node).cloned().unwrap_or_default();

    assert_close(px(style.width()), 96.0);
    assert_close(px(style.height()), 96.0);
    Ok(())
}

#[test]
fn em_and_rem_keep_the_existing_bounded_resolution_contract() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("font-size:20px;width:2em;height:2rem")?;
    let styles = compute_styles(&document);
    let style = styles.get(node).cloned().unwrap_or_default();

    assert_close(style.font_size(), 20.0);
    assert_close(px(style.width()), 40.0);
    assert_close(px(style.height()), 32.0);
    Ok(())
}

#[test]
fn font_size_accepts_percentage_of_inherited_size() -> Result<(), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let mut parent_attributes = BTreeMap::new();
    parent_attributes.insert("style".to_owned(), "font-size:20px".to_owned());
    let parent = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", parent_attributes)),
    )?;

    let mut child_attributes = BTreeMap::new();
    child_attributes.insert("style".to_owned(), "font-size:125%".to_owned());
    let child = document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes("span", child_attributes)),
    )?;

    let styles = compute_styles(&document);
    assert_close(
        styles
            .get(child)
            .map(|style| style.font_size())
            .unwrap_or_default(),
        25.0,
    );
    Ok(())
}

#[test]
fn multiple_spellings_of_unitless_zero_are_lengths() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("width:0.0;height:-0;margin-left:+0")?;
    let styles = compute_styles(&document);
    let style = styles.get(node).cloned().unwrap_or_default();

    assert_eq!(style.width(), Length::Px(0.0));
    assert_eq!(style.height(), Length::Px(-0.0));
    assert_close(style.margin().left(), 0.0);
    Ok(())
}

#[test]
fn nonzero_unitless_length_remains_invalid() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("width:40px;width:12")?;
    let styles = compute_styles(&document);
    assert_eq!(
        styles.get(node).map(|style| style.width()),
        Some(Length::Px(40.0))
    );
    Ok(())
}

#[test]
fn negative_box_sizes_are_invalid_instead_of_being_clamped() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style(
        "width:40px;width:-5px;\
         height:30px;height:-2em;\
         flex-basis:20px;flex-basis:-1rem",
    )?;
    let styles = compute_styles(&document);
    let style = styles.get(node).cloned().unwrap_or_default();

    assert_eq!(style.width(), Length::Px(40.0));
    assert_eq!(style.height(), Length::Px(30.0));
    assert_eq!(style.flex_basis(), Length::Px(20.0));
    Ok(())
}

#[test]
fn negative_padding_border_and_gap_do_not_replace_valid_values() -> Result<(), phantom_dom::DomError>
{
    let (document, node) = element_with_style(
        "padding-left:8px;padding-left:-1px;\
         border-left-width:3px;border-left-width:-2px;\
         border-style:solid;\
         gap:12px;gap:-4px",
    )?;
    let styles = compute_styles(&document);
    let style = styles.get(node).cloned().unwrap_or_default();

    assert_close(style.padding().left(), 8.0);
    assert_close(style.border_width().left(), 3.0);
    assert_eq!(style.gap(), Length::Px(12.0));
    Ok(())
}

#[test]
fn negative_margin_lengths_remain_valid() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("margin-left:-1in;margin-top:-2em;font-size:10px")?;
    let styles = compute_styles(&document);
    let style = styles.get(node).cloned().unwrap_or_default();

    assert_close(style.margin().left(), -96.0);
    assert_close(style.margin().top(), -20.0);
    Ok(())
}

#[test]
fn negative_font_size_does_not_override_prior_valid_declaration()
-> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("font-size:20px;font-size:-150%")?;
    let styles = compute_styles(&document);

    assert_close(
        styles
            .get(node)
            .map(|style| style.font_size())
            .unwrap_or_default(),
        20.0,
    );
    Ok(())
}

#[test]
fn max_constraints_keep_none_as_unconstrained_and_accept_absolute_units()
-> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("min-width:1in;max-width:none;max-height:72pt")?;
    let styles = compute_styles(&document);
    let style = styles.get(node).cloned().unwrap_or_default();

    assert_eq!(style.min_width(), Length::Px(96.0));
    assert_eq!(style.max_width(), Length::Auto);
    assert_eq!(style.max_height(), Length::Px(96.0));
    Ok(())
}
