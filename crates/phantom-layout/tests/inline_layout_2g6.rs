//! Phantom 2G-6 line-height and inline line-box regression corpus.

use std::io;

use phantom_css::compute_styles;
use phantom_html::parse;
use phantom_layout::{LayoutKind, LayoutSnapshot, build_layout_snapshot};

fn layout_for(source: &str) -> Result<LayoutSnapshot, Box<dyn std::error::Error>> {
    let document = parse(source)?;
    let styles = compute_styles(&document);
    Ok(build_layout_snapshot(&document, &styles, 800.0)?)
}

fn first_line_height(layout: &LayoutSnapshot) -> Result<f32, io::Error> {
    layout
        .boxes()
        .iter()
        .find(|layout_box| matches!(layout_box.kind(), LayoutKind::Line))
        .map(|layout_box| layout_box.rect().height())
        .ok_or_else(|| io::Error::other("line box missing"))
}

#[test]
fn explicit_line_height_expands_line_box() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(r#"<div style="font-size:16px;line-height:48px">Phantom</div>"#)?;

    assert!(first_line_height(&layout)? >= 47.95);
    Ok(())
}

#[test]
fn unitless_line_height_uses_current_font_size() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(r#"<div style="font-size:20px;line-height:2">Engine</div>"#)?;

    assert!(first_line_height(&layout)? >= 39.95);
    Ok(())
}

#[test]
fn inherited_unitless_line_height_tracks_child_font_size() -> Result<(), Box<dyn std::error::Error>>
{
    let layout =
        layout_for(r#"<div style="line-height:2"><span style="font-size:24px">Text</span></div>"#)?;

    assert!(first_line_height(&layout)? >= 47.95);
    Ok(())
}

#[test]
fn smaller_requested_line_height_does_not_shrink_natural_metrics()
-> Result<(), Box<dyn std::error::Error>> {
    let natural = layout_for(r#"<div style="font-size:20px">Text</div>"#)?;
    let compact = layout_for(r#"<div style="font-size:20px;line-height:1px">Text</div>"#)?;

    assert!(first_line_height(&compact)? + 0.05 >= first_line_height(&natural)?);
    Ok(())
}
