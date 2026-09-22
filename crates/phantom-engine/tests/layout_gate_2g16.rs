//! Phantom 2G-16 final layout-gate corpus.

// PHANTOM_2G16_FINAL_LAYOUT_GATE

use std::io;

use phantom_dom::{Document, NodeId, NodeKind};
use phantom_engine::{
    Engine, LayoutKind, LayoutSnapshot, PaintAttachment, PaintColor, PaintCommand, Rect,
};

fn node_by_id(document: &Document, id: &str) -> Option<NodeId> {
    document.nodes().find_map(|node| {
        let NodeKind::Element(element) = node.kind() else {
            return None;
        };
        (element.attribute("id") == Some(id)).then_some(node.id())
    })
}

fn rect_by_id(document: &Document, snapshot: &LayoutSnapshot, id: &str) -> Option<Rect> {
    let node_id = node_by_id(document, id)?;
    snapshot.boxes().iter().find_map(|layout_box| {
        if layout_box.source_node() != node_id {
            return None;
        }
        match layout_box.kind() {
            LayoutKind::Block
            | LayoutKind::Flex
            | LayoutKind::Image { .. }
            | LayoutKind::Control { .. } => Some(layout_box.rect()),
            LayoutKind::Root | LayoutKind::Line | LayoutKind::Text { .. } => None,
        }
    })
}

fn required_rect(engine: &Engine, id: &str) -> Result<Rect, Box<dyn std::error::Error>> {
    rect_by_id(engine.document(), engine.layout(), id)
        .ok_or_else(|| io::Error::other(format!("layout geometry missing for #{id}")).into())
}

fn fill_colors(engine: &Engine) -> Vec<PaintColor> {
    engine
        .paint_list()
        .commands()
        .iter()
        .filter_map(|command| match command {
            PaintCommand::FillRect { color, .. } => Some(*color),
            _ => None,
        })
        .collect()
}

#[test]
fn relayout_recomputes_absolute_and_fixed_percentages_from_new_viewport()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport_size(
        r#"<body style="margin:0">
            <div id="absolute" style="position:absolute;left:10%;top:50%;width:20px;height:10px"></div>
            <div id="fixed" style="position:fixed;left:10%;top:50%;width:20px;height:10px"></div>
        </body>"#,
        400.0,
        200.0,
    )?;

    let absolute_before = required_rect(&engine, "absolute")?;
    let fixed_before = required_rect(&engine, "fixed")?;
    assert!((absolute_before.x() - 40.0).abs() < 0.1);
    assert!((absolute_before.y() - 100.0).abs() < 0.1);
    assert!((fixed_before.x() - 40.0).abs() < 0.1);
    assert!((fixed_before.y() - 100.0).abs() < 0.1);

    engine.relayout_with_viewport(800.0, 300.0)?;

    let absolute_after = required_rect(&engine, "absolute")?;
    let fixed_after = required_rect(&engine, "fixed")?;
    assert!((absolute_after.x() - 80.0).abs() < 0.1);
    assert!((absolute_after.y() - 150.0).abs() < 0.1);
    assert!((fixed_after.x() - 80.0).abs() < 0.1);
    assert!((fixed_after.y() - 150.0).abs() < 0.1);
    Ok(())
}

#[test]
fn coarse_stack_order_combines_negative_normal_zero_and_positive_positioned_content()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport_size(
        r#"<body style="margin:0">
            <div style="position:absolute;z-index:5;left:0;top:0;width:20px;height:20px;background-color:rgb(255,0,0)"></div>
            <div style="width:20px;height:20px;background-color:rgb(0,0,255)"></div>
            <div style="position:fixed;z-index:-2;left:0;top:0;width:20px;height:20px;background-color:rgb(0,255,0)"></div>
            <div style="position:relative;z-index:0;width:20px;height:20px;background-color:rgb(255,255,0)"></div>
        </body>"#,
        640.0,
        360.0,
    )?;

    let red = PaintColor::new(255, 0, 0, 255);
    let blue = PaintColor::new(0, 0, 255, 255);
    let green = PaintColor::new(0, 255, 0, 255);
    let yellow = PaintColor::new(255, 255, 0, 255);
    let selected = fill_colors(&engine)
        .into_iter()
        .filter(|color| *color == red || *color == blue || *color == green || *color == yellow)
        .collect::<Vec<_>>();

    assert_eq!(selected, vec![green, blue, yellow, red]);
    Ok(())
}

#[test]
fn equal_positive_z_index_keeps_source_order_across_positioned_kinds()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport_size(
        r#"<body style="margin:0">
            <div style="position:relative;z-index:3;width:20px;height:20px;background-color:rgb(255,0,0)"></div>
            <div style="position:fixed;z-index:3;left:0;top:0;width:20px;height:20px;background-color:rgb(0,255,0)"></div>
            <div style="position:absolute;z-index:3;left:0;top:0;width:20px;height:20px;background-color:rgb(0,0,255)"></div>
        </body>"#,
        640.0,
        360.0,
    )?;

    let red = PaintColor::new(255, 0, 0, 255);
    let green = PaintColor::new(0, 255, 0, 255);
    let blue = PaintColor::new(0, 0, 255, 255);
    let selected = fill_colors(&engine)
        .into_iter()
        .filter(|color| *color == red || *color == green || *color == blue)
        .collect::<Vec<_>>();

    assert_eq!(selected, vec![red, green, blue]);
    Ok(())
}

#[test]
fn viewport_attachment_survives_stacking_reorder_and_relayout()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport_size(
        r#"<body style="margin:0">
            <div style="position:fixed;z-index:10;left:8px;top:8px;width:30px;height:20px;background-color:rgb(12,34,56)">PIN</div>
            <div style="width:30px;height:20px;background-color:rgb(90,80,70)"></div>
        </body>"#,
        500.0,
        300.0,
    )?;

    for _ in 0..2 {
        let fixed_color = PaintColor::new(12, 34, 56, 255);
        let document_color = PaintColor::new(90, 80, 70, 255);
        let mut fixed_fill_attachment = None;
        let mut document_fill_attachment = None;
        let mut fixed_text_attachment = None;

        for (index, command) in engine.paint_list().commands().iter().enumerate() {
            match command {
                PaintCommand::FillRect { color, .. } if *color == fixed_color => {
                    fixed_fill_attachment = engine.paint_list().attachment(index);
                }
                PaintCommand::FillRect { color, .. } if *color == document_color => {
                    document_fill_attachment = engine.paint_list().attachment(index);
                }
                PaintCommand::Text { text, .. }
                    if engine.paint_list().text(*text) == Some("PIN") =>
                {
                    fixed_text_attachment = engine.paint_list().attachment(index);
                }
                _ => {}
            }
        }

        assert_eq!(fixed_fill_attachment, Some(PaintAttachment::Viewport));
        assert_eq!(fixed_text_attachment, Some(PaintAttachment::Viewport));
        assert_eq!(document_fill_attachment, Some(PaintAttachment::Document));

        engine.relayout_with_viewport(760.0, 420.0)?;
    }

    Ok(())
}
