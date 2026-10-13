//! Phantom 2O-3: stylesheet installation and document-generation isolation.
//!
//! Regression baseline for pages that render without their external CSS.

use phantom_engine::{Engine, EngineError};
use std::error::Error;

#[test]
fn discovers_external_stylesheets_in_document_order() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<link rel="stylesheet" href="/base.css"><link rel="stylesheet" href="/theme.css"><p>content</p>"#)?;
    let sheets = engine.external_stylesheet_requests();
    assert_eq!(sheets.len(), 2);
    assert_eq!(sheets[0].source(), "/base.css");
    assert_eq!(sheets[1].source(), "/theme.css");
    assert_ne!(sheets[0].node_id(), sheets[1].node_id());
    Ok(())
}

#[test]
fn stylesheet_installation_preserves_document_and_readiness() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<link rel="stylesheet" href="/site.css"><p id="message">Hello</p>"#)?;
    let request = engine.external_stylesheet_requests();
    assert_eq!(request.len(), 1);
    let initial_nodes = engine.document().len();
    let initial_report = format!("{:?}", engine.html_build_report());
    engine.install_external_stylesheet(request[0].node_id(), "p { color: red; }")?;
    assert_eq!(engine.document().len(), initial_nodes);
    assert_eq!(format!("{:?}", engine.html_build_report()), initial_report);
    assert!(engine.fragment_target("message").is_some());
    Ok(())
}

#[test]
fn stylesheet_requests_are_replaced_on_new_navigation() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<link rel="stylesheet" href="/old.css"><p>old</p>"#)?;
    let old = engine.external_stylesheet_requests();
    assert_eq!(old.len(), 1);
    engine.install_external_stylesheet(old[0].node_id(), "p { color: red; }")?;
    engine.load_html("<p>new</p>")?;
    assert!(engine.external_stylesheet_requests().is_empty());
    Ok(())
}

#[test]
fn invalid_stylesheet_target_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html("<p>no stylesheet links</p>")?;
    assert!(matches!(
        engine.install_external_stylesheet(u64::MAX, "p { color: red; }"),
        Err(EngineError::ExternalStylesheetTargetInvalid(_))
    ));
    Ok(())
}

#[test]
fn oversized_external_css_is_rejected_without_discarding_requests() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<link rel="stylesheet" href="/site.css"><p>content</p>"#)?;
    let requests = engine.external_stylesheet_requests();
    let stylesheet = "x".repeat(1024 * 1024 + 1);
    assert!(matches!(
        engine.install_external_stylesheet(requests[0].node_id(), &stylesheet),
        Err(EngineError::ExternalStylesheetTooLarge { .. })
    ));
    assert_eq!(engine.external_stylesheet_requests(), requests);
    Ok(())
}
