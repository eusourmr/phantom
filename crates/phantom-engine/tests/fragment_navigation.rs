//! Phantom 2C-14 fragment-target compatibility cases.

use std::error::Error;

use phantom_engine::Engine;

#[test]
fn fragment_target_resolves_nested_content_position() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"
        <div style="height: 120px">Before</div>
        <section id="details"><span>Target text</span></section>
        "#,
    )?;

    let target = engine
        .fragment_target("details")
        .ok_or("fragment target missing")?;

    assert_eq!(target.id(), "details");
    assert!(target.top() >= 0.0);

    Ok(())
}

#[test]
fn unknown_fragment_does_not_invent_a_target() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<p id="known">Known</p>"#)?;

    assert!(engine.fragment_target("missing").is_none());

    Ok(())
}

#[test]
fn empty_fragment_is_document_top_not_an_element_target() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<p id="">Empty id</p>"#)?;

    assert!(engine.fragment_target("").is_none());

    Ok(())
}

#[test]
fn hidden_target_without_layout_is_not_reported_as_visible() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<div id="hidden" style="display:none">Hidden</div><p>Visible</p>"#)?;

    assert!(engine.fragment_target("hidden").is_none());

    Ok(())
}
