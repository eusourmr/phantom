//! Phantom 2G-9 Inline Layout IV white-space II regression corpus.

use phantom_css::compute_styles;
use phantom_html::parse;
use phantom_layout::{LayoutSnapshot, build_layout_snapshot};

fn layout_for(
    source: &str,
    viewport_width: f32,
) -> Result<LayoutSnapshot, Box<dyn std::error::Error>> {
    let document = parse(source)?;
    let styles = compute_styles(&document);
    Ok(build_layout_snapshot(&document, &styles, viewport_width)?)
}

fn rendered_text(layout: &LayoutSnapshot) -> String {
    let mut output = String::new();

    for layout_box in layout.boxes() {
        if let Some(text) = layout.text_for(layout_box) {
            output.push_str(text);
        }
    }

    output
}

#[test]
fn nowrap_collapses_spaces_without_automatic_wrap() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        r#"<div style="white-space: nowrap">Alpha  Beta Gamma</div>"#,
        72.0,
    )?;

    assert_eq!(rendered_text(&layout), "Alpha Beta Gamma");
    assert_eq!(layout.line_count(), 1);
    Ok(())
}

#[test]
fn nowrap_suppresses_wrap_before_inline_controls() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        r#"<div style="white-space: nowrap">Alpha <input style="width: 120px"> Beta</div>"#,
        72.0,
    )?;

    assert_eq!(layout.line_count(), 1);
    Ok(())
}

#[test]
fn pre_wrap_preserves_spaces_line_breaks_and_allows_wrap() -> Result<(), Box<dyn std::error::Error>>
{
    let layout = layout_for(
        "<div style=\"white-space: pre-wrap\">Alpha  Beta\nGamma</div>",
        72.0,
    )?;

    assert_eq!(rendered_text(&layout), "Alpha  BetaGamma");
    assert!(layout.line_count() >= 3);
    Ok(())
}

#[test]
fn pre_line_collapses_spaces_but_preserves_explicit_line_breaks()
-> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        "<div style=\"white-space: pre-line\">Alpha   Beta\nGamma</div>",
        800.0,
    )?;

    assert_eq!(rendered_text(&layout), "Alpha BetaGamma");
    assert!(layout.line_count() >= 2);
    Ok(())
}

#[test]
fn pre_line_preserved_break_uses_inherited_line_height() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        "<div style=\"white-space: nowrap\"><span style=\"white-space: pre-line; line-height: 48px\">A  B\nC</span></div>",
        800.0,
    )?;

    assert_eq!(rendered_text(&layout), "A BC");
    assert!(layout.line_count() >= 2);
    assert!(layout.content_height() >= 95.9);
    Ok(())
}

#[test]
fn inline_nowrap_override_does_not_force_parent_to_preserve_spaces()
-> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(
        r#"<div style="white-space: normal">A <span style="white-space: nowrap">  B   C</span> D</div>"#,
        800.0,
    )?;

    assert_eq!(rendered_text(&layout), "A B C D");
    assert_eq!(layout.line_count(), 1);
    Ok(())
}
