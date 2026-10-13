//! Phantom 2G-5 inline replaced-element baseline regression corpus.

use std::io;

use phantom_css::compute_styles;
use phantom_html::parse;
use phantom_layout::{ControlKind, LayoutBox, LayoutKind, LayoutSnapshot, build_layout_snapshot};
use phantom_text::{
    FallbackTextShaper, FontFamily, FontMetrics, FontSlant, FontWeight, TextShaper, TextStyle,
};

const EPSILON: f32 = 0.05;

fn layout_for(source: &str) -> Result<LayoutSnapshot, Box<dyn std::error::Error>> {
    let document = parse(source)?;
    let styles = compute_styles(&document);
    Ok(build_layout_snapshot(&document, &styles, 800.0)?)
}

fn find_text_box<'a>(
    layout: &'a LayoutSnapshot,
    expected: &str,
) -> Result<&'a LayoutBox, io::Error> {
    layout
        .boxes()
        .iter()
        .find(|layout_box| {
            matches!(layout_box.kind(), LayoutKind::Text { .. })
                && layout.text_for(layout_box) == Some(expected)
        })
        .ok_or_else(|| io::Error::other(format!("missing text fragment: {expected}")))
}

fn find_image_box(layout: &LayoutSnapshot) -> Result<&LayoutBox, io::Error> {
    layout
        .boxes()
        .iter()
        .find(|layout_box| matches!(layout_box.kind(), LayoutKind::Image { .. }))
        .ok_or_else(|| io::Error::other("missing inline image"))
}

fn find_submit_control(layout: &LayoutSnapshot) -> Result<&LayoutBox, io::Error> {
    layout
        .boxes()
        .iter()
        .find(|layout_box| {
            matches!(
                layout_box.kind(),
                LayoutKind::Control {
                    kind: ControlKind::SubmitButton
                }
            )
        })
        .ok_or_else(|| io::Error::other("missing submit control"))
}

fn metrics(font_size: f32) -> FontMetrics {
    let shaper = FallbackTextShaper;
    shaper.font_metrics(TextStyle::new(
        font_size,
        FontFamily::SansSerif,
        FontWeight::Normal,
        FontSlant::Normal,
    ))
}

fn text_baseline(layout_box: &LayoutBox, font_size: f32) -> f32 {
    layout_box.rect().y() + metrics(font_size).ascent()
}

fn replaced_baseline(layout_box: &LayoutBox) -> f32 {
    layout_box.rect().y() + layout_box.rect().height() + layout_box.margin().bottom()
}

#[test]
fn text_then_image_share_default_inline_baseline() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        r#"<div><span style="font-size:40px">Text</span><img src="x.png" width="12" height="12"></div>"#,
    )?;

    let text = find_text_box(&layout, "Text")?;
    let image = find_image_box(&layout)?;

    assert_eq!(text.parent(), image.parent());
    assert!((text_baseline(text, 40.0) - replaced_baseline(image)).abs() <= EPSILON);

    Ok(())
}

#[test]
fn image_before_large_text_is_repositioned_when_baseline_grows()
-> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        r#"<div><img src="x.png" width="12" height="12"><span style="font-size:48px">Large</span></div>"#,
    )?;

    let image = find_image_box(&layout)?;
    let text = find_text_box(&layout, "Large")?;

    assert_eq!(text.parent(), image.parent());
    assert!(image.rect().y() > 0.0);
    assert!((text_baseline(text, 48.0) - replaced_baseline(image)).abs() <= EPSILON);

    Ok(())
}

#[test]
fn submit_control_participates_in_inline_baseline() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        r#"<div><input type="submit" style="display:inline"><span style="font-size:48px">Text</span></div>"#,
    )?;

    let control = find_submit_control(&layout)?;
    let text = find_text_box(&layout, "Text")?;

    assert_eq!(text.parent(), control.parent());
    assert!((text_baseline(text, 48.0) - replaced_baseline(control)).abs() <= EPSILON);

    Ok(())
}

#[test]
fn tall_image_contributes_to_line_ascent_without_losing_text_descent()
-> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        r#"<div><span style="font-size:20px">Text</span><img src="x.png" width="20" height="60"></div>"#,
    )?;

    let text = find_text_box(&layout, "Text")?;
    let image = find_image_box(&layout)?;
    let line_id = text
        .parent()
        .ok_or_else(|| io::Error::other("text has no line parent"))?;
    let line = layout
        .get(line_id)
        .ok_or_else(|| io::Error::other("line box is missing"))?;

    assert_eq!(image.parent(), Some(line_id));
    assert!(matches!(line.kind(), LayoutKind::Line));

    let text_metrics = metrics(20.0);
    let expected_minimum = image.rect().height() + text_metrics.descent() + text_metrics.line_gap();

    assert!(line.rect().height() + EPSILON >= expected_minimum);

    Ok(())
}
