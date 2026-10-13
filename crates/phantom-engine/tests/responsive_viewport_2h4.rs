//! Phantom 2H-4 engine integration for responsive viewport recascade.

use phantom_dom::NodeKind;
use phantom_engine::{Display, Engine, LayoutKind, Rgba};

fn target_id(engine: &Engine, id: &str) -> Option<phantom_dom::NodeId> {
    engine
        .document()
        .nodes()
        .find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.attribute("id") == Some(id) => Some(node.id()),
            NodeKind::Document
            | NodeKind::Element(_)
            | NodeKind::Text(_)
            | NodeKind::Comment(_) => None,
        })
}

fn has_principal_layout_box(engine: &Engine, node_id: phantom_dom::NodeId) -> bool {
    engine.layout().boxes().iter().any(|layout_box| {
        layout_box.source_node() == node_id
            && matches!(
                layout_box.kind(),
                LayoutKind::Block
                    | LayoutKind::Flex
                    | LayoutKind::Image { .. }
                    | LayoutKind::Control { .. }
            )
    })
}

#[test]
fn viewport_relayout_recascades_media_queries_without_reparsing_html()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport_size(
        "<style>\
            #desktop { display:block; color:red; }\
            #mobile { display:none; color:green; }\
            @media (max-width: 700px) {\
                #desktop { display:none; color:blue; }\
                #mobile { display:block; color:blue; }\
            }\
         </style>\
         <body>\
            <div id='desktop'>Desktop</div>\
            <div id='mobile'>Mobile</div>\
         </body>",
        1000.0,
        700.0,
    )?;

    let original_node_count = engine.document().nodes().count();
    let desktop = target_id(&engine, "desktop")
        .ok_or_else(|| std::io::Error::other("desktop node missing"))?;
    let mobile =
        target_id(&engine, "mobile").ok_or_else(|| std::io::Error::other("mobile node missing"))?;

    assert_eq!(
        engine.styles().get(desktop).map(|style| style.display()),
        Some(Display::Block)
    );
    assert_eq!(
        engine.styles().get(desktop).map(|style| style.color()),
        Some(Rgba::new(255, 0, 0, 255))
    );
    assert_eq!(
        engine.styles().get(mobile).map(|style| style.display()),
        Some(Display::None)
    );
    assert!(has_principal_layout_box(&engine, desktop));
    assert!(!has_principal_layout_box(&engine, mobile));

    engine.relayout_with_viewport(600.0, 700.0)?;

    assert_eq!(engine.document().nodes().count(), original_node_count);
    assert_eq!(target_id(&engine, "desktop"), Some(desktop));
    assert_eq!(target_id(&engine, "mobile"), Some(mobile));
    assert_eq!(
        engine.styles().get(desktop).map(|style| style.display()),
        Some(Display::None)
    );
    assert_eq!(
        engine.styles().get(desktop).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );
    assert_eq!(
        engine.styles().get(mobile).map(|style| style.display()),
        Some(Display::Block)
    );
    assert!(!has_principal_layout_box(&engine, desktop));
    assert!(has_principal_layout_box(&engine, mobile));

    engine.relayout_with_viewport(1000.0, 700.0)?;
    assert_eq!(
        engine.styles().get(desktop).map(|style| style.display()),
        Some(Display::Block)
    );
    assert_eq!(
        engine.styles().get(mobile).map(|style| style.display()),
        Some(Display::None)
    );
    assert!(has_principal_layout_box(&engine, desktop));
    assert!(!has_principal_layout_box(&engine, mobile));

    Ok(())
}

#[test]
fn viewport_height_and_orientation_recascade_together() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport_size(
        "<style>\
            #target { color:red; }\
            @media (orientation: portrait) and (min-height: 600px) {\
                #target { color:blue; }\
            }\
         </style>\
         <body><div id='target'>Target</div></body>",
        900.0,
        500.0,
    )?;

    let target =
        target_id(&engine, "target").ok_or_else(|| std::io::Error::other("target node missing"))?;
    assert_eq!(
        engine.styles().get(target).map(|style| style.color()),
        Some(Rgba::new(255, 0, 0, 255))
    );

    engine.relayout_with_viewport(500.0, 800.0)?;
    assert_eq!(
        engine.styles().get(target).map(|style| style.color()),
        Some(Rgba::new(0, 0, 255, 255))
    );

    Ok(())
}
