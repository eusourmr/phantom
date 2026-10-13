//! 2O-11 — bounded CSS Grid container fallback, not native Grid layout.
use std::collections::BTreeMap;

use phantom_css::{Display, Stylesheet, compute_styles};
use phantom_dom::{Document, ElementData, NodeKind};

fn computed_display(style: &str) -> Result<Option<Display>, phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();
    let mut attrs = BTreeMap::new();
    attrs.insert("style".to_owned(), style.to_owned());
    let target = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("section", attrs)),
    )?;
    Ok(compute_styles(&document)
        .get(target)
        .map(|style| style.display()))
}

#[test]
fn grid_container_remains_visible_as_block_flow() -> Result<(), phantom_dom::DomError> {
    assert_eq!(computed_display("display: grid")?, Some(Display::Block));
    assert_eq!(
        computed_display("display: inline-grid")?,
        Some(Display::Block)
    );
    Ok(())
}

#[test]
fn grid_fallback_respects_later_cascade_overrides() -> Result<(), phantom_dom::DomError> {
    assert_eq!(
        computed_display("display: grid; display: none")?,
        Some(Display::None)
    );
    assert_eq!(
        computed_display("display: grid; display: flex")?,
        Some(Display::Flex)
    );
    assert_eq!(
        computed_display("display: grid !important; display: none")?,
        Some(Display::Block)
    );
    Ok(())
}

#[test]
fn grid_fallback_is_bounded_and_does_not_expand_parser_limits() {
    let css = "div { display: grid; }".repeat(4_400);
    let parsed = Stylesheet::parse(&css);
    let diagnostics = parsed.diagnostics();
    assert!(diagnostics.accepted_selector_rules <= 2_048);
    assert!(diagnostics.accepted_declarations <= 16_384);
}
