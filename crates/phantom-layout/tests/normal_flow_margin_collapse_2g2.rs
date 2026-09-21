//! Phantom 2G-2 sibling vertical-margin collapsing validation.
//!
//! This slice intentionally covers adjoining in-flow block-level siblings.
//! Parent/child collapsing, empty-block collapse-through and clearance remain
//! later Layout compatibility work.

use std::io;

use phantom_dom::{Document, NodeId, NodeKind};
use phantom_layout::{LayoutKind, LayoutSnapshot, Rect, build_layout_snapshot};

fn layout(
    source: &str,
    viewport: f32,
) -> Result<(Document, LayoutSnapshot), Box<dyn std::error::Error>> {
    let document = phantom_html::parse(source)?;
    let styles = phantom_css::compute_styles(&document);
    let snapshot = build_layout_snapshot(&document, &styles, viewport)?;
    Ok((document, snapshot))
}

fn node_by_id(document: &Document, id: &str) -> Option<NodeId> {
    document.nodes().find_map(|node| {
        let NodeKind::Element(element) = node.kind() else {
            return None;
        };

        (element.attribute("id") == Some(id)).then_some(node.id())
    })
}

fn rect_by_id(document: &Document, snapshot: &LayoutSnapshot, id: &str) -> Option<Rect> {
    let node_id = node_by_id(document, id)?;

    snapshot.boxes().iter().find_map(|layout_box| {
        if layout_box.source_node() != node_id {
            return None;
        }

        match layout_box.kind() {
            LayoutKind::Block
            | LayoutKind::Flex
            | LayoutKind::Image { .. }
            | LayoutKind::Control { .. } => Some(layout_box.rect()),
            LayoutKind::Root | LayoutKind::Line | LayoutKind::Text { .. } => None,
        }
    })
}

fn required_rect(
    document: &Document,
    snapshot: &LayoutSnapshot,
    id: &str,
) -> Result<Rect, Box<dyn std::error::Error>> {
    rect_by_id(document, snapshot, id)
        .ok_or_else(|| io::Error::other(format!("layout geometry missing for #{id}")).into())
}

fn vertical_gap(first: Rect, second: Rect) -> f32 {
    second.y() - (first.y() + first.height())
}

#[test]
fn positive_sibling_margins_collapse_to_larger_margin() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="a" style="height:20px;margin-bottom:20px"></div><div id="b" style="height:20px;margin-top:30px"></div></body>"#,
        800.0,
    )?;

    let first = required_rect(&document, &snapshot, "a")?;
    let second = required_rect(&document, &snapshot, "b")?;

    assert!((vertical_gap(first, second) - 30.0).abs() < 0.1);
    Ok(())
}

#[test]
fn positive_and_negative_sibling_margins_are_combined() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="a" style="height:20px;margin-bottom:30px"></div><div id="b" style="height:20px;margin-top:-10px"></div></body>"#,
        800.0,
    )?;

    let first = required_rect(&document, &snapshot, "a")?;
    let second = required_rect(&document, &snapshot, "b")?;

    assert!((vertical_gap(first, second) - 20.0).abs() < 0.1);
    Ok(())
}

#[test]
fn negative_sibling_margins_use_the_larger_absolute_value() -> Result<(), Box<dyn std::error::Error>>
{
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="a" style="height:40px;margin-bottom:-20px"></div><div id="b" style="height:20px;margin-top:-30px"></div></body>"#,
        800.0,
    )?;

    let first = required_rect(&document, &snapshot, "a")?;
    let second = required_rect(&document, &snapshot, "b")?;

    assert!((vertical_gap(first, second) + 30.0).abs() < 0.1);
    Ok(())
}

#[test]
fn whitespace_and_comments_do_not_break_sibling_collapse() -> Result<(), Box<dyn std::error::Error>>
{
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="a" style="height:20px;margin-bottom:12px"></div>
          <!-- transparent to normal-flow margin adjacency -->

          <div id="b" style="height:20px;margin-top:28px"></div>
        </body>
        "#,
        800.0,
    )?;

    let first = required_rect(&document, &snapshot, "a")?;
    let second = required_rect(&document, &snapshot, "b")?;

    assert!((vertical_gap(first, second) - 28.0).abs() < 0.1);
    Ok(())
}

#[test]
fn display_none_sibling_does_not_break_collapse() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="a" style="height:20px;margin-bottom:16px"></div><div style="display:none;height:200px"></div><div id="b" style="height:20px;margin-top:24px"></div></body>"#,
        800.0,
    )?;

    let first = required_rect(&document, &snapshot, "a")?;
    let second = required_rect(&document, &snapshot, "b")?;

    assert!((vertical_gap(first, second) - 24.0).abs() < 0.1);
    Ok(())
}

#[test]
fn block_replaced_image_participates_in_sibling_collapse() -> Result<(), Box<dyn std::error::Error>>
{
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="a" style="height:20px;margin-bottom:10px"></div><img id="b" src="photo.png" style="display:block;width:80px;height:40px;margin-top:26px" alt="Photo"></body>"#,
        800.0,
    )?;

    let first = required_rect(&document, &snapshot, "a")?;
    let second = required_rect(&document, &snapshot, "b")?;

    assert!((vertical_gap(first, second) - 26.0).abs() < 0.1);
    Ok(())
}

#[test]
fn explicit_line_break_prevents_sibling_margin_collapse() -> Result<(), Box<dyn std::error::Error>>
{
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="a" style="height:20px;margin-bottom:20px"></div><br><div id="b" style="height:20px;margin-top:30px"></div></body>"#,
        800.0,
    )?;

    let first = required_rect(&document, &snapshot, "a")?;
    let second = required_rect(&document, &snapshot, "b")?;

    assert!(vertical_gap(first, second) > 50.0);
    Ok(())
}
