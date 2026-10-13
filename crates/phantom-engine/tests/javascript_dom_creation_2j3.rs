//! Phantom 2J-3 real DOM node-creation integration regression corpus.

use phantom_engine::{Engine, JavaScriptValue, NodeKind};

#[test]
fn javascript_create_append_and_text_reach_dom_layout_and_paint()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<body style=\"margin:0\"><main id=\"root\"></main></body>")?;

    let value = engine.execute_javascript(
        "let card = document.createElement('section'); \
         card.id = 'created'; card.className = 'feature'; \
         card.textContent = 'Created by Phantom JS'; \
         document.getElementById('root').appendChild(card); \
         card.id;",
        "https://example.test/",
        1.0,
    )?;

    assert_eq!(value, JavaScriptValue::String("created".to_owned()));

    let created = engine
        .document()
        .nodes()
        .find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.attribute("id") == Some("created") => {
                Some(node.id())
            }
            _ => None,
        });
    let Some(created) = created else {
        return Err(std::io::Error::other("created element missing").into());
    };

    assert!(
        engine
            .document()
            .node(created)
            .and_then(|node| node.parent())
            .is_some()
    );
    assert!(
        engine
            .layout()
            .boxes()
            .iter()
            .any(|layout_box| layout_box.source_node() == created)
    );

    let painted_text = engine
        .paint_list()
        .commands()
        .iter()
        .filter_map(|command| match command {
            phantom_engine::PaintCommand::Text { text, .. } => engine.paint_list().text(*text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    let normalized_painted_text = painted_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(normalized_painted_text, "Created by Phantom JS");
    Ok(())
}

#[test]
fn nested_created_nodes_are_committed_in_mutation_order() -> Result<(), Box<dyn std::error::Error>>
{
    let mut engine = Engine::new();
    engine.load_html("<div id=\"root\"></div>")?;

    engine.execute_javascript(
        "let parent = document.createElement('article'); \
         parent.id = 'parent'; \
         let child = document.createElement('span'); \
         child.id = 'child'; child.textContent = 'Nested'; \
         parent.appendChild(child); \
         document.getElementById('root').appendChild(parent);",
        "https://example.test/",
        1.0,
    )?;

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
        return Err(std::io::Error::other("nested nodes missing").into());
    };
    assert_eq!(
        engine.document().node(child).and_then(|node| node.parent()),
        Some(parent)
    );
    Ok(())
}

#[test]
fn remove_detaches_element_and_rebuilds_layout() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<div id=\"remove-me\">Visible</div>")?;

    let removed = engine
        .document()
        .nodes()
        .find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.attribute("id") == Some("remove-me") => {
                Some(node.id())
            }
            _ => None,
        });
    let Some(removed) = removed else {
        return Err(std::io::Error::other("remove target missing").into());
    };

    engine.execute_javascript(
        "document.getElementById('remove-me').remove();",
        "https://example.test/",
        1.0,
    )?;

    assert_eq!(
        engine
            .document()
            .node(removed)
            .and_then(|node| node.parent()),
        None
    );
    assert!(
        !engine
            .layout()
            .boxes()
            .iter()
            .any(|layout_box| { layout_box.source_node() == removed })
    );
    Ok(())
}
