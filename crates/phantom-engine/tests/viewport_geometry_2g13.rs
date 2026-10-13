//! Phantom 2G-13 engine viewport geometry regression corpus.

use phantom_engine::Engine;

#[test]
fn explicit_viewport_size_reaches_layout_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport_size(
        "<body style=\"margin:0\"><div style=\"height:20px\"></div></body>",
        720.0,
        405.0,
    )?;

    assert!((engine.layout().viewport_width() - 720.0).abs() < f32::EPSILON);
    assert!((engine.layout().viewport_height() - 405.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn relayout_updates_both_viewport_axes_without_reparsing() -> Result<(), Box<dyn std::error::Error>>
{
    let mut engine = Engine::new();
    engine.load_html("<body style=\"margin:0\"><p>viewport</p></body>")?;
    let node_count = engine.document().len();

    engine.relayout_with_viewport(900.0, 520.0)?;

    assert_eq!(engine.document().len(), node_count);
    assert!((engine.layout().viewport_width() - 900.0).abs() < f32::EPSILON);
    assert!((engine.layout().viewport_height() - 520.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn width_only_relayout_preserves_current_viewport_height() -> Result<(), Box<dyn std::error::Error>>
{
    let mut engine = Engine::new();
    engine.load_html_with_viewport_size("<body></body>", 640.0, 360.0)?;

    engine.relayout(800.0)?;

    assert!((engine.layout().viewport_width() - 800.0).abs() < f32::EPSILON);
    assert!((engine.layout().viewport_height() - 360.0).abs() < f32::EPSILON);
    Ok(())
}
