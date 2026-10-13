//! Phantom 2K-4 engine-level async integration final gate.
//!
//! PHANTOM_2K4_ENGINE_ASYNC_FINAL_GATE

use phantom_engine::{Engine, JavaScriptValue, NodeKind};

fn element_text(engine: &Engine, id: &str) -> Option<String> {
    let node_id = engine
        .document()
        .nodes()
        .find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.attribute("id") == Some(id) => Some(node.id()),
            _ => None,
        })?;

    let node = engine.document().node(node_id)?;
    let mut text = String::new();
    let mut stack = node.children().iter().rev().copied().collect::<Vec<_>>();

    while let Some(current_id) = stack.pop() {
        let current = engine.document().node(current_id)?;
        match current.kind() {
            NodeKind::Text(value) => text.push_str(value),
            NodeKind::Document | NodeKind::Element(_) | NodeKind::Comment(_) => {}
        }
        stack.extend(current.children().iter().rev().copied());
    }

    Some(text)
}

fn painted_text(engine: &Engine) -> String {
    engine
        .paint_list()
        .commands()
        .iter()
        .filter_map(|command| match command {
            phantom_engine::PaintCommand::Text { text, .. } => engine.paint_list().text(*text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn sync_microtask_fetch_and_timer_rebuild_dom_in_explicit_host_order()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<p id=\"target\">Before</p>")?;

    engine.execute_javascript(
        "var order = ''; \
         document.getElementById('target').textContent = 'Sync'; \
         queueMicrotask(function() { \
             order = order + 'M'; \
             document.getElementById('target').textContent = 'Micro'; \
         }); \
         setTimeout(function() { \
             order = order + 'T'; \
             document.getElementById('target').textContent = 'Timer'; \
         }, 0); \
         fetch('/data').then(function(response) { \
             order = order + 'F'; \
             response.text().then(function(text) { \
                 order = order + 'R'; \
                 document.getElementById('target').textContent = text; \
             }); \
         }); \
         order = order + 'S';",
        "https://example.test/app",
        1.0,
    )?;

    assert_eq!(element_text(&engine, "target").as_deref(), Some("Micro"));
    assert_eq!(
        engine.execute_javascript("order;", "https://example.test/app", 1.0)?,
        JavaScriptValue::String("SM".to_owned())
    );

    let requests = engine.drain_javascript_fetch_requests();
    assert_eq!(requests, vec![(1, "/data".to_owned())]);

    engine.complete_javascript_fetch_success(
        requests[0].0,
        200,
        "https://example.test/data",
        "Fetch",
        "https://example.test/app",
        1.0,
    )?;

    assert_eq!(element_text(&engine, "target").as_deref(), Some("Fetch"));
    assert_eq!(
        engine.execute_javascript("order;", "https://example.test/app", 1.0)?,
        JavaScriptValue::String("SMFR".to_owned())
    );

    assert_eq!(
        engine.advance_javascript_time(0, "https://example.test/app", 1.0,)?,
        1
    );
    assert_eq!(element_text(&engine, "target").as_deref(), Some("Timer"));
    assert!(painted_text(&engine).contains("Timer"));
    assert_eq!(
        engine.execute_javascript("order;", "https://example.test/app", 1.0)?,
        JavaScriptValue::String("SMFRT".to_owned())
    );
    Ok(())
}

#[test]
fn failed_fetch_reaction_does_not_leak_partial_dom_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<p id=\"target\">Before</p>")?;

    engine.execute_javascript(
        "fetch('/transaction').then(function() { \
             document.getElementById('target').textContent = 'Dirty'; \
             missingFetchReactionIdentifier; \
         });",
        "https://example.test/app",
        1.0,
    )?;

    let requests = engine.drain_javascript_fetch_requests();
    assert_eq!(requests.len(), 1);

    let completion = engine.complete_javascript_fetch_success(
        requests[0].0,
        200,
        "https://example.test/transaction",
        "payload",
        "https://example.test/app",
        1.0,
    );
    assert!(completion.is_err());
    assert_eq!(element_text(&engine, "target").as_deref(), Some("Before"));

    engine.complete_javascript_fetch_failure(
        requests[0].0,
        "fallback",
        "https://example.test/app",
        1.0,
    )?;
    assert_eq!(element_text(&engine, "target").as_deref(), Some("Before"));
    Ok(())
}

#[test]
fn navigation_resets_combined_async_generation() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<div id=\"old\">Old</div>")?;

    engine.execute_javascript(
        "var oldMarker = 7; \
         setTimeout(function() { oldMarker = 8; }, 0); \
         fetch('/old');",
        "https://old.example/",
        1.0,
    )?;
    let old_requests = engine.drain_javascript_fetch_requests();
    assert_eq!(old_requests.len(), 1);

    engine.load_html("<div id=\"new\">New</div>")?;

    assert!(engine.drain_javascript_fetch_requests().is_empty());
    assert_eq!(
        engine.advance_javascript_time(0, "https://new.example/", 1.0,)?,
        0
    );

    let stale_completion = engine.complete_javascript_fetch_success(
        old_requests[0].0,
        200,
        "https://old.example/old",
        "stale",
        "https://new.example/",
        1.0,
    );
    assert!(stale_completion.is_err());

    let old_global = engine.execute_javascript("oldMarker;", "https://new.example/", 1.0);
    assert!(old_global.is_err());
    assert_eq!(element_text(&engine, "new").as_deref(), Some("New"));
    Ok(())
}
