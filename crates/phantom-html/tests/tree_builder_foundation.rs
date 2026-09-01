//! 2E-2 token-to-DOM tree-builder foundation tests.

use std::error::Error;

use phantom_dom::{Document, NodeId, NodeKind};
use phantom_html::{
    MAX_RETAINED_TEXT_BYTES, MAX_TEXT_NODE_BYTES,
    tree_builder::{
        MAX_TREE_DIAGNOSTICS, TreeBuilderError, TreePipelineError, TreeRecoveryCode,
        tokenize_and_build,
    },
};

fn element_id(document: &Document, tag_name: &str) -> Option<NodeId> {
    document.nodes().find_map(|node| match node.kind() {
        NodeKind::Element(element) if element.tag_name() == tag_name => Some(node.id()),
        NodeKind::Document | NodeKind::Element(_) | NodeKind::Text(_) | NodeKind::Comment(_) => {
            None
        }
    })
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

#[test]
fn explicit_html_head_and_body_are_preserved() -> Result<(), Box<dyn Error>> {
    let output = tokenize_and_build(
        "<!DOCTYPE html><html lang='pt'><head><title>Phantom</title></head><body><p>Olá</p></body></html>",
    )?;
    let document = output.document();

    let html = element_id(document, "html").ok_or_else(|| std::io::Error::other("html element"))?;
    let head = element_id(document, "head").ok_or_else(|| std::io::Error::other("head element"))?;
    let body = element_id(document, "body").ok_or_else(|| std::io::Error::other("body element"))?;

    assert_eq!(
        document.node(html).and_then(|node| match node.kind() {
            NodeKind::Element(element) => element.attribute("lang"),
            NodeKind::Document | NodeKind::Text(_) | NodeKind::Comment(_) => None,
        }),
        Some("pt")
    );
    assert_eq!(direct_child_tags(document, html), vec!["head", "body"]);
    assert_eq!(descendant_text(document, head), "Phantom");
    assert_eq!(descendant_text(document, body), "Olá");
    assert!(output.report().doctype_seen);
    assert!(!output.report().implicit_html);
    assert!(!output.report().implicit_head);
    assert!(!output.report().implicit_body);
    Ok(())
}

#[test]
fn missing_document_scaffold_is_created_deterministically() -> Result<(), Box<dyn Error>> {
    let output = tokenize_and_build("<p>Hello</p>")?;
    let document = output.document();

    let html = element_id(document, "html").ok_or_else(|| std::io::Error::other("html element"))?;
    let body = element_id(document, "body").ok_or_else(|| std::io::Error::other("body element"))?;

    assert_eq!(direct_child_tags(document, html), vec!["head", "body"]);
    assert_eq!(direct_child_tags(document, body), vec!["p"]);
    assert!(output.report().implicit_html);
    assert!(output.report().implicit_head);
    assert!(output.report().implicit_body);
    assert!(output.report().recoveries_total >= 3);
    Ok(())
}

#[test]
fn block_start_implicitly_closes_open_paragraph() -> Result<(), Box<dyn Error>> {
    let output = tokenize_and_build("<p>one<div>two</div>")?;
    let document = output.document();
    let body = element_id(document, "body").ok_or_else(|| std::io::Error::other("body element"))?;

    assert_eq!(direct_child_tags(document, body), vec!["p", "div"]);
    assert!(
        output
            .report()
            .recoveries
            .iter()
            .any(|recovery| { recovery.code == TreeRecoveryCode::ImplicitParagraphClose })
    );
    Ok(())
}

#[test]
fn second_paragraph_implicitly_closes_first() -> Result<(), Box<dyn Error>> {
    let output = tokenize_and_build("<p>one<p>two")?;
    let document = output.document();
    let body = element_id(document, "body").ok_or_else(|| std::io::Error::other("body element"))?;

    assert_eq!(direct_child_tags(document, body), vec!["p", "p"]);
    Ok(())
}

#[test]
fn misnested_end_tag_closes_descendants_and_records_recovery() -> Result<(), Box<dyn Error>> {
    let output = tokenize_and_build("<div><span>x</div><p>y</p>")?;
    let document = output.document();
    let body = element_id(document, "body").ok_or_else(|| std::io::Error::other("body element"))?;

    assert_eq!(direct_child_tags(document, body), vec!["div", "p"]);
    assert!(
        output
            .report()
            .recoveries
            .iter()
            .any(|recovery| { recovery.code == TreeRecoveryCode::MisnestedEndTag })
    );
    Ok(())
}

#[test]
fn void_elements_are_not_left_on_open_element_stack() -> Result<(), Box<dyn Error>> {
    let output = tokenize_and_build("<div>a<br>b<img src='x'>c</div><p>d</p>")?;
    let document = output.document();
    let body = element_id(document, "body").ok_or_else(|| std::io::Error::other("body element"))?;

    assert_eq!(direct_child_tags(document, body), vec!["div", "p"]);
    assert!(output.report().max_open_elements <= 4);
    Ok(())
}

#[test]
fn non_void_self_closing_flag_is_ignored_and_reported() -> Result<(), Box<dyn Error>> {
    let output = tokenize_and_build("<div/>inside</div>")?;

    assert!(
        output
            .report()
            .recoveries
            .iter()
            .any(|recovery| { recovery.code == TreeRecoveryCode::IgnoredSelfClosingFlag })
    );
    Ok(())
}

#[test]
fn tokenizer_diagnostics_flow_into_structural_report() -> Result<(), Box<dyn Error>> {
    let output = tokenize_and_build("<div A='first' a='second'>x</div>")?;

    assert_eq!(output.report().tokenizer_parse_errors, 1);
    assert_eq!(output.report().character_reference_candidates, 0);
    Ok(())
}

#[test]
fn character_references_remain_deferred_to_2e3() -> Result<(), Box<dyn Error>> {
    let output = tokenize_and_build("<p>A&amp;B</p>")?;
    let document = output.document();
    let paragraph =
        element_id(document, "p").ok_or_else(|| std::io::Error::other("paragraph element"))?;

    assert_eq!(descendant_text(document, paragraph), "A&amp;B");
    assert_eq!(output.report().character_reference_candidates, 1);
    Ok(())
}

#[test]
fn tree_builder_enforces_inherited_nesting_budget() {
    let source = format!("{}x{}", "<div>".repeat(300), "</div>".repeat(300));

    assert!(matches!(
        tokenize_and_build(&source),
        Err(TreePipelineError::TreeBuilder(
            TreeBuilderError::NestingTooDeep
        ))
    ));
}

#[test]
fn tree_builder_enforces_individual_text_budget() {
    let text = "x".repeat(MAX_TEXT_NODE_BYTES.saturating_add(1));
    let source = format!("<p>{text}</p>");

    assert!(matches!(
        tokenize_and_build(&source),
        Err(TreePipelineError::TreeBuilder(
            TreeBuilderError::TextNodeTooLarge
        ))
    ));
}

#[test]
fn tree_builder_enforces_aggregate_retained_text_budget() {
    let piece_size = (MAX_RETAINED_TEXT_BYTES / 4).saturating_add(1);
    let piece = "x".repeat(piece_size);
    let source = format!("<p>{piece}</p><p>{piece}</p><p>{piece}</p><p>{piece}</p>");

    assert!(matches!(
        tokenize_and_build(&source),
        Err(TreePipelineError::TreeBuilder(
            TreeBuilderError::RetainedTextTooLarge
        ))
    ));
}

#[test]
fn recovery_diagnostics_are_bounded() -> Result<(), Box<dyn Error>> {
    let source = "</missing>".repeat(MAX_TREE_DIAGNOSTICS.saturating_add(32));
    let output = tokenize_and_build(&source)?;

    assert_eq!(output.report().recoveries.len(), MAX_TREE_DIAGNOSTICS);
    assert!(output.report().recoveries_truncated);
    assert!(output.report().recoveries_total > MAX_TREE_DIAGNOSTICS);
    Ok(())
}

#[test]
fn empty_source_still_builds_a_stable_document_scaffold() -> Result<(), Box<dyn Error>> {
    let output = tokenize_and_build("")?;
    let document = output.document();

    let html = element_id(document, "html").ok_or_else(|| std::io::Error::other("html element"))?;
    assert_eq!(direct_child_tags(document, html), vec!["head", "body"]);
    assert_eq!(output.report().nodes_created, 4);
    Ok(())
}
