//! Phantom 2G-15 stacking/z-index and viewport-attached paint regression corpus.

use phantom_engine::{Engine, PaintAttachment, PaintColor, PaintCommand};

fn fill_colors(engine: &Engine) -> Vec<PaintColor> {
    engine
        .paint_list()
        .commands()
        .iter()
        .filter_map(|command| match command {
            PaintCommand::FillRect { color, .. } => Some(*color),
            PaintCommand::Image { .. } | PaintCommand::Text { .. } => None,
        })
        .collect()
}

#[test]
fn explicit_positioned_z_index_reorders_sibling_subtrees() -> Result<(), Box<dyn std::error::Error>>
{
    let mut engine = Engine::new();
    engine.load_html(
        r#"<body style="margin:0">
            <div style="position:relative;z-index:5;width:20px;height:20px;background-color:rgb(255,0,0)"></div>
            <div style="position:relative;z-index:-2;width:20px;height:20px;background-color:rgb(0,255,0)"></div>
            <div style="position:relative;width:20px;height:20px;background-color:rgb(0,0,255)"></div>
        </body>"#,
    )?;

    let colors = fill_colors(&engine);
    let green = PaintColor::new(0, 255, 0, 255);
    let blue = PaintColor::new(0, 0, 255, 255);
    let red = PaintColor::new(255, 0, 0, 255);
    let selected = colors
        .into_iter()
        .filter(|color| *color == green || *color == blue || *color == red)
        .collect::<Vec<_>>();

    assert_eq!(selected, vec![green, blue, red]);
    Ok(())
}

#[test]
fn static_z_index_does_not_escape_source_order() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<body style="margin:0">
            <div style="z-index:999;width:20px;height:20px;background-color:rgb(255,0,0)"></div>
            <div style="position:relative;z-index:1;width:20px;height:20px;background-color:rgb(0,255,0)"></div>
        </body>"#,
    )?;

    let colors = fill_colors(&engine);
    let red = PaintColor::new(255, 0, 0, 255);
    let green = PaintColor::new(0, 255, 0, 255);
    let selected = colors
        .into_iter()
        .filter(|color| *color == red || *color == green)
        .collect::<Vec<_>>();

    assert_eq!(selected, vec![red, green]);
    Ok(())
}

#[test]
fn fixed_subtree_commands_are_marked_viewport_attached() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport_size(
        r#"<body style="margin:0">
            <div style="position:fixed;top:8px;left:8px;width:40px;height:20px;background-color:rgb(12,34,56)">fixed</div>
            <div style="width:40px;height:20px;background-color:rgb(90,80,70)"></div>
        </body>"#,
        640.0,
        360.0,
    )?;

    let fixed_color = PaintColor::new(12, 34, 56, 255);
    let document_color = PaintColor::new(90, 80, 70, 255);
    let mut fixed_attachment = None;
    let mut document_attachment = None;

    for (index, command) in engine.paint_list().commands().iter().enumerate() {
        if let PaintCommand::FillRect { color, .. } = command {
            if *color == fixed_color {
                fixed_attachment = engine.paint_list().attachment(index);
            } else if *color == document_color {
                document_attachment = engine.paint_list().attachment(index);
            }
        }
    }

    assert_eq!(fixed_attachment, Some(PaintAttachment::Viewport));
    assert_eq!(document_attachment, Some(PaintAttachment::Document));
    Ok(())
}
