//! Phantom 2J-2 live JavaScript/DOM/paint integration regression corpus.

use phantom_engine::{Engine, JavaScriptValue, NodeKind, Rgba};

#[test]
fn javascript_text_content_mutation_reaches_real_dom_and_paint()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<body style=\"margin:0\"><p id=\"hero\">Before</p></body>")?;

    let value = engine.execute_javascript(
        "let hero = document.getElementById('hero'); \
         hero.textContent = 'After'; hero.textContent;",
        "https://example.test/",
        1.0,
    )?;

    assert_eq!(value, JavaScriptValue::String("After".to_owned()));

    let hero = engine
        .document()
        .nodes()
        .find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.attribute("id") == Some("hero") => {
                Some(node.id())
            }
            _ => None,
        });
    let Some(hero) = hero else {
        return Err(std::io::Error::other("hero element missing").into());
    };

    let text = engine
        .document()
        .node(hero)
        .and_then(|node| node.children().first())
        .and_then(|child| engine.document().node(*child))
        .and_then(|node| match node.kind() {
            NodeKind::Text(value) => Some(value.as_str()),
            _ => None,
        });

    assert_eq!(text, Some("After"));
    let painted = engine
        .paint_list()
        .commands()
        .iter()
        .any(|command| match command {
            phantom_engine::PaintCommand::Text { text, .. } => engine
                .paint_list()
                .text(*text)
                .is_some_and(|value| value.contains("After")),
            _ => false,
        });
    assert!(painted);
    Ok(())
}

#[test]
fn javascript_style_attribute_mutation_recomputes_css_layout_paint()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        "<body style=\"margin:0\"><p id=\"hero\" style=\"color:#000000\">Text</p></body>",
    )?;

    engine.execute_javascript(
        "document.getElementById('hero').setAttribute('style', 'color:#ff0000');",
        "https://example.test/",
        1.0,
    )?;

    let hero = engine
        .document()
        .nodes()
        .find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.attribute("id") == Some("hero") => {
                Some(node.id())
            }
            _ => None,
        });
    let Some(hero) = hero else {
        return Err(std::io::Error::other("hero element missing").into());
    };

    assert_eq!(
        engine.styles().get(hero).map(|style| style.color()),
        Some(Rgba::new(255, 0, 0, 255))
    );
    Ok(())
}

#[test]
fn query_selector_and_attribute_removal_use_live_engine_dom()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<div id=\"box\" data-state=\"ready\">Box</div>")?;

    let value = engine.execute_javascript(
        "let box = document.querySelector('#box'); \
         let before = box.getAttribute('data-state'); \
         box.removeAttribute('data-state'); before;",
        "https://example.test/",
        1.0,
    )?;

    assert_eq!(value, JavaScriptValue::String("ready".to_owned()));

    let has_attribute = engine.document().nodes().any(|node| match node.kind() {
        NodeKind::Element(element) if element.attribute("id") == Some("box") => {
            element.attribute("data-state").is_some()
        }
        _ => false,
    });
    assert!(!has_attribute);
    Ok(())
}
