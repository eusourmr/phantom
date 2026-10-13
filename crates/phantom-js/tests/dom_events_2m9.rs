//! Phantom 2M-9 DOM Event Propagation II regression corpus.

use std::collections::BTreeMap;

use phantom_js::{
    BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value,
};

fn runtime_with_tree() -> Result<Runtime, Box<dyn std::error::Error>> {
    // Simulate the browser bridge: the top element can point at a Document
    // NodeId that is intentionally absent from the element-only host snapshot.
    let root = BrowserDomElementSnapshot::new(
        1,
        "div",
        BTreeMap::from([("id".to_owned(), "root".to_owned())]),
        "root",
    )?
    .with_parent_node_id(Some(999));
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
    let snapshot =
        BrowserHostSnapshot::new("https://events.test/", "Propagation", 800.0, 600.0, 1.0)?
            .with_dom_elements(vec![root, parent, target])?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn bubbling_event_runs_capture_target_and_bubble_with_event_phase()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_tree()?;
    let value = runtime.execute(
        "var root = document.getElementById('root'); \
         var parent = document.getElementById('parent'); \
         var target = document.getElementById('target'); \
         var log = ''; \
         var rc = function(event) { if (this === root && event.target === target && event.currentTarget === root && event.eventPhase === 1) { log = log + 'R'; } else { log = log + 'X'; } }; \
         var pc = function(event) { if (this === parent && event.currentTarget === parent && event.eventPhase === 1) { log = log + 'P'; } else { log = log + 'X'; } }; \
         var tc = function(event) { if (this === target && event.currentTarget === target && event.eventPhase === 2) { log = log + 'T'; } else { log = log + 'X'; } }; \
         var tb = function(event) { if (event.eventPhase === 2) { log = log + 't'; } else { log = log + 'X'; } }; \
         var pb = function(event) { if (event.eventPhase === 3) { log = log + 'p'; } else { log = log + 'X'; } }; \
         var rb = function(event) { if (event.eventPhase === 3) { log = log + 'r'; } else { log = log + 'X'; } }; \
         root.addEventListener('go', rc, true); \
         parent.addEventListener('go', pc, true); \
         target.addEventListener('go', tc, true); \
         target.addEventListener('go', tb); \
         parent.addEventListener('go', pb); \
         root.addEventListener('go', rb); \
         var event = { type: 'go', bubbles: true }; \
         var dispatched = target.dispatchEvent(event); \
         log + '|' + dispatched + '|' + event.eventPhase + '|' + (event.currentTarget === null) + '|' + event.bubbles;",
    )?;

    assert_eq!(value, Value::String("RPTtpr|true|0|true|true".to_owned()));
    Ok(())
}

#[test]
fn non_bubbling_event_still_captures_ancestors_but_skips_ancestor_bubble()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_tree()?;
    let value = runtime.execute(
        "var root = document.getElementById('root'); \
         var parent = document.getElementById('parent'); \
         var target = document.getElementById('target'); \
         var log = ''; \
         var rc = function(event) { log = log + 'R'; }; \
         var pc = function(event) { log = log + 'P'; }; \
         var tc = function(event) { log = log + 'T'; }; \
         var tb = function(event) { log = log + 't'; }; \
         var pb = function(event) { log = log + 'x'; }; \
         var rb = function(event) { log = log + 'y'; }; \
         root.addEventListener('quiet', rc, true); \
         parent.addEventListener('quiet', pc, true); \
         target.addEventListener('quiet', tc, true); \
         target.addEventListener('quiet', tb); \
         parent.addEventListener('quiet', pb); \
         root.addEventListener('quiet', rb); \
         target.dispatchEvent({ type: 'quiet', bubbles: false }); \
         log;",
    )?;

    assert_eq!(value, Value::String("RPTt".to_owned()));
    Ok(())
}

#[test]
fn non_element_document_parent_terminates_element_path_without_error()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_tree()?;
    let value = runtime.execute(
        "var root = document.getElementById('root'); \
         var target = document.getElementById('target'); \
         var log = ''; \
         root.addEventListener('edge', function(event) { if (event.eventPhase === 1) { log = log + 'R'; } }, true); \
         target.addEventListener('edge', function(event) { if (event.eventPhase === 2) { log = log + 'T'; } }); \
         target.dispatchEvent({ type: 'edge', bubbles: true }); \
         log;",
    )?;

    assert_eq!(value, Value::String("RT".to_owned()));
    Ok(())
}

#[test]
fn event_ancestor_path_is_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let mut elements = Vec::new();
    for node_id in 1u64..=258u64 {
        let id = if node_id == 258 { "target" } else { "node" };
        let element = BrowserDomElementSnapshot::new(
            node_id,
            "div",
            BTreeMap::from([("id".to_owned(), format!("{id}-{node_id}"))]),
            "node",
        )?
        .with_parent_node_id((node_id > 1).then_some(node_id - 1));
        elements.push(element);
    }
    let snapshot = BrowserHostSnapshot::new(
        "https://events.test/deep",
        "Deep propagation",
        800.0,
        600.0,
        1.0,
    )?
    .with_dom_elements(elements)?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    let error = runtime
        .execute(
            "document.getElementById('target-258').dispatchEvent({ type: 'deep', bubbles: true });",
        )
        .err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(RuntimeErrorKind::EventPropagationPathExceeded)
    );
    Ok(())
}
