//! Phantom 2O-4 read-only visual rendering diagnostics regressions.

use phantom_engine::Engine;
use std::error::Error;

#[test]
fn new_engine_has_no_document_resources() {
    let engine = Engine::new();
    let report = engine.render_diagnostics();
    assert_eq!(report.external_css_discovered, 0);
    assert_eq!(report.external_css_installed, 0);
    assert_eq!(report.paint_commands, 0);
}

#[test]
fn stylesheet_installation_changes_only_installation_counter() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<link rel="stylesheet" href="/one.css"><link rel="stylesheet" href="/two.css"><p>Hello</p>"#,
    )?;
    let requests = engine.external_stylesheet_requests();
    let before = engine.render_diagnostics();
    assert_eq!(before.external_css_discovered, 2);
    assert_eq!(before.external_css_installed, 0);
    assert!(before.dom_nodes > 0);
    assert!(before.layout_boxes > 0);

    engine.install_external_stylesheet(requests[0].node_id(), "p { color: red; }")?;
    let after = engine.render_diagnostics();
    assert_eq!(after.external_css_discovered, 2);
    assert_eq!(after.external_css_installed, 1);
    assert_eq!(after.dom_nodes, before.dom_nodes);
    Ok(())
}

#[test]
fn rejected_stylesheet_does_not_change_installed_count() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<link rel="stylesheet" href="/site.css"><p>Hello</p>"#)?;
    let request = engine.external_stylesheet_requests();
    let before = engine.render_diagnostics();
    let invalid = "x".repeat(1024 * 1024 + 1);
    assert!(
        engine
            .install_external_stylesheet(request[0].node_id(), &invalid)
            .is_err()
    );
    assert_eq!(engine.render_diagnostics(), before);
    Ok(())
}

#[test]
fn navigation_resets_stylesheet_diagnostics() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<link rel="stylesheet" href="/old.css"><p>Old</p>"#)?;
    let request = engine.external_stylesheet_requests();
    engine.install_external_stylesheet(request[0].node_id(), "p { color: blue; }")?;
    engine.load_html("<p>New page</p>")?;
    let report = engine.render_diagnostics();
    assert_eq!(report.external_css_discovered, 0);
    assert_eq!(report.external_css_installed, 0);
    assert!(report.dom_nodes > 0);
    Ok(())
}
