//! Site Identity I discovery tests for Phantom 2C-11.

use std::error::Error;

use phantom_engine::Engine;

#[test]
fn discovers_declared_png_site_icon() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<html><head><link rel="icon" type="image/png" href="/assets/icon.png"></head></html>"#,
    )?;

    let icon = engine.site_icon_request();
    assert_eq!(
        icon.as_ref().map(|request| request.source()),
        Some("/assets/icon.png"),
    );

    Ok(())
}

#[test]
fn rel_icon_token_is_case_insensitive_and_may_have_other_tokens() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<link rel="shortcut ICON" href="site.webp">"#)?;

    let icon = engine.site_icon_request();
    assert_eq!(
        icon.as_ref().map(|request| request.source()),
        Some("site.webp"),
    );

    Ok(())
}

#[test]
fn unsupported_declared_type_is_skipped_for_site_identity_i() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<link rel="icon" type="image/svg+xml" href="vector.svg"><link rel="icon" type="image/png" href="fallback.png">"#,
    )?;

    let icon = engine.site_icon_request();
    assert_eq!(
        icon.as_ref().map(|request| request.source()),
        Some("fallback.png"),
    );

    Ok(())
}

#[test]
fn does_not_synthesize_implicit_favicon() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html("<html><head><title>No icon</title></head><body></body></html>")?;

    assert!(engine.site_icon_request().is_none());

    Ok(())
}
