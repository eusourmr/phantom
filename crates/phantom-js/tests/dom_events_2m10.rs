//! Phantom 2M-10 DOM Event Control III regression corpus.

use std::collections::BTreeMap;

use phantom_js::{BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, Value};

fn runtime_with_tree() -> Result<Runtime, Box<dyn std::error::Error>> {
    let root = BrowserDomElementSnapshot::new(
        1,
        "div",
        BTreeMap::from([("id".to_owned(), "root".to_owned())]),
        "root",
    )?;
    let parent = BrowserDomElementSnapshot::new(
        2,
        "div",
        BTreeMap::from([("id".to_owned(), "parent".to_owned())]),
        "parent",
    )?
    .with_parent_node_id(Some(1));
    let target = BrowserDomElementSnapshot::new(
        3,
        "div",
        BTreeMap::from([("id".to_owned(), "target".to_owned())]),
        "target",
    )?
    .with_parent_node_id(Some(2));
    let snapshot = BrowserHostSnapshot::new(
        "https://events.test/control",
        "Event control",
        800.0,
        600.0,
        1.0,
    )?
    .with_dom_elements(vec![root, parent, target])?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn prevent_default_updates_default_prevented_and_dispatch_return()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_tree()?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var observed = ''; \
         target.addEventListener('save', function(event) { event.preventDefault(); observed = event.cancelable + '|' + event.defaultPrevented; }); \
         var event = { type: 'save', cancelable: true }; \
         var dispatched = target.dispatchEvent(event); \
         dispatched + '|' + event.defaultPrevented + '|' + observed;",
    )?;

    assert_eq!(value, Value::String("false|true|true|true".to_owned()));
    Ok(())
}

#[test]
fn stop_propagation_keeps_same_target_listeners_but_stops_ancestors()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_tree()?;
    let value = runtime.execute(
        "var parent = document.getElementById('parent'); \
         var target = document.getElementById('target'); \
         var log = ''; \
         target.addEventListener('halt', function(event) { log = log + 'A'; event.stopPropagation(); }); \
         target.addEventListener('halt', function(event) { log = log + 'B'; }); \
         parent.addEventListener('halt', function(event) { log = log + 'X'; }); \
         target.dispatchEvent({ type: 'halt', bubbles: true }); \
         log;",
    )?;

    assert_eq!(value, Value::String("AB".to_owned()));
    Ok(())
}

#[test]
fn stop_immediate_propagation_skips_remaining_target_and_ancestor_listeners()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_tree()?;
    let value = runtime.execute(
        "var parent = document.getElementById('parent'); \
         var target = document.getElementById('target'); \
         var log = ''; \
         target.addEventListener('halt-now', function(event) { log = log + 'A'; event.stopImmediatePropagation(); }); \
         target.addEventListener('halt-now', function(event) { log = log + 'X'; }); \
         parent.addEventListener('halt-now', function(event) { log = log + 'Y'; }); \
         target.dispatchEvent({ type: 'halt-now', bubbles: true }); \
         log;",
    )?;

    assert_eq!(value, Value::String("A".to_owned()));
    Ok(())
}

#[test]
fn once_option_removes_listener_before_second_dispatch()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_tree()?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var count = 0; \
         var listener = function(event) { count = count + 1; }; \
         target.addEventListener('once-only', listener, { once: true }); \
         target.dispatchEvent({ type: 'once-only' }); \
         target.dispatchEvent({ type: 'once-only' }); \
         count;",
    )?;

    assert_eq!(value, Value::Number(1.0));
    Ok(())
}
