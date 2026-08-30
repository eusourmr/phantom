//! Security regression tests for the Phantom 2D-6 security gate.

use phantom_css::compute_styles;
use phantom_html::{HtmlError, parse};
use phantom_layout::build_layout_snapshot;

#[test]
fn non_finite_viewport_is_sanitized_before_layout() -> Result<(), Box<dyn std::error::Error>> {
    let document = parse("<div>Phantom</div>")?;
    let styles = compute_styles(&document);
    let layout = build_layout_snapshot(&document, &styles, f32::NAN)?;

    assert_eq!(layout.viewport_width(), 1024.0);
    assert!(layout.content_height().is_finite());
    Ok(())
}

#[test]
fn maximum_parser_depth_remains_layout_safe() -> Result<(), Box<dyn std::error::Error>> {
    let source = format!("{}x{}", "<div>".repeat(256), "</div>".repeat(256));
    let document = parse(&source)?;
    let styles = compute_styles(&document);
    let layout = build_layout_snapshot(&document, &styles, 800.0)?;

    assert!(layout.content_height().is_finite());
    assert!(layout.viewport_width().is_finite());
    assert!(layout.len() <= document.len().saturating_add(512));
    Ok(())
}

#[test]
fn parser_rejects_depth_that_would_expand_layout_recursion() {
    let source = "<div>".repeat(257);
    assert!(matches!(parse(&source), Err(HtmlError::NestingTooDeep)));
}
