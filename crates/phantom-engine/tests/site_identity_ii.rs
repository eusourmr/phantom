//! Phantom 2C-15 Site Identity II compatibility cases.

use std::error::Error;

use phantom_engine::Engine;

#[test]
fn site_icon_candidates_preserve_document_order() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"
        <head>
            <link rel="icon" href="/first.png" type="image/png">
            <link rel="shortcut ICON" href="/second.ico" type="image/x-icon">
            <link rel="icon" href="/third.webp">
        </head>
        "#,
    )?;

    let candidates = engine.site_icon_requests();
    let sources = candidates
        .iter()
        .map(|candidate| candidate.source())
        .collect::<Vec<_>>();

    assert_eq!(sources, vec!["/first.png", "/second.ico", "/third.webp"]);
    assert_eq!(
        engine
            .site_icon_request()
            .map(|candidate| candidate.source().to_owned()),
        Some("/first.png".to_owned())
    );

    Ok(())
}

#[test]
fn typed_svg_is_skipped_but_later_raster_candidate_survives() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"
        <head>
            <link rel="icon" href="/vector.svg" type="image/svg+xml">
            <link rel="icon" href="/fallback.ico" type="image/vnd.microsoft.icon">
        </head>
        "#,
    )?;

    let candidates = engine.site_icon_requests();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].source(), "/fallback.ico");

    Ok(())
}

#[test]
fn untyped_candidate_is_kept_for_decoder_authority() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"
        <head>
            <link rel="icon" href="/unknown-extension.asset">
            <link rel="icon" href="/fallback.png" type="image/png">
        </head>
        "#,
    )?;

    let candidates = engine.site_icon_requests();

    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].source(), "/unknown-extension.asset");
    assert_eq!(candidates[1].source(), "/fallback.png");

    Ok(())
}

#[test]
fn document_title_is_normalized_for_browser_chrome() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        "<html><head><title>  Phantom\n   Search   Results </title></head><body></body></html>",
    )?;

    assert_eq!(
        engine.document_title().as_deref(),
        Some("Phantom Search Results")
    );

    Ok(())
}

#[test]
fn empty_document_title_falls_back_at_browser_layer() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html("<html><head><title>   </title></head><body></body></html>")?;

    assert!(engine.document_title().is_none());

    Ok(())
}
