//! Cycle-2E curated HTML compatibility and production-parser cutover gate.
//!
//! Cases are behavior-focused and derived from WHATWG/WPT parsing families.
//! No upstream test-suite source is vendored here. The external compatibility
//! oracle is pinned for this cycle to web-platform-tests/wpt commit
//! `344f3c3b6508203343551f0694a7a07b1d58c1b5` (2026-09-14).

use std::error::Error;

use phantom_dom::{Document, NodeId, NodeKind};
use phantom_html::{
    HtmlError, parse, parse_with_report,
    tree_builder::{HtmlParserGeneration, TreeRecoveryCode, tokenize_and_build},
};

fn element_id(document: &Document, tag_name: &str) -> Option<NodeId> {
    document.nodes().find_map(|node| match node.kind() {
        NodeKind::Element(element) if element.tag_name() == tag_name => Some(node.id()),
        NodeKind::Document | NodeKind::Element(_) | NodeKind::Text(_) | NodeKind::Comment(_) => {
            None
        }
    })
}

fn descendant_text(document: &Document, parent: NodeId) -> String {
    let Some(parent_node) = document.node(parent) else {
        return String::new();
    };

    let mut text = String::new();
    let mut stack = parent_node
        .children()
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>();

    while let Some(node_id) = stack.pop() {
        let Some(node) = document.node(node_id) else {
            continue;
        };

        if let NodeKind::Text(value) = node.kind() {
            text.push_str(value);
        }

        stack.extend(node.children().iter().rev().copied());
    }

    text
}

fn direct_child_tags(document: &Document, parent: NodeId) -> Vec<String> {
    let Some(parent_node) = document.node(parent) else {
        return Vec::new();
    };

    parent_node
        .children()
        .iter()
        .filter_map(|child| {
            document.node(*child).and_then(|node| match node.kind() {
                NodeKind::Element(element) => Some(element.tag_name().to_owned()),
                NodeKind::Document | NodeKind::Text(_) | NodeKind::Comment(_) => None,
            })
        })
        .collect()
}

fn fingerprint(document: &Document) -> Vec<String> {
    document
        .nodes()
        .map(|node| {
            let kind = match node.kind() {
                NodeKind::Document => "#document".to_owned(),
                NodeKind::Element(element) => {
                    let attributes = element
                        .attributes()
                        .iter()
                        .map(|(name, value)| format!(" {name}={value:?}"))
                        .collect::<String>();
                    format!("<{}{attributes}>", element.tag_name())
                }
                NodeKind::Text(text) => format!("#text:{text:?}"),
                NodeKind::Comment(comment) => format!("#comment:{comment:?}"),
            };

            let parent = node
                .parent()
                .map_or_else(|| "root".to_owned(), |parent| parent.as_u64().to_string());
            format!("{}:{parent}:{kind}", node.id().as_u64())
        })
        .collect()
}

#[test]
fn production_parse_matches_direct_pipeline() -> Result<(), Box<dyn Error>> {
    let source = "<!doctype html><title>A &amp; B</title><ul><li>one<li>two</ul>";
    let production = parse(source)?;
    let direct = tokenize_and_build(source)?;

    assert_eq!(fingerprint(&production), fingerprint(direct.document()));
    Ok(())
}

#[test]
fn production_parse_and_report_are_one_authoritative_result() -> Result<(), Box<dyn Error>> {
    let (document, report) = parse_with_report("<p>A&amp;B<p>C")?;
    let paragraph =
        element_id(&document, "p").ok_or_else(|| std::io::Error::other("paragraph element"))?;

    assert!(descendant_text(&document, paragraph).contains("A&B"));
    assert_eq!(report.parser_generation, HtmlParserGeneration::Native2E);
    assert_eq!(report.nodes_created, document.len());
    assert_eq!(report.character_references_resolved, 1);
    assert!(report.recoveries_total >= 1);
    Ok(())
}

#[test]
fn structure_family_synthesizes_document_scaffolding() -> Result<(), Box<dyn Error>> {
    let document = parse("<main><h1>Phantom</h1></main>")?;

    assert!(element_id(&document, "html").is_some());
    assert!(element_id(&document, "head").is_some());
    assert!(element_id(&document, "body").is_some());
    assert!(element_id(&document, "main").is_some());
    Ok(())
}

#[test]
fn references_family_covers_named_numeric_data_and_attributes() -> Result<(), Box<dyn Error>> {
    let (document, report) = parse_with_report(
        "<p>&lt;A&amp;B&nbsp;&copy;&#65;&#x42;&#0;</p>\
         <a href='?x=1&amp;y=2' data-raw='&ampx'>link</a>",
    )?;

    let paragraph =
        element_id(&document, "p").ok_or_else(|| std::io::Error::other("paragraph element"))?;

    let href = document.nodes().find_map(|node| match node.kind() {
        NodeKind::Element(element) if element.tag_name() == "a" => element.attribute("href"),
        _ => None,
    });
    let raw = document.nodes().find_map(|node| match node.kind() {
        NodeKind::Element(element) if element.tag_name() == "a" => element.attribute("data-raw"),
        _ => None,
    });

    assert_eq!(
        descendant_text(&document, paragraph),
        "<A&B\u{00a0}©AB\u{fffd}"
    );
    assert_eq!(href, Some("?x=1&y=2"));
    assert_eq!(raw, Some("&ampx"));
    assert_eq!(report.character_references_resolved, 8);
    assert_eq!(report.character_references_preserved, 1);
    Ok(())
}

