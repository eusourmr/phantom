//! Cycle-2E authoritative HTML parser observability integration tests.

use std::error::Error;

use phantom_dom::NodeKind;
use phantom_engine::Engine;
use phantom_html::tree_builder::HtmlParserGeneration;

#[test]
fn engine_exposes_report_from_authoritative_rendering_parse() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    engine.load_html("<p>A&amp;B<p>C")?;

    let report = engine
        .html_build_report()
        .ok_or_else(|| std::io::Error::other("cycle-2E HTML build report"))?;

    let contains_resolved_text = engine
        .document()
        .nodes()
        .any(|node| matches!(node.kind(), NodeKind::Text(text) if text == "A&B"));

    assert!(contains_resolved_text);
    assert_eq!(report.parser_generation, HtmlParserGeneration::Native2E);
    assert_eq!(report.character_references_resolved, 1);
    assert_eq!(report.character_references_preserved, 0);
    assert!(report.recoveries_total >= 1);
    assert_eq!(report.nodes_created, engine.document().len());
    Ok(())
}

#[test]
fn authoritative_parser_report_is_replaced_with_each_successful_document()
-> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();

    engine.load_html("<title>A &amp; B</title>")?;
    let first = engine
        .html_build_report()
        .ok_or_else(|| std::io::Error::other("first HTML build report"))?;
    assert_eq!(first.character_references_resolved, 1);
    assert_eq!(first.rcdata_sections, 1);

    engine.load_html("<p>Plain</p>")?;
    let second = engine
        .html_build_report()
        .ok_or_else(|| std::io::Error::other("second HTML build report"))?;
    assert_eq!(second.character_references_resolved, 0);
    assert_eq!(second.rcdata_sections, 0);
    assert_eq!(second.nodes_created, engine.document().len());
    Ok(())
}
