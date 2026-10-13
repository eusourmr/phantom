//! Phantom 2H-4 Responsive / Viewport Compatibility regression corpus.
//!
//! This slice adds a bounded `@media` foundation and viewport-aware
//! recascade without changing CSS/DOM security budgets.

use std::collections::BTreeMap;

use phantom_css::{Display, Rgba, Stylesheet, compute_styles_with_viewport};
use phantom_dom::{Document, ElementData, NodeId, NodeKind};

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
fn width_queries_switch_cascade_at_viewport_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let html = element(&mut document, root, "html", &[])?;
    let head = element(&mut document, html, "head", &[])?;
    install_style(
        &mut document,
        head,
        "#target { color: red; } \
         @media screen and (max-width: 700px) { #target { color: blue; } } \
         @media (min-width: 701px) { #target { background-color: green; } }",
    )?;
    let body = element(&mut document, html, "body", &[])?;
    let target = element(&mut document, body, "div", &[("id", "target")])?;

    let wide = compute_styles_with_viewport(&document, 900.0, 600.0);
    assert_eq!(
        wide.get(target).map(|style| style.color()),
        Some(Rgba::new(255, 0, 0, 255))
    );
    assert_eq!(
        wide.get(target).and_then(|style| style.background_color()),
        Some(Rgba::new(0, 128, 0, 255))
    );

    let narrow = compute_styles_with_viewport(&document, 700.0, 600.0);
    assert_eq!(
        narrow.get(target).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );
    assert_eq!(
        narrow
            .get(target)
            .and_then(|style| style.background_color()),
        None
    );

    Ok(())
}

#[test]
fn height_orientation_and_comma_queries_are_supported() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let html = element(&mut document, root, "html", &[])?;
    let head = element(&mut document, html, "head", &[])?;
    install_style(
        &mut document,
        head,
        "#target { color: black; } \
         @media (orientation: portrait) and (min-height: 40em), print { \
             #target { color: blue; } \
         } \
         @media screen and (max-height: 30rem) { \
             #target { background-color: red; } \
         }",
    )?;
    let body = element(&mut document, html, "body", &[])?;
    let target = element(&mut document, body, "div", &[("id", "target")])?;

    let portrait = compute_styles_with_viewport(&document, 500.0, 700.0);
    assert_eq!(
        portrait.get(target).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );
    assert_eq!(
        portrait
            .get(target)
            .and_then(|style| style.background_color()),
        None
    );

    let short_landscape = compute_styles_with_viewport(&document, 900.0, 480.0);
    assert_eq!(
        short_landscape.get(target).map(|style| style.color()),
        Some(Rgba::new(0, 0, 0, 255))
    );
    assert_eq!(
        short_landscape
            .get(target)
            .and_then(|style| style.background_color()),
        Some(Rgba::new(255, 0, 0, 255))
    );

    Ok(())
}

#[test]
fn nested_active_media_rules_preserve_source_order() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let html = element(&mut document, root, "html", &[])?;
    let head = element(&mut document, html, "head", &[])?;
    install_style(
        &mut document,
        head,
        "#target { color: red; } \
         @media screen { \
             #target { color: green; } \
             @media (max-width: 600px) { #target { color: blue; } } \
         } \
         @media (min-width: 601px) { #target { color: black; } }",
    )?;
    let body = element(&mut document, html, "body", &[])?;
    let target = element(&mut document, body, "div", &[("id", "target")])?;

    let narrow = compute_styles_with_viewport(&document, 600.0, 800.0);
    assert_eq!(
        narrow.get(target).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );

    let wide = compute_styles_with_viewport(&document, 900.0, 800.0);
    assert_eq!(
        wide.get(target).map(|style| style.color()),
        Some(Rgba::new(0, 0, 0, 255))
    );

    Ok(())
}

#[test]
fn responsive_display_can_be_computed_without_dom_reparse() -> Result<(), Box<dyn std::error::Error>>
{
    let mut document = Document::new();
    let root = document.root();
    let html = element(&mut document, root, "html", &[])?;
    let head = element(&mut document, html, "head", &[])?;
    install_style(
        &mut document,
        head,
        "#desktop { display: block; } \
         #mobile { display: none; } \
         @media (max-width: 48em) { \
             #desktop { display: none; } \
             #mobile { display: block; } \
         }",
    )?;
    let body = element(&mut document, html, "body", &[])?;
    let desktop = element(&mut document, body, "div", &[("id", "desktop")])?;
    let mobile = element(&mut document, body, "div", &[("id", "mobile")])?;

    let wide = compute_styles_with_viewport(&document, 1000.0, 700.0);
    assert_eq!(
        wide.get(desktop).map(|style| style.display()),
        Some(Display::Block)
    );
    assert_eq!(
        wide.get(mobile).map(|style| style.display()),
        Some(Display::None)
    );

    let narrow = compute_styles_with_viewport(&document, 700.0, 700.0);
    assert_eq!(
        narrow.get(desktop).map(|style| style.display()),
        Some(Display::None)
    );
    assert_eq!(
        narrow.get(mobile).map(|style| style.display()),
        Some(Display::Block)
    );

    Ok(())
}

#[test]
fn unsupported_media_features_fail_closed() {
    let stylesheet = Stylesheet::parse_for_viewport(
        "@media (unknown-feature: 1) { div { color: red; } }",
        600.0,
        800.0,
    );
    assert!(stylesheet.is_empty());
}

#[test]
fn active_media_is_now_an_accepted_rule_while_print_stays_inactive() {
    let active =
        Stylesheet::parse("@media screen { .inside { color: red; } } .target { color: blue; }");
    assert_eq!(active.len(), 2);

    let inactive =
        Stylesheet::parse("@media print { .inside { color: red; } } .target { color: blue; }");
    assert_eq!(inactive.len(), 1);
}
