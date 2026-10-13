//! Phantom 2J-4 DOM collection/document-boundary hardening regression corpus.

use phantom_engine::{Engine, JavaScriptValue, NodeKind};

#[test]
fn collections_follow_real_dom_order_after_reparent() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        "<html><body>\
         <div id=\"outside\"><span id=\"b\">B</span></div>\
         <main id=\"root\"><span id=\"a\">A</span></main>\
         </body></html>",
    )?;

    engine.execute_javascript(
        "document.getElementById('root').appendChild(document.getElementById('b'));",
        "https://example.test/",
        1.0,
    )?;

    let value = engine.execute_javascript(
        "let spans = document.querySelectorAll('span'); spans[0].id + '|' + spans[1].id;",
        "https://example.test/",
        1.0,
    )?;
    assert_eq!(value, JavaScriptValue::String("a|b".to_owned()));
    Ok(())
}

#[test]
fn loading_a_new_document_resets_javascript_global_state() -> Result<(), Box<dyn std::error::Error>>
{
    let mut engine = Engine::new();
    engine.load_html("<p id=\"first\">First</p>")?;
    engine.execute_javascript("var leaked = 41; leaked + 1;", "https://one.example/", 1.0)?;

    engine.load_html("<p id=\"second\">Second</p>")?;
    assert!(
        engine
            .execute_javascript("leaked;", "https://two.example/", 1.0)
            .is_err()
    );
    Ok(())
}

#[test]
fn failed_dom_batch_does_not_partially_mutate_real_document()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<div id=\"parent\"><span id=\"child\">Original</span></div>")?;

    let result = engine.execute_javascript(
        "let parent = document.getElementById('parent'); \
         let child = document.getElementById('child'); \
         parent.textContent = 'Replacement'; parent.appendChild(child);",
        "https://example.test/",
        1.0,
    );
    assert!(result.is_err());

    let parent = engine
        .document()
        .nodes()
        .find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.attribute("id") == Some("parent") => {
                Some(node.id())
            }
            _ => None,
        });
    let child = engine
        .document()
        .nodes()
        .find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.attribute("id") == Some("child") => {
                Some(node.id())
            }
            _ => None,
        });
    let (Some(parent), Some(child)) = (parent, child) else {
        return Err(std::io::Error::other("transactional DOM nodes missing").into());
    };
    assert_eq!(
        engine.document().node(child).and_then(|node| node.parent()),
        Some(parent)
    );

    let child_text = engine
        .document()
        .node(child)
        .and_then(|node| node.children().first())
        .and_then(|node_id| engine.document().node(*node_id))
        .and_then(|node| match node.kind() {
            NodeKind::Text(value) => Some(value.as_str()),
            _ => None,
        });
    assert_eq!(child_text, Some("Original"));
    Ok(())
}
