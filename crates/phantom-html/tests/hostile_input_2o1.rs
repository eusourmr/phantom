//! Phantom 2O-1 — deterministic hostile HTML admission and recovery regression gate.
//! This is a test-only addition: production parser budgets remain authoritative.

use phantom_dom::NodeKind;
use phantom_html::{
    HtmlError, MAX_ATTRIBUTES_PER_ELEMENT, MAX_COMMENT_BYTES, MAX_HTML_NESTING_DEPTH,
    MAX_HTML_SOURCE_BYTES, MAX_RETAINED_TEXT_BYTES, MAX_STYLE_BODY_BYTES, MAX_TEXT_NODE_BYTES,
    parse,
};

fn contains_text(document: &phantom_dom::Document, expected: &str) -> bool {
    document
        .nodes()
        .any(|node| matches!(node.kind(), NodeKind::Text(text) if text == expected))
}

#[test]
fn malformed_neighbors_do_not_poison_later_content() -> Result<(), HtmlError> {
    for source in [
        "<div><p>before</div><p>survivor</p>",
        "<p><!-- incomplete? --><span>survivor</span>",
        "<style>p{color:red}</StYlE><p>survivor</p>",
        "<p data-x='>'>survivor</p>",
        "<p>\0</p><p>survivor</p>",
    ] {
        let document = parse(source)?;
        assert!(contains_text(&document, "survivor"), "input: {source:?}");
    }
    Ok(())
}

#[test]
fn hostile_inputs_have_stable_admission_decisions() {
    let corpus = [
        "",
        "<",
        "<!--",
        "<!DOCTYPE",
        "<div",
        "</div>",
        "<script>var x = '<div>';</script><p>ok</p>",
        "<style>p{color:red}</style><p>ok</p>",
        "<p title='unterminated>ok",
        "<div><span></div></span>",
        "\0\0\0<em>ok</em>",
    ];

    for source in corpus {
        let first = parse(source);
        let second = parse(source);
        assert_eq!(
            first.as_ref().map(|_| ()).map_err(std::mem::discriminant),
            second.as_ref().map(|_| ()).map_err(std::mem::discriminant),
            "admission changed for {source:?}"
        );
    }
}

#[test]
fn oversized_source_is_rejected_before_partial_document_commit() {
    let source = "x".repeat(MAX_HTML_SOURCE_BYTES.saturating_add(1));
    assert!(matches!(parse(&source), Err(HtmlError::SourceTooLarge)));
    assert!(parse("<p>survivor</p>").is_ok());
}

#[test]
fn nesting_limit_rejection_does_not_poison_next_parse() -> Result<(), HtmlError> {
    let hostile = "<div>".repeat(MAX_HTML_NESTING_DEPTH.saturating_add(1));
    assert!(matches!(parse(&hostile), Err(HtmlError::NestingTooDeep)));
    let recovered = parse("<p>survivor</p>")?;
    assert!(contains_text(&recovered, "survivor"));
    Ok(())
}

#[test]
fn excessive_attributes_do_not_poison_next_parse() -> Result<(), HtmlError> {
    let mut hostile = String::from("<div");
    for index in 0..=MAX_ATTRIBUTES_PER_ELEMENT {
        hostile.push_str(&format!(" a{index}='x'"));
    }
    hostile.push('>');
    assert!(matches!(parse(&hostile), Err(HtmlError::TooManyAttributes)));
    assert!(contains_text(&parse("<p>survivor</p>")?, "survivor"));
    Ok(())
}

#[test]
fn oversized_comment_style_and_text_are_independently_rejected() {
    let cases = [
        (
            format!("<!--{}-->", "x".repeat(MAX_COMMENT_BYTES.saturating_add(1))),
            "comment",
        ),
        (
            format!(
                "<style>{}</style>",
                "x".repeat(MAX_STYLE_BODY_BYTES.saturating_add(1))
            ),
            "style",
        ),
        ("x".repeat(MAX_TEXT_NODE_BYTES.saturating_add(1)), "text"),
    ];
    for (input, label) in cases {
        assert!(parse(&input).is_err(), "{label} was admitted");
        assert!(
            parse("<p>still works</p>").is_ok(),
            "{label} poisoned parser"
        );
    }
}

#[test]
fn total_retained_text_budget_is_not_bypassed_by_sibling_nodes() {
    let chunk_len = MAX_TEXT_NODE_BYTES.saturating_mul(3) / 4;
    let chunk = "x".repeat(chunk_len);
    let count = MAX_RETAINED_TEXT_BYTES / chunk_len + 1;
    let mut input = String::new();
    for _ in 0..count {
        input.push_str("<p>");
        input.push_str(&chunk);
        input.push_str("</p>");
    }
    assert!(matches!(
        parse(&input),
        Err(HtmlError::RetainedTextTooLarge)
    ));
}
