//! Phantom 2M-8 DOM EventTarget I regression corpus.

use std::collections::BTreeMap;

use phantom_js::{BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value};

fn runtime_with_target() -> Result<Runtime, Box<dyn std::error::Error>> {
    let target = BrowserDomElementSnapshot::new(
        1,
        "div",
        BTreeMap::from([("id".to_owned(), "target".to_owned())]),
        "target",
    )?;
    let snapshot = BrowserHostSnapshot::new(
        "https://events.test/",
        "Events",
        800.0,
        600.0,
        1.0,
    )?
    .with_dom_elements(vec![target])?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn event_target_dispatch_orders_capture_then_bubble_and_binds_context()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target()?;
    let value = runtime.execute(
        "var node = document.getElementById('target'); \
         var log = ''; \
         var first = function(event) { \
             if (this === node && event.target === node && event.currentTarget === node && event.type === 'ready') { log = log + 'A'; } \
             else { log = log + 'X'; } \
         }; \
         var capture = function(event) { log = log + 'C'; }; \
         var removed = function(event) { log = log + 'R'; }; \
         var last = function(event) { log = log + 'B'; }; \
         node.addEventListener('ready', first); \
         node.addEventListener('ready', capture, true); \
         node.addEventListener('ready', first); \
         node.addEventListener('ready', removed); \
         node.removeEventListener('ready', removed); \
         node.addEventListener('ready', last); \
         var event = { type: 'ready' }; \
         var dispatched = node.dispatchEvent(event); \
         log + '|' + dispatched + '|' + (event.currentTarget === null);",
    )?;

    assert_eq!(value, Value::String("CAB|true|true".to_owned()));
    Ok(())
}

#[test]
fn event_listener_survives_successive_execute_calls()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target()?;
    runtime.execute(
        "var node = document.getElementById('target'); \
         var count = ''; \
         var listener = function(event) { count = count + 'x'; }; \
         node.addEventListener('later', listener);",
    )?;
    let value = runtime.execute(
        "var node = document.getElementById('target'); \
         node.dispatchEvent({ type: 'later' }); count;",
    )?;

    assert_eq!(value, Value::String("x".to_owned()));
    Ok(())
}

#[test]
fn add_event_listener_rejects_non_callable_listener() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target()?;
    let error = runtime
        .execute("var node = document.getElementById('target'); node.addEventListener('ready', 7);")
        .err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(RuntimeErrorKind::InvalidEventCallback)
    );
    Ok(())
}
