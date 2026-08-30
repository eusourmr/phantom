//! Security regression tests for the Phantom 2D-6 security gate.

use phantom_dom::{DomError, MAX_DOM_NODES};
use phantom_html::{
    HtmlError, MAX_ATTRIBUTE_BYTES_PER_ELEMENT, MAX_ATTRIBUTES_PER_ELEMENT, MAX_COMMENT_BYTES,
    MAX_HTML_NESTING_DEPTH, MAX_HTML_SOURCE_BYTES, MAX_RAW_START_TAG_BYTES,
    MAX_RETAINED_TEXT_BYTES, MAX_STYLE_BODY_BYTES, MAX_TEXT_NODE_BYTES, parse,
};

#[test]
fn rejects_source_above_document_admission_budget() {
    let source = "x".repeat(MAX_HTML_SOURCE_BYTES.saturating_add(1));
    assert!(matches!(parse(&source), Err(HtmlError::SourceTooLarge)));
}

#[test]
fn rejects_nesting_above_depth_budget() {
    let source = "<div>".repeat(MAX_HTML_NESTING_DEPTH.saturating_add(1));
    assert!(matches!(parse(&source), Err(HtmlError::NestingTooDeep)));
}

#[test]
fn rejects_attribute_fanout_above_budget() {
    let mut source = String::from("<div");
    for index in 0..=MAX_ATTRIBUTES_PER_ELEMENT {
        source.push_str(&format!(" a{index}=\"x\""));
    }
    source.push('>');

    assert!(matches!(parse(&source), Err(HtmlError::TooManyAttributes)));
}

#[test]
fn rejects_attribute_bytes_above_budget() {
    let value = "x".repeat(MAX_ATTRIBUTE_BYTES_PER_ELEMENT.saturating_add(1));
    let source = format!("<div data-x=\"{value}\">");
    assert!(matches!(
        parse(&source),
        Err(HtmlError::AttributeBytesExceeded)
    ));
}

#[test]
fn accepts_large_hydration_attribute_within_compatibility_budget() -> Result<(), HtmlError> {
    let value_len = 384 * 1024;
    let value = "x".repeat(value_len);
    let source = format!(r#"<header data-props="{value}"></header>"#);
    let document = parse(&source)?;
    let retained_len = document.nodes().find_map(|node| match node.kind() {
        phantom_dom::NodeKind::Element(element) if element.tag_name() == "header" => {
            element.attribute("data-props").map(str::len)
        }
        _ => None,
    });
    assert_eq!(retained_len, Some(value_len));
    Ok(())
}

#[test]
fn rejects_raw_start_tag_above_scan_budget() {
    let value = "x".repeat(MAX_RAW_START_TAG_BYTES.saturating_add(1));
    let source = format!(r#"<div data-x="{value}"></div>"#);
    assert!(matches!(parse(&source), Err(HtmlError::StartTagTooLarge)));
}

#[test]
fn rejects_single_text_node_above_budget() {
    let source = "x".repeat(MAX_TEXT_NODE_BYTES.saturating_add(1));
    assert!(matches!(parse(&source), Err(HtmlError::TextNodeTooLarge)));
}

#[test]
fn rejects_aggregate_retained_text_above_budget() {
    let chunk_len = MAX_TEXT_NODE_BYTES.saturating_mul(3) / 4;
    let chunk = "x".repeat(chunk_len);
    let chunk_count = (MAX_RETAINED_TEXT_BYTES / chunk_len).saturating_add(1);
    let mut source = String::new();

    for _ in 0..chunk_count {
        source.push_str("<p>");
        source.push_str(&chunk);
        source.push_str("</p>");
    }

    assert!(matches!(
        parse(&source),
        Err(HtmlError::RetainedTextTooLarge)
    ));
}

#[test]
fn rejects_comment_above_budget() {
    let comment = "x".repeat(MAX_COMMENT_BYTES.saturating_add(1));
    let source = format!("<!--{comment}-->");
    assert!(matches!(parse(&source), Err(HtmlError::CommentTooLarge)));
}

#[test]
fn rejects_unterminated_comment_above_budget() {
    let comment = "x".repeat(MAX_COMMENT_BYTES.saturating_add(1));
    let source = format!("<!--{comment}");
    assert!(matches!(parse(&source), Err(HtmlError::CommentTooLarge)));
}

#[test]
fn rejects_style_body_above_budget() {
    let css = "x".repeat(MAX_STYLE_BODY_BYTES.saturating_add(1));
    let source = format!("<style>{css}</style>");
    assert!(matches!(parse(&source), Err(HtmlError::StyleBodyTooLarge)));
}

#[test]
fn dom_node_budget_is_enforced_through_html_parser() {
    let source = "<br>".repeat(MAX_DOM_NODES);
    assert!(matches!(
        parse(&source),
        Err(HtmlError::Dom(DomError::NodeLimitExceeded))
    ));
}

#[test]
fn mixed_case_raw_text_close_preserves_following_document() -> Result<(), HtmlError> {
    let document = parse("<style>p{color:red}</StYlE><p>after</p>")?;
    assert!(document.nodes().any(|node| {
        matches!(node.kind(), phantom_dom::NodeKind::Text(text) if text == "after")
    }));
    Ok(())
}
