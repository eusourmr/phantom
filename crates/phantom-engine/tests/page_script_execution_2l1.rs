//! Regression tests for Phantom 2L-1 inline page script execution.

use phantom_engine::{Engine, JavaScriptValue, NodeKind};

fn text_at(engine: &Engine, id: &str) -> Option<String> {
    let element = engine.document().nodes().find(|node| {
        matches!(node.kind(), NodeKind::Element(data) if data.attribute("id") == Some(id))
    })?;
    Some(element.children().iter().filter_map(|child| {
        match engine.document().node(*child)?.kind() {
            NodeKind::Text(text) => Some(text.as_str()),
            _ => None,
        }
    }).collect())
}

#[test]
fn inline_scripts_share_runtime_and_update_paint_in_order() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<p id='result'>before</p><script>var answer = 'first';</script><script>document.getElementById('result').textContent = answer; answer = 'second';</script>")?;
    assert_eq!(text_at(&engine, "result").as_deref(), Some("before"));
    assert_eq!(engine.execute_inline_page_scripts("https://example.test/", 1.0)?, 2);
    assert_eq!(text_at(&engine, "result").as_deref(), Some("first"));
    assert_eq!(engine.execute_javascript("answer;", "https://example.test/", 1.0)?, JavaScriptValue::String("second".to_owned()));
    assert!(engine.paint_list().commands().iter().any(|command| match command {
        phantom_engine::PaintCommand::Text { text, .. } => engine.paint_list().text(*text).is_some_and(|value| value.contains("first")),
        _ => false,
    }));
    Ok(())
}

#[test]
fn unsupported_script_forms_stay_blocked_while_classic_defer_executes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<script src='/external.js'>unknown();</script><script type='module'>unknown();</script><script async>unknown();</script><script defer>var deferred_ready = 11;</script><script type='application/json'>{bad}</script><script type='text/javascript'>var ready = 7;</script>")?;
    assert_eq!(
        engine.execute_inline_page_scripts("https://example.test/", 1.0)?,
        2
    );
    assert_eq!(
        engine.execute_javascript("ready;", "https://example.test/", 1.0)?,
        JavaScriptValue::Number(7.0)
    );
    assert_eq!(
        engine.execute_javascript("deferred_ready;", "https://example.test/", 1.0)?,
        JavaScriptValue::Number(11.0)
    );
    Ok(())
}
#[test]
fn new_navigation_resets_script_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<script>var oldPage = 5;</script>")?;
    assert_eq!(engine.execute_inline_page_scripts("https://example.test/one", 1.0)?, 1);
    engine.load_html("<script>var fresh = 9;</script>")?;
    assert_eq!(engine.execute_inline_page_scripts("https://example.test/two", 1.0)?, 1);
    assert_eq!(engine.execute_javascript("fresh;", "https://example.test/two", 1.0)?, JavaScriptValue::Number(9.0));
    Ok(())
}