#[test]
fn references_family_distinguishes_missing_semicolon_by_context() -> Result<(), Box<dyn Error>> {
    let document = parse("<p>&ampx</p><a data-x='&ampx'>x</a>")?;
    let paragraph =
        element_id(&document, "p").ok_or_else(|| std::io::Error::other("paragraph element"))?;
    let attribute = document.nodes().find_map(|node| match node.kind() {
        NodeKind::Element(element) if element.tag_name() == "a" => element.attribute("data-x"),
        _ => None,
    });

    assert_eq!(descendant_text(&document, paragraph), "&x");
    assert_eq!(attribute, Some("&ampx"));
    Ok(())
}

#[test]
fn text_mode_family_rcdata_preserves_markup_and_decodes_references() -> Result<(), Box<dyn Error>> {
    let (document, report) =
        parse_with_report("<title>A &amp; B <b>literal</b></title><p>Visible</p>")?;
    let title =
        element_id(&document, "title").ok_or_else(|| std::io::Error::other("title element"))?;

    assert_eq!(descendant_text(&document, title), "A & B <b>literal</b>");
    assert_eq!(report.rcdata_sections, 1);
    Ok(())
}

#[test]
fn text_mode_family_rawtext_preserves_css_source() -> Result<(), Box<dyn Error>> {
    let (document, report) =
        parse_with_report(r#"<style>p::before{content:"<b>&amp;"}</StYlE><p>Visible</p>"#)?;
    let style =
        element_id(&document, "style").ok_or_else(|| std::io::Error::other("style element"))?;

    assert_eq!(
        descendant_text(&document, style),
        r#"p::before{content:"<b>&amp;"}"#
    );
    assert_eq!(report.rawtext_sections, 1);
    Ok(())
}

#[test]
fn text_mode_family_script_data_is_retained_for_phantom_js() -> Result<(), Box<dyn Error>> {
    let (document, report) =
        parse_with_report(r#"<script>if (a < b) { s = "<div>"; }</script><p>Visible</p>"#)?;
    let script =
        element_id(&document, "script").ok_or_else(|| std::io::Error::other("script element"))?;

    assert_eq!(
        descendant_text(&document, script),
        r#"if (a < b) { s = "<div>"; }"#
    );
    assert_eq!(report.script_data_sections, 1);
    Ok(())
}

#[test]
fn recovery_family_repeated_list_items_are_siblings() -> Result<(), Box<dyn Error>> {
    let (document, report) = parse_with_report("<ul><li>one<li>two</ul>")?;
    let list = element_id(&document, "ul").ok_or_else(|| std::io::Error::other("ul element"))?;

    assert_eq!(direct_child_tags(&document, list), vec!["li", "li"]);
    assert!(
        report
            .recoveries
            .iter()
            .any(|recovery| { recovery.code == TreeRecoveryCode::ImplicitListItemClose })
    );
    Ok(())
}

#[test]
fn recovery_family_definition_items_and_headings_are_deterministic() -> Result<(), Box<dyn Error>> {
    let (definition, definition_report) = parse_with_report("<dl><dt>A<dd>B<dt>C</dl>")?;
    let list = element_id(&definition, "dl").ok_or_else(|| std::io::Error::other("dl element"))?;
    assert_eq!(direct_child_tags(&definition, list), vec!["dt", "dd", "dt"]);
    assert!(
        definition_report
            .recoveries
            .iter()
            .any(|recovery| { recovery.code == TreeRecoveryCode::ImplicitDefinitionItemClose })
    );

    let (headings, heading_report) = parse_with_report("<h1>A<h2>B</h2>")?;
    let body =
        element_id(&headings, "body").ok_or_else(|| std::io::Error::other("body element"))?;
    assert_eq!(direct_child_tags(&headings, body), vec!["h1", "h2"]);
    assert!(
        heading_report
            .recoveries
            .iter()
            .any(|recovery| { recovery.code == TreeRecoveryCode::ImplicitHeadingClose })
    );
    Ok(())
}

#[test]
fn recovery_family_stray_paragraph_end_tag_synthesizes_paragraph() -> Result<(), Box<dyn Error>> {
    let (document, report) = parse_with_report("<div>before</p>after</div>")?;

    assert!(element_id(&document, "p").is_some());
    assert!(
        report
            .recoveries
            .iter()
            .any(|recovery| { recovery.code == TreeRecoveryCode::StrayParagraphEndTag })
    );
    Ok(())
}

#[test]
fn cutover_preserves_public_security_error_surface() {
    let oversized = "x".repeat(phantom_html::MAX_HTML_SOURCE_BYTES.saturating_add(1));
    assert!(matches!(parse(&oversized), Err(HtmlError::SourceTooLarge)));
}
