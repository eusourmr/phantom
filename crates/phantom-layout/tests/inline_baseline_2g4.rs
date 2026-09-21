//! Phantom 2G-4 inline baseline and typography regression corpus.

use std::io;

use phantom_css::compute_styles;
use phantom_html::parse;
use phantom_layout::{LayoutBox, LayoutKind, LayoutSnapshot, Rect, build_layout_snapshot};
use phantom_text::{
    FallbackTextShaper, FontFamily, FontMetrics, FontSlant, FontWeight, TextShaper, TextStyle,
};

const EPSILON: f32 = 0.05;

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

fn fallback_metrics(font_size: f32) -> FontMetrics {
    let shaper = FallbackTextShaper;
    shaper.font_metrics(TextStyle::new(
        font_size,
        FontFamily::SansSerif,
        FontWeight::Normal,
        FontSlant::Normal,
    ))
}

fn layout_for(source: &str) -> Result<LayoutSnapshot, Box<dyn std::error::Error>> {
    let document = parse(source)?;
    let styles = compute_styles(&document);
    Ok(build_layout_snapshot(&document, &styles, 800.0)?)
}

fn baseline(rect: Rect, metrics: FontMetrics) -> f32 {
    rect.y() + metrics.ascent()
}

#[test]
fn later_larger_text_realigns_earlier_text_to_shared_baseline()
-> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        r#"<div><span style="font-size:10px">small</span><span style="font-size:30px">BIG</span></div>"#,
    )?;

    let small = find_text_box(&layout, "small")?;
    let big = find_text_box(&layout, "BIG")?;

    let small_baseline = baseline(small.rect(), fallback_metrics(10.0));
    let big_baseline = baseline(big.rect(), fallback_metrics(30.0));

    assert!((small_baseline - big_baseline).abs() <= EPSILON);
    assert!(small.rect().y() > big.rect().y());

    Ok(())
}

#[test]
fn later_smaller_text_is_positioned_against_existing_baseline()
-> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        r#"<div><span style="font-size:30px">BIG</span><span style="font-size:10px">small</span></div>"#,
    )?;

    let big = find_text_box(&layout, "BIG")?;
    let small = find_text_box(&layout, "small")?;

    let big_baseline = baseline(big.rect(), fallback_metrics(30.0));
    let small_baseline = baseline(small.rect(), fallback_metrics(10.0));

    assert!((small_baseline - big_baseline).abs() <= EPSILON);
    assert!(small.rect().y() > big.rect().y());

    Ok(())
}

#[test]
fn mixed_font_line_height_contains_shared_text_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        r#"<div><span style="font-size:10px">small</span><span style="font-size:30px">BIG</span></div>"#,
    )?;

    let small = find_text_box(&layout, "small")?;
    let big = find_text_box(&layout, "BIG")?;

    if small.parent() != big.parent() {
        return Err(io::Error::other("mixed text fragments did not share one line").into());
    }

    let line_id = small
        .parent()
        .ok_or_else(|| io::Error::other("text fragment has no line parent"))?;
    let line = layout
        .get(line_id)
        .ok_or_else(|| io::Error::other("line parent is missing"))?;

    if !matches!(line.kind(), LayoutKind::Line) {
        return Err(io::Error::other("text parent is not a line box").into());
    }

    let small_metrics = fallback_metrics(10.0);
    let big_metrics = fallback_metrics(30.0);

    let expected_height = small_metrics.ascent().max(big_metrics.ascent())
        + small_metrics.descent().max(big_metrics.descent())
        + small_metrics.line_gap().max(big_metrics.line_gap());

    assert!((line.rect().height() - expected_height).abs() <= EPSILON);

    Ok(())
}
