//! Phantom 2M-11 DOM Event Listener Semantics IV final regression corpus.

use std::collections::BTreeMap;

use phantom_js::{BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, Value};

fn runtime_with_target() -> Result<Runtime, Box<dyn std::error::Error>> {
    let target = BrowserDomElementSnapshot::new(
        1,
        "button",
        BTreeMap::from([("id".to_owned(), "target".to_owned())]),
        "target",
    )?;
    let snapshot = BrowserHostSnapshot::new(
        "https://events.test/listeners",
        "Event listener semantics",
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
fn passive_listener_cannot_cancel_default() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target()?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var event = { type: 'passive', cancelable: true }; \
         target.addEventListener('passive', function(event) { event.preventDefault(); }, { passive: true }); \
         var dispatched = target.dispatchEvent(event); \
         dispatched + '|' + event.defaultPrevented;",
    )?;

    assert_eq!(value, Value::String("true|false".to_owned()));
    Ok(())
}

#[test]
fn dispatch_snapshot_defers_added_listener_and_skips_removed_listener()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target()?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var log = ''; \
         var late = function(event) { log = log + 'L'; }; \
         var second = function(event) { log = log + 'B'; }; \
         var first = function(event) { log = log + 'A'; target.addEventListener('mutate', late); target.removeEventListener('mutate', second); }; \
         target.addEventListener('mutate', first); \
         target.addEventListener('mutate', second); \
         target.dispatchEvent({ type: 'mutate' }); \
         target.dispatchEvent({ type: 'mutate' }); \
         log;",
    )?;

    assert_eq!(value, Value::String("AAL".to_owned()));
    Ok(())
}

#[test]
fn listener_object_handle_event_uses_listener_as_this_and_removes_by_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target()?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var count = 0; \
         var listener = { handleEvent: function(event) { if (this === listener && event.currentTarget === target) { count = count + 1; } } }; \
         target.addEventListener('object-listener', listener); \
         target.dispatchEvent({ type: 'object-listener' }); \
         target.removeEventListener('object-listener', listener, { capture: false, passive: true, once: true }); \
         target.dispatchEvent({ type: 'object-listener' }); \
         count;",
    )?;

    assert_eq!(value, Value::Number(1.0));
    Ok(())
}

#[test]
fn duplicate_registration_keeps_first_passive_and_once_configuration()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target()?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var count = 0; \
         var listener = function(event) { count = count + 1; event.preventDefault(); }; \
         target.addEventListener('duplicate', listener, { passive: true }); \
         target.addEventListener('duplicate', listener, { once: true }); \
         var first = { type: 'duplicate', cancelable: true }; \
         var second = { type: 'duplicate', cancelable: true }; \
         var firstResult = target.dispatchEvent(first); \
         var secondResult = target.dispatchEvent(second); \
         count + '|' + firstResult + '|' + first.defaultPrevented + '|' + secondResult + '|' + second.defaultPrevented;",
    )?;

    assert_eq!(value, Value::String("2|true|false|true|false".to_owned()));
    Ok(())
}

#[test]
fn once_listener_is_removed_before_reentrant_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target()?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var count = 0; \
         var listener = function(event) { count = count + 1; target.dispatchEvent({ type: 'reentrant' }); }; \
         target.addEventListener('reentrant', listener, { once: true }); \
         target.dispatchEvent({ type: 'reentrant' }); \
         count;",
    )?;

    assert_eq!(value, Value::Number(1.0));
    Ok(())
}
