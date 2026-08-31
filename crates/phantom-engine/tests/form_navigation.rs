//! Phantom 2C-13 Form Navigation I compatibility cases.

use std::collections::BTreeMap;
use std::error::Error;

use phantom_engine::{Engine, FormControlKind, FormSubmissionError};

#[test]
fn get_form_uses_current_values_hidden_fields_and_submitter() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"
        <form action="/search" method="get">
            <input name="q" value="initial">
            <input type="hidden" name="lang" value="pt-BR">
            <input name="ignored" value="x" disabled>
            <input type="submit" name="go" value="Search">
        </form>
        "#,
    )?;

    let controls = engine.form_control_regions();
    assert_eq!(controls.len(), 3);

    let text = controls
        .iter()
        .find(|control| control.kind() == FormControlKind::Text && control.name() == Some("q"))
        .ok_or("text input region missing")?;
    let submit = controls
        .iter()
        .find(|control| control.kind() == FormControlKind::Submit)
        .ok_or("submit region missing")?;

    assert!(text.rect().width() > 0.0);
    assert!(text.rect().height() > 0.0);
    assert_eq!(text.initial_value(), "initial");

    let mut values = BTreeMap::new();
    values.insert(text.id(), "phantom browser".to_owned());

    let submission = engine.build_get_form_submission(text.form(), Some(submit.id()), &values)?;

    assert_eq!(submission.action(), "/search");
    assert_eq!(
        submission.fields(),
        &[
            ("q".to_owned(), "phantom browser".to_owned()),
            ("lang".to_owned(), "pt-BR".to_owned()),
            ("go".to_owned(), "Search".to_owned()),
        ],
    );

    Ok(())
}

#[test]
fn submitter_name_is_not_sent_on_enter_submission() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<form><input name="q" value="x"><button name="go" value="yes">Go</button></form>"#,
    )?;

    let controls = engine.form_control_regions();
    let text = controls
        .iter()
        .find(|control| control.kind() == FormControlKind::Text)
        .ok_or("text input region missing")?;

    let submission = engine.build_get_form_submission(text.form(), None, &BTreeMap::new())?;

    assert_eq!(submission.fields(), &[("q".to_owned(), "x".to_owned())]);

    Ok(())
}

#[test]
fn post_form_is_explicitly_rejected() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<form method="POST"><input name="q"></form>"#)?;

    let control = engine
        .form_control_regions()
        .into_iter()
        .next()
        .ok_or("control region missing")?;

    let error = match engine.build_get_form_submission(control.form(), None, &BTreeMap::new()) {
        Ok(_) => {
            return Err(std::io::Error::other("POST form unexpectedly succeeded as GET").into());
        }
        Err(error) => error,
    };

    assert_eq!(
        error,
        FormSubmissionError::UnsupportedMethod("post".to_owned())
    );

    Ok(())
}

#[test]
fn unnamed_and_unsupported_controls_are_not_successful() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"
        <form>
            <input value="unnamed">
            <input type="password" name="secret" value="not-supported">
            <input type="text" name="ok" value="yes">
        </form>
        "#,
    )?;

    let control = engine
        .form_control_regions()
        .into_iter()
        .find(|control| control.name() == Some("ok"))
        .ok_or("supported control missing")?;

    let submission = engine.build_get_form_submission(control.form(), None, &BTreeMap::new())?;

    assert_eq!(submission.fields(), &[("ok".to_owned(), "yes".to_owned())]);

    Ok(())
}

#[test]
fn control_ids_are_document_generation_local_dom_handles() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html(r#"<form><input name="q"></form>"#)?;

    let control = engine
        .form_control_regions()
        .into_iter()
        .next()
        .ok_or("control missing")?;

    let raw: u64 = control.id().as_u64();
    assert!(raw > 0);

    let control_id = control.id();
    assert_eq!(control_id.as_u64(), raw);

    Ok(())
}
