//! Phantom 2G-7 Inline Layout III white-space regression corpus.

use phantom_css::compute_styles;
use phantom_html::parse;
use phantom_layout::{LayoutSnapshot, build_layout_snapshot};

fn layout_for(source: &str) -> Result<LayoutSnapshot, Box<dyn std::error::Error>> {
    let document = parse(source)?;
    let styles = compute_styles(&document);
    Ok(build_layout_snapshot(&document, &styles, 800.0)?)
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
fn normal_collapses_repeated_spaces() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(r#"<div style="white-space: normal">A  B</div>"#)?;

    assert_eq!(rendered_text(&layout), "A B");
    Ok(())
}

#[test]
fn pre_preserves_repeated_spaces_and_line_breaks() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for("<div style=\"white-space: pre\">A  B\nC</div>")?;

    assert_eq!(rendered_text(&layout), "A  BC");
    assert!(layout.line_count() >= 2);
    Ok(())
}

#[test]
fn inline_pre_override_preserves_its_local_spaces() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(r#"<div>A<span style="white-space: pre">  B</span></div>"#)?;

    assert_eq!(rendered_text(&layout), "A  B");
    Ok(())
}

#[test]
fn inline_normal_can_override_pre_parent() -> Result<(), Box<dyn std::error::Error>> {
    let layout = layout_for(r#"<pre>A<span style="white-space: normal">  B</span></pre>"#)?;

    assert_eq!(rendered_text(&layout), "A B");
    Ok(())
}

#[test]
fn explicit_line_height_is_used_for_preserved_line_breaks() -> Result<(), Box<dyn std::error::Error>>
{
    let layout = layout_for("<div style=\"white-space: pre; line-height: 48px\">A\n\nB</div>")?;

    assert!(layout.line_count() >= 3);
    assert!(layout.content_height() >= 143.0);
    Ok(())
}
