//! Phantom 2G-8 attribute-selector operator regression corpus.

use std::collections::BTreeMap;

use phantom_css::{Rgba, compute_styles};
use phantom_dom::{Document, ElementData, NodeKind};

fn selected_color(
    selector: &str,
    attribute_name: &str,
    attribute_value: &str,
) -> Result<Rgba, phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();

    let style = document.append_child(root, NodeKind::Element(ElementData::new("style")))?;
    document.append_child(
        style,
        NodeKind::Text(format!("{selector} {{ color: red; }}")),
    )?;

    let mut attributes = BTreeMap::new();
    attributes.insert(attribute_name.to_owned(), attribute_value.to_owned());

    let target = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", attributes)),
    )?;

    Ok(compute_styles(&document)
        .get(target)
        .cloned()
        .unwrap_or_default()
        .color())
}

#[test]
fn includes_operator_matches_ascii_whitespace_separated_tokens() -> Result<(), phantom_dom::DomError>
{
    assert_eq!(
        selected_color(r#"div[data-tags~="beta"]"#, "data-tags", "alpha beta gamma")?,
        Rgba::new(255, 0, 0, 255)
    );
    assert_eq!(
        selected_color(r#"div[data-tags~="bet"]"#, "data-tags", "alpha beta gamma")?,
        Rgba::new(0, 0, 0, 255)
    );
    Ok(())
}

#[test]
fn dash_match_operator_supports_language_style_prefixes() -> Result<(), phantom_dom::DomError> {
    assert_eq!(
        selected_color(r#"div[lang|="pt"]"#, "lang", "pt-BR")?,
        Rgba::new(255, 0, 0, 255)
    );
    assert_eq!(
        selected_color(r#"div[lang|="pt"]"#, "lang", "en-US")?,
        Rgba::new(0, 0, 0, 255)
    );
    Ok(())
}

#[test]
fn prefix_suffix_and_substring_operators_match_expected_segments()
-> Result<(), phantom_dom::DomError> {
    assert_eq!(
        selected_color(
            r#"div[data-route^="/app"]"#,
            "data-route",
            "/app/profile/settings"
        )?,
        Rgba::new(255, 0, 0, 255)
    );
    assert_eq!(
        selected_color(
            r#"div[data-route$="settings"]"#,
            "data-route",
            "/app/profile/settings"
        )?,
        Rgba::new(255, 0, 0, 255)
    );
    assert_eq!(
        selected_color(
            r#"div[data-route*="profile"]"#,
            "data-route",
            "/app/profile/settings"
        )?,
        Rgba::new(255, 0, 0, 255)
    );
    Ok(())
}

#[test]
fn empty_substring_search_values_match_nothing() -> Result<(), phantom_dom::DomError> {
    assert_eq!(
        selected_color(r#"div[data-route*=""]"#, "data-route", "/app/profile")?,
        Rgba::new(0, 0, 0, 255)
    );
    assert_eq!(
        selected_color(r#"div[data-route^=""]"#, "data-route", "/app/profile")?,
        Rgba::new(0, 0, 0, 255)
    );
    assert_eq!(
        selected_color(r#"div[data-route$=""]"#, "data-route", "/app/profile")?,
        Rgba::new(0, 0, 0, 255)
    );
    Ok(())
}
