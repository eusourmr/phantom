//! Phantom 2H-5 Image/Text Composition I paint-order regression corpus.

use std::io;

use phantom_engine::{Engine, PaintCommand};

#[test]
fn inline_text_image_text_keeps_clip_and_layer_order() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<body style="margin:0">
            <p style="margin:0">before<img
                src="image.png"
                alt="image"
                width="80"
                height="40"
                style="object-fit:cover;object-position:right 10% bottom 20%"
            >after</p>
        </body>"#,
    )?;

    let paint = engine.paint_list();
    let commands = paint.commands();

    let before = commands.iter().position(|command| match command {
        PaintCommand::Text { text, .. } => paint
            .text(*text)
            .is_some_and(|content| content.contains("before")),
        _ => false,
    });
    let image = commands
        .iter()
        .position(|command| matches!(command, PaintCommand::Image { .. }));
    let after = commands.iter().rposition(|command| match command {
        PaintCommand::Text { text, .. } => paint
            .text(*text)
            .is_some_and(|content| content.contains("after")),
        _ => false,
    });

    let before = before.ok_or_else(|| io::Error::other("before text missing"))?;
    let image = image.ok_or_else(|| io::Error::other("image command missing"))?;
    let after = after.ok_or_else(|| io::Error::other("after text missing"))?;

    let clip_push = commands[..image]
        .iter()
        .rposition(|command| matches!(command, PaintCommand::PushClipRect { .. }))
        .ok_or_else(|| io::Error::other("image content clip push missing"))?;

    let clip_pop = commands[image + 1..]
        .iter()
        .position(|command| matches!(command, PaintCommand::PopClipRect))
        .map(|offset| image + 1 + offset)
        .ok_or_else(|| io::Error::other("image content clip pop missing"))?;

    assert!(before < clip_push);
    assert!(clip_push < image);
    assert!(image < clip_pop);
    assert!(clip_pop < after);

    let PaintCommand::PushClipRect { rect: clip_rect } = &commands[clip_push] else {
        return Err(io::Error::other("unexpected clip command").into());
    };
    let PaintCommand::Image { rect, position, .. } = &commands[image] else {
        return Err(io::Error::other("unexpected image command").into());
    };

    assert_eq!(clip_rect, rect);
    assert!((position.x() - 0.9).abs() < f32::EPSILON);
    assert!((position.y() - 0.8).abs() < f32::EPSILON);

    Ok(())
}

#[test]
fn nested_overflow_and_image_content_clips_remain_balanced()
-> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"<body style="margin:0">
            <div style="width:120px;height:80px;overflow:hidden">
                <img src="image.png" width="160" height="100" style="object-fit:none">
            </div>
        </body>"#,
    )?;

    let mut depth = 0_i32;
    let mut maximum_depth = 0_i32;

    for command in engine.paint_list().commands() {
        match command {
            PaintCommand::PushClipRect { .. } => {
                depth += 1;
                maximum_depth = maximum_depth.max(depth);
            }
            PaintCommand::PopClipRect => {
                depth -= 1;
                assert!(depth >= 0);
            }
            _ => {}
        }
    }

    assert_eq!(depth, 0);
    assert!(maximum_depth >= 2);
    Ok(())
}
