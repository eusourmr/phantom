//! Phantom 2H-2 Box Visual Fidelity I engine/paint regression corpus.

use std::io;

use phantom_engine::{Engine, PaintColor, PaintCommand};

#[test]
fn rounded_background_and_uniform_border_emit_native_box_commands()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<body style="margin:0">
            <div style="width:100px;height:60px;background-color:rgb(10,20,30);border:4px solid rgb(40,50,60);border-radius:12px"></div>
        </body>"#,
    )?;

    let background = PaintColor::new(10, 20, 30, 255);
    let border = PaintColor::new(40, 50, 60, 255);
    let mut rounded_background = false;
    let mut rounded_border = false;

    for command in engine.paint_list().commands() {
        match command {
            PaintCommand::FillRect { radius, color, .. } if *color == background => {
                assert_eq!(*radius, 12.0);
                rounded_background = true;
            }
            PaintCommand::StrokeRect {
                radius,
                width,
                color,
                ..
            } if *color == border => {
                assert_eq!(*radius, 12.0);
                assert_eq!(*width, 4.0);
                rounded_border = true;
            }
            _ => {}
        }
    }

    assert!(rounded_background);
    assert!(rounded_border);
    Ok(())
}

#[test]
fn percentage_radius_is_resolved_against_the_smaller_box_dimension()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<body style="margin:0">
            <div style="width:100px;height:40px;background-color:rgb(11,22,33);border-radius:50%"></div>
        </body>"#,
    )?;

    let target = PaintColor::new(11, 22, 33, 255);
    let radius = engine
        .paint_list()
        .commands()
        .iter()
        .find_map(|command| match command {
            PaintCommand::FillRect { radius, color, .. } if *color == target => Some(*radius),
            _ => None,
        });

    assert_eq!(radius, Some(20.0));
    Ok(())
}

#[test]
fn nested_opacity_multiplies_effective_paint_alpha() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<body style="margin:0">
            <div style="opacity:.5">
                <div style="opacity:.5;width:20px;height:20px;background-color:rgb(255,0,0)">x</div>
            </div>
        </body>"#,
    )?;

    let mut background_alpha = None;
    let mut text_alpha = None;

    for command in engine.paint_list().commands() {
        match command {
            PaintCommand::FillRect { color, .. }
                if color.red() == 255 && color.green() == 0 && color.blue() == 0 =>
            {
                background_alpha = Some(color.alpha());
            }
            PaintCommand::Text { color, .. } => {
                text_alpha = Some(color.alpha());
            }
            _ => {}
        }
    }

    assert_eq!(background_alpha, Some(64));
    assert_eq!(text_alpha, Some(64));
    Ok(())
}

#[test]
fn overflow_hidden_emits_balanced_descendant_clip_commands()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<body style="margin:0">
            <div style="width:100px;height:50px;overflow:hidden">
                <div style="width:240px;height:80px;background-color:rgb(1,2,3)"></div>
            </div>
        </body>"#,
    )?;

    let commands = engine.paint_list().commands();
    let push = commands
        .iter()
        .position(|command| matches!(command, PaintCommand::PushClipRect { .. }));
    let child = commands.iter().position(|command| {
        matches!(
            command,
            PaintCommand::FillRect { color, .. }
                if *color == PaintColor::new(1, 2, 3, 255)
        )
    });
    let pop = commands
        .iter()
        .position(|command| matches!(command, PaintCommand::PopClipRect));

    let push = push.ok_or_else(|| io::Error::other("missing clip push"))?;
    let child = child.ok_or_else(|| io::Error::other("missing clipped child"))?;
    let pop = pop.ok_or_else(|| io::Error::other("missing clip pop"))?;

    assert!(push < child);
    assert!(child < pop);

    let PaintCommand::PushClipRect { rect } = &commands[push] else {
        return Err(io::Error::other("unexpected clip command").into());
    };
    assert_eq!(rect.width(), 100.0);
    assert_eq!(rect.height(), 50.0);
    Ok(())
}

#[test]
fn nonuniform_border_keeps_existing_per_edge_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<body style="margin:0">
            <div style="width:50px;height:40px;border-style:solid;border-color:rgb(9,8,7);border-width:1px 2px 3px 4px;border-radius:10px"></div>
        </body>"#,
    )?;

    let border = PaintColor::new(9, 8, 7, 255);
    let stroke_count = engine
        .paint_list()
        .commands()
        .iter()
        .filter(
            |command| matches!(command, PaintCommand::StrokeRect { color, .. } if *color == border),
        )
        .count();
    let edge_count = engine
        .paint_list()
        .commands()
        .iter()
        .filter(|command| matches!(command, PaintCommand::FillRect { color, radius, .. } if *color == border && *radius == 0.0))
        .count();

    assert_eq!(stroke_count, 0);
    assert_eq!(edge_count, 4);
    Ok(())
}
