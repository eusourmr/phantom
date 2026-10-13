//! Phantom 2K-1 timer/DOM/paint integration regression corpus.

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
fn timer_callback_mutates_real_dom_and_rebuilds_paint_at_deadline()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<body><p id=\"target\">Before</p></body>")?;

    let timer_id = engine.execute_javascript(
        "setTimeout(function() { \
             document.getElementById('target').textContent = 'After'; \
         }, 10);",
        "https://example.test/",
        1.0,
    )?;
    assert!(matches!(timer_id, JavaScriptValue::Number(_)));

    assert_eq!(
        engine.advance_javascript_time(9, "https://example.test/", 1.0)?,
        0
    );
    assert_eq!(element_text(&engine, "target").as_deref(), Some("Before"));

    assert_eq!(
        engine.advance_javascript_time(1, "https://example.test/", 1.0)?,
        1
    );
    assert_eq!(element_text(&engine, "target").as_deref(), Some("After"));
    assert!(painted_text(&engine).contains("After"));
    Ok(())
}

#[test]
fn clear_timeout_prevents_dom_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<p id=\"target\">Before</p>")?;

    engine.execute_javascript(
        "var timer = setTimeout(function() { \
             document.getElementById('target').textContent = 'Wrong'; \
         }, 5); \
         clearTimeout(timer);",
        "https://example.test/",
        1.0,
    )?;

    assert_eq!(
        engine.advance_javascript_time(10, "https://example.test/", 1.0)?,
        0
    );
    assert_eq!(element_text(&engine, "target").as_deref(), Some("Before"));
    Ok(())
}

#[test]
fn loading_new_document_discards_old_document_timers() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<p id=\"old\">Old</p>")?;

    engine.execute_javascript(
        "setTimeout(function() { \
             document.getElementById('old').textContent = 'Leaked'; \
         }, 1);",
        "https://old.example/",
        1.0,
    )?;

    engine.load_html("<p id=\"new\">New</p>")?;

    assert_eq!(
        engine.advance_javascript_time(10, "https://new.example/", 1.0)?,
        0
    );
    assert_eq!(element_text(&engine, "new").as_deref(), Some("New"));
    assert_eq!(element_text(&engine, "old"), None);
    Ok(())
}
