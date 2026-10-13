//! Phantom 2K-3 fetch/Promise/DOM integration regression corpus.

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
fn fetch_completion_can_mutate_real_dom_and_rebuild_paint() -> Result<(), Box<dyn std::error::Error>>
{
    let mut engine = Engine::new();
    engine.load_html("<p id=\"target\">Before</p>")?;

    engine.execute_javascript(
        "fetch('/data').then(function(response) { \
             response.text().then(function(text) { \
                 document.getElementById('target').textContent = text; \
             }); \
         });",
        "https://example.test/app",
        1.0,
    )?;

    let requests = engine.drain_javascript_fetch_requests();
    assert_eq!(requests, vec![(1, "/data".to_owned())]);

    engine.complete_javascript_fetch_success(
        requests[0].0,
        200,
        "https://example.test/data",
        "After Fetch",
        "https://example.test/app",
        1.0,
    )?;

    assert_eq!(
        element_text(&engine, "target").as_deref(),
        Some("After Fetch")
    );
    assert!(painted_text(&engine).contains("After Fetch"));
    Ok(())
}

#[test]
fn fetch_failure_rejection_is_visible_to_following_script_job()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<div></div>")?;

    engine.execute_javascript(
        "var failure = ''; \
         fetch('/offline').catch(function(reason) { failure = reason; });",
        "https://example.test/app",
        1.0,
    )?;
    let request = engine.drain_javascript_fetch_requests().remove(0);

    engine.complete_javascript_fetch_failure(
        request.0,
        "offline",
        "https://example.test/app",
        1.0,
    )?;

    assert_eq!(
        engine.execute_javascript("failure;", "https://example.test/app", 1.0,)?,
        JavaScriptValue::String("offline".to_owned())
    );
    Ok(())
}

#[test]
fn new_document_discards_old_fetch_runtime_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<div>Old</div>")?;

    engine.execute_javascript("fetch('/old');", "https://old.example/", 1.0)?;
    assert_eq!(engine.drain_javascript_fetch_requests().len(), 1);

    engine.load_html("<div>New</div>")?;

    assert!(engine.drain_javascript_fetch_requests().is_empty());
    Ok(())
}
