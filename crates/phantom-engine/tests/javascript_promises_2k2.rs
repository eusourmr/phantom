//! Phantom 2K-2 Promise/microtask engine integration regression corpus.

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
fn promise_microtask_reaches_real_dom_layout_and_paint() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<body><p id=\"target\">Before</p></body>")?;

    engine.execute_javascript(
        "Promise.resolve('After').then(function(value) { \
             document.getElementById('target').textContent = value; \
         });",
        "https://example.test/",
        1.0,
    )?;

    assert_eq!(element_text(&engine, "target").as_deref(), Some("After"));
    assert!(painted_text(&engine).contains("After"));
    Ok(())
}

#[test]
fn promise_chain_is_visible_to_following_script_job() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<div></div>")?;

    engine.execute_javascript(
        "var answer = 0; \
         Promise.resolve(20) \
             .then(function(value) { return value + 22; }) \
             .then(function(value) { answer = value; });",
        "https://example.test/",
        1.0,
    )?;

    assert_eq!(
        engine.execute_javascript("answer;", "https://example.test/", 1.0,)?,
        JavaScriptValue::Number(42.0)
    );
    Ok(())
}

#[test]
fn timer_pump_runs_promise_microtasks_before_next_timer() -> Result<(), Box<dyn std::error::Error>>
{
    let mut engine = Engine::new();
    engine.load_html("<div></div>")?;

    engine.execute_javascript(
        "var order = ''; \
         setTimeout(function() { \
             order = order + 'A'; \
             Promise.resolve(1).then(function() { order = order + 'M'; }); \
         }, 0); \
         setTimeout(function() { order = order + 'B'; }, 0);",
        "https://example.test/",
        1.0,
    )?;

    assert_eq!(
        engine.advance_javascript_time(0, "https://example.test/", 1.0,)?,
        2
    );
    assert_eq!(
        engine.execute_javascript("order;", "https://example.test/", 1.0,)?,
        JavaScriptValue::String("AMB".to_owned())
    );
    Ok(())
}
