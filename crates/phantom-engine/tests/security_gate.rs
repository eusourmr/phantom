//! Security regression tests for the Phantom 2D-6 security gate.

use phantom_engine::{Engine, EngineState};
use phantom_html::MAX_HTML_SOURCE_BYTES;

#[test]
fn rejected_document_does_not_replace_engine_snapshots() {
    let mut engine = Engine::new();
    let source = "x".repeat(MAX_HTML_SOURCE_BYTES.saturating_add(1));

    assert!(engine.load_html(&source).is_err());
    assert_eq!(engine.state(), EngineState::Idle);
    assert_eq!(engine.document().len(), 1);
    assert!(engine.layout().is_empty());
    assert!(engine.paint_list().is_empty());
}

#[test]
fn bounded_deep_document_reaches_ready_state() {
    let mut engine = Engine::new();
    let source = format!("{}ok{}", "<div>".repeat(256), "</div>".repeat(256));

    assert!(engine.load_html(&source).is_ok());
    assert_eq!(engine.state(), EngineState::Ready);
    assert!(engine.document().len() <= 65_536);
}
