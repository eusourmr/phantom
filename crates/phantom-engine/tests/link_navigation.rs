//! Compatibility-oriented hyperlink interaction tests for Phantom 2C-12.
//!
//! These are project-owned cases shaped so they can later map cleanly to the
//! curated WPT harness. They do not copy upstream WPT test content.

use std::error::Error;

use phantom_engine::Engine;

#[test]
fn nested_text_anchor_exposes_clickable_region() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<html><body><p>Before <a href="/next"><span>Next page</span></a> after.</p></body></html>"#,
    )?;

    let links = engine.link_regions();
    assert!(!links.is_empty());
    assert!(links.iter().all(|link| link.href() == "/next"));

    let first = &links[0];
    let rect = first.rect();
    let hit = engine.link_at(
        rect.x() + rect.width() * 0.5,
        rect.y() + rect.height() * 0.5,
    );

    assert_eq!(hit.map(|link| link.href()), Some("/next"));

    Ok(())
}

#[test]
fn target_blank_requests_new_context() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<p><a href="child.html" target="_BLANK">Child</a></p>"#)?;

    let links = engine.link_regions();
    assert!(!links.is_empty());
    assert!(links.iter().all(|link| link.opens_new_context()));

    Ok(())
}

#[test]
fn anchor_without_href_is_not_interactive() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<p><a>Not a navigation link</a></p>"#)?;

    assert!(engine.link_regions().is_empty());

    Ok(())
}

#[test]
fn empty_href_remains_a_valid_navigation_reference() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<p><a href="">Reload current document reference</a></p>"#)?;

    let links = engine.link_regions();
    assert!(!links.is_empty());
    assert!(links.iter().all(|link| link.href().is_empty()));

    Ok(())
}

#[test]
fn relayout_rebuilds_link_geometry() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<p><a href="/responsive">A longer clickable hyperlink used for relayout.</a></p>"#,
    )?;

    let before = engine
        .link_regions()
        .first()
        .map(|link| link.rect())
        .ok_or("expected a link before relayout")?;

    engine.relayout(320.0)?;

    let after = engine
        .link_regions()
        .first()
        .map(|link| link.rect())
        .ok_or("expected a link after relayout")?;

    assert_eq!(engine.link_regions()[0].href(), "/responsive");
    assert!(before.width() > 0.0);
    assert!(after.width() > 0.0);

    Ok(())
}
