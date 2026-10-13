//! Phantom 2M-23 HTML element interaction / reflected-state regression corpus.

use std::collections::BTreeMap;

use phantom_js::{BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, Value};

fn element(
    node_id: u64,
    tag: &str,
    attributes: BTreeMap<String, String>,
    parent: Option<u64>,
) -> Result<BrowserDomElementSnapshot, Box<dyn std::error::Error>> {
    Ok(
        BrowserDomElementSnapshot::new(node_id, tag, attributes, "")?
            .with_parent_node_id(parent)
            .with_connected(true),
    )
}

fn attrs(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn interaction_runtime() -> Result<Runtime, Box<dyn std::error::Error>> {
    let snapshot = BrowserHostSnapshot::new(
        "https://html-elements.test/",
        "HTML element interaction I",
        800.0,
        600.0,
        1.0,
    )?
    .with_dom_elements(vec![
        element(1, "html", attrs(&[("id", "html")]), Some(99))?,
        element(2, "body", attrs(&[("id", "body")]), Some(1))?,
        element(3, "section", attrs(&[("id", "panel")]), Some(2))?,
        element(
            4,
            "input",
            attrs(&[("id", "field"), ("type", "text"), ("value", "alpha")]),
            Some(3),
        )?,
        element(5, "button", attrs(&[("id", "go")]), Some(3))?,
        element(6, "a", attrs(&[("id", "link"), ("href", "/old")]), Some(3))?,
        element(
            7,
            "form",
            attrs(&[("id", "form"), ("action", "/old"), ("method", "get")]),
            Some(3),
        )?,
        element(
            8,
            "label",
            attrs(&[("id", "label"), ("for", "field")]),
            Some(3),
        )?,
        element(
            9,
            "button",
            attrs(&[("id", "disabled"), ("disabled", "")]),
            Some(3),
        )?,
    ])?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn common_string_properties_round_trip_with_attributes() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = interaction_runtime()?;
    let value = runtime.execute(
        "var f = document.getElementById('field'); f.value = 'beta'; f.name = 'q'; f.placeholder = 'Search'; f.setAttribute('type', 'email'); var a = document.getElementById('link'); a.href = '/next'; a.target = '_blank'; a.rel = 'noopener'; f.value + '|' + f.getAttribute('value') + '|' + f.name + '|' + f.placeholder + '|' + f.type + '|' + a.href + '|' + a.target + '|' + a.rel;",
    )?;
    assert_eq!(
        value,
        Value::String("beta|beta|q|Search|email|/next|_blank|noopener".to_owned())
    );
    Ok(())
}

#[test]
fn form_label_and_camel_case_reflection_stays_live() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = interaction_runtime()?;
    let value = runtime.execute(
        "var form = document.getElementById('form'); var button = document.getElementById('go'); var label = document.getElementById('label'); form.action = '/submit'; form.method = 'post'; form.enctype = 'multipart/form-data'; button.formAction = '/override'; button.formMethod = 'post'; button.formEnctype = 'text/plain'; label.htmlFor = 'field'; form.setAttribute('method', 'dialog'); form.action + '|' + form.method + '|' + form.enctype + '|' + button.getAttribute('formaction') + '|' + button.formMethod + '|' + button.formEnctype + '|' + label.getAttribute('for') + '|' + label.htmlFor;",
    )?;
    assert_eq!(
        value,
        Value::String(
            "/submit|dialog|multipart/form-data|/override|post|text/plain|field|field".to_owned()
        )
    );
    Ok(())
}

#[test]
fn boolean_form_states_reflect_attribute_presence_both_directions()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = interaction_runtime()?;
    let value = runtime.execute(
        "var f = document.getElementById('field'); f.disabled = true; f.checked = true; f.required = true; f.readOnly = true; f.multiple = true; f.autoFocus = true; f.removeAttribute('checked'); f.setAttribute('selected', 'selected'); f.disabled + '|' + f.checked + '|' + f.required + '|' + f.readOnly + '|' + f.multiple + '|' + f.autoFocus + '|' + f.selected + '|' + f.hasAttribute('readonly');",
    )?;
    assert_eq!(
        value,
        Value::String("true|false|true|true|true|true|true|true".to_owned())
    );
    Ok(())
}

#[test]
fn synthetic_click_uses_event_target_propagation_and_can_mutate_dom()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = interaction_runtime()?;
    let value = runtime.execute(
        "var panel = document.getElementById('panel'); var button = document.getElementById('go'); var trace = ''; panel.addEventListener('click', function(e) { trace = trace + 'P'; }); button.addEventListener('click', function(e) { trace = trace + 'T'; if (this === button) { trace = trace + 'H'; } if (e.target === button) { trace = trace + 'E'; } if (!e.isTrusted) { trace = trace + 'S'; } button.dataset.state = 'clicked'; }); button.click(); trace + '|' + button.dataset.state;",
    )?;
    assert_eq!(value, Value::String("THESP|clicked".to_owned()));
    Ok(())
}

#[test]
fn disabled_form_control_suppresses_synthetic_click_until_reenabled()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = interaction_runtime()?;
    let value = runtime.execute(
        "var button = document.getElementById('disabled'); var count = 0; button.addEventListener('click', function() { count = count + 1; }); button.click(); button.disabled = false; button.click(); count + '|' + button.disabled + '|' + button.hasAttribute('disabled');",
    )?;
    assert_eq!(value, Value::String("1|false|false".to_owned()));
    Ok(())
}
