//! Phantom 2M-24 native interaction -> JavaScript event regression corpus.

use std::collections::BTreeMap;

use phantom_js::{
    BrowserDomElementSnapshot, BrowserDomMutation, BrowserHostSnapshot, Runtime, Value,
};

fn element(
    node_id: u64,
    tag: &str,
    attributes: BTreeMap<String, String>,
    parent: Option<u64>,
) -> Result<BrowserDomElementSnapshot, Box<dyn std::error::Error>> {
    Ok(BrowserDomElementSnapshot::new(node_id, tag, attributes, "")?
        .with_parent_node_id(parent)
        .with_connected(true))
}

fn attrs(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn interaction_runtime() -> Result<Runtime, Box<dyn std::error::Error>> {
    let snapshot = BrowserHostSnapshot::new(
        "https://native-events.test/",
        "Native interaction events I",
        800.0,
        600.0,
        1.0,
    )?
    .with_dom_elements(vec![
        element(1, "html", attrs(&[("id", "html")]), None)?,
        element(2, "body", attrs(&[("id", "body")]), Some(1))?,
        element(3, "section", attrs(&[("id", "panel")]), Some(2))?,
        element(4, "button", attrs(&[("id", "go")]), Some(3))?,
        element(
            5,
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
fn trusted_click_uses_capture_target_bubble_and_exposes_is_trusted()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = interaction_runtime()?;
    runtime.execute(
        "var trace = ''; \
         var panel = document.getElementById('panel'); \
         var button = document.getElementById('go'); \
         panel.addEventListener('click', function(event) { trace = trace + 'C' + event.isTrusted; }, true); \
         button.addEventListener('click', function(event) { trace = trace + '|T' + event.isTrusted; }); \
         panel.addEventListener('click', function(event) { trace = trace + '|B' + event.isTrusted; });",
    )?;

    let default_allowed = runtime.dispatch_trusted_click(4)?;

    assert!(default_allowed);
    assert_eq!(
        runtime.global("trace"),
        Some(&Value::String("Ctrue|Ttrue|Btrue".to_owned()))
    );
    Ok(())
}

#[test]
fn prevent_default_blocks_native_default_action_and_preserves_listener_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = interaction_runtime()?;
    runtime.execute(
        "var seen = false; \
         var button = document.getElementById('go'); \
         button.addEventListener('click', function(event) { seen = event.isTrusted; button.dataset.state = 'blocked'; event.preventDefault(); });",
    )?;

    let default_allowed = runtime.dispatch_trusted_click(4)?;
    let mutations = runtime.drain_dom_mutations();

    assert!(!default_allowed);
    assert_eq!(runtime.global("seen"), Some(&Value::Boolean(true)));
    assert!(mutations.iter().any(|mutation| {
        matches!(
            mutation,
            BrowserDomMutation::SetAttribute { node_id, name, value }
                if *node_id == 4 && name == "data-state" && value == "blocked"
        )
    }));
    Ok(())
}

#[test]
fn trusted_click_finishes_with_a_microtask_checkpoint()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = interaction_runtime()?;
    runtime.execute(
        "var trace = 'sync'; \
         var button = document.getElementById('go'); \
         button.addEventListener('click', function(event) { queueMicrotask(function() { if (event.isTrusted) { trace = 'trusted-microtask'; } else { trace = 'wrong'; } }); });",
    )?;

    let default_allowed = runtime.dispatch_trusted_click(4)?;

    assert!(default_allowed);
    assert_eq!(
        runtime.global("trace"),
        Some(&Value::String("trusted-microtask".to_owned()))
    );
    assert_eq!(runtime.pending_microtask_count(), 0);
    Ok(())
}


#[test]
fn script_dispatch_event_cannot_forge_is_trusted()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = interaction_runtime()?;
    let value = runtime.execute(
        "var button = document.getElementById('go'); \
         var seen = true; \
         var event = { type: 'click', bubbles: true, cancelable: true, isTrusted: true }; \
         button.addEventListener('click', function(current) { seen = current.isTrusted; }); \
         button.dispatchEvent(event); \
         seen + '|' + event.isTrusted;",
    )?;

    assert_eq!(value, Value::String("false|false".to_owned()));
    Ok(())
}

#[test]
fn disabled_form_control_suppresses_host_originated_click()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = interaction_runtime()?;
    runtime.execute(
        "var count = 0; document.getElementById('disabled').addEventListener('click', function() { count = count + 1; });",
    )?;

    let default_allowed = runtime.dispatch_trusted_click(5)?;

    assert!(!default_allowed);
    assert_eq!(runtime.global("count"), Some(&Value::Number(0.0)));
    Ok(())
}
