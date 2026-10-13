//! Phantom 2G-3 parent/child vertical-margin collapsing validation.
//!
//! This slice intentionally covers safe normal-flow parent/first-child and
//! parent/last-child cases. Root/body propagation, empty-block collapse-through,
//! clearance and formatting-context edge cases remain later Layout work.

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
fn parent_and_first_child_top_margins_collapse() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="before" style="height:10px"></div><div id="parent" style="margin-top:10px"><div id="child" style="height:20px;margin-top:30px"></div></div></body>"#,
        800.0,
    )?;

    let before = required_rect(&document, &snapshot, "before")?;
    let parent = required_rect(&document, &snapshot, "parent")?;
    let child = required_rect(&document, &snapshot, "child")?;

    assert!((vertical_gap(before, parent) - 30.0).abs() < 0.1);
    assert!((parent.y() - child.y()).abs() < 0.1);
    Ok(())
}

#[test]
fn mixed_parent_first_child_top_margins_combine() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="before" style="height:10px"></div><div id="parent" style="margin-top:30px"><div id="child" style="height:20px;margin-top:-10px"></div></div></body>"#,
        800.0,
    )?;

    let before = required_rect(&document, &snapshot, "before")?;
    let parent = required_rect(&document, &snapshot, "parent")?;
    let child = required_rect(&document, &snapshot, "child")?;

    assert!((vertical_gap(before, parent) - 20.0).abs() < 0.1);
    assert!((parent.y() - child.y()).abs() < 0.1);
    Ok(())
}

#[test]
fn top_padding_blocks_parent_first_child_collapse() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="before" style="height:10px"></div><div id="parent" style="margin-top:10px;padding-top:5px"><div id="child" style="height:20px;margin-top:30px"></div></div></body>"#,
        800.0,
    )?;

    let before = required_rect(&document, &snapshot, "before")?;
    let parent = required_rect(&document, &snapshot, "parent")?;
    let child = required_rect(&document, &snapshot, "child")?;

    assert!((vertical_gap(before, parent) - 10.0).abs() < 0.1);
    assert!((child.y() - parent.y() - 35.0).abs() < 0.1);
    Ok(())
}

#[test]
fn parent_and_last_child_bottom_margins_collapse() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="parent" style="margin-bottom:10px"><div id="child" style="height:20px;margin-bottom:30px"></div></div><div id="after" style="height:10px"></div></body>"#,
        800.0,
    )?;

    let parent = required_rect(&document, &snapshot, "parent")?;
    let child = required_rect(&document, &snapshot, "child")?;
    let after = required_rect(&document, &snapshot, "after")?;

    assert!(((parent.y() + parent.height()) - (child.y() + child.height())).abs() < 0.1);
    assert!((vertical_gap(parent, after) - 30.0).abs() < 0.1);
    Ok(())
}

#[test]
fn bottom_padding_blocks_parent_last_child_collapse() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="parent" style="margin-bottom:10px;padding-bottom:5px"><div id="child" style="height:20px;margin-bottom:30px"></div></div><div id="after" style="height:10px"></div></body>"#,
        800.0,
    )?;

    let parent = required_rect(&document, &snapshot, "parent")?;
    let child = required_rect(&document, &snapshot, "child")?;
    let after = required_rect(&document, &snapshot, "after")?;

    assert!(((parent.y() + parent.height()) - (child.y() + child.height()) - 35.0).abs() < 0.1);
    assert!((vertical_gap(parent, after) - 10.0).abs() < 0.1);
    Ok(())
}

#[test]
fn explicit_parent_height_blocks_bottom_collapse() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="parent" style="height:50px;margin-bottom:10px"><div id="child" style="height:20px;margin-bottom:30px"></div></div><div id="after" style="height:10px"></div></body>"#,
        800.0,
    )?;

    let parent = required_rect(&document, &snapshot, "parent")?;
    let after = required_rect(&document, &snapshot, "after")?;

    assert!((parent.height() - 50.0).abs() < 0.1);
    assert!((vertical_gap(parent, after) - 10.0).abs() < 0.1);
    Ok(())
}

#[test]
fn nested_first_child_chain_collapses_outward() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="before" style="height:10px"></div><div id="outer" style="margin-top:10px"><div id="middle" style="margin-top:20px"><div id="inner" style="height:20px;margin-top:30px"></div></div></div></body>"#,
        800.0,
    )?;

    let before = required_rect(&document, &snapshot, "before")?;
    let outer = required_rect(&document, &snapshot, "outer")?;
    let middle = required_rect(&document, &snapshot, "middle")?;
    let inner = required_rect(&document, &snapshot, "inner")?;

    assert!((vertical_gap(before, outer) - 30.0).abs() < 0.1);
    assert!((outer.y() - middle.y()).abs() < 0.1);
    assert!((middle.y() - inner.y()).abs() < 0.1);
    Ok(())
}

#[test]
fn block_image_can_be_first_collapsing_child() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="before" style="height:10px"></div><div id="parent" style="margin-top:8px"><img id="child" src="photo.png" alt="Photo" style="display:block;width:80px;height:40px;margin-top:24px"></div></body>"#,
        800.0,
    )?;

    let before = required_rect(&document, &snapshot, "before")?;
    let parent = required_rect(&document, &snapshot, "parent")?;
    let child = required_rect(&document, &snapshot, "child")?;

    assert!((vertical_gap(before, parent) - 24.0).abs() < 0.1);
    assert!((parent.y() - child.y()).abs() < 0.1);
    Ok(())
}

#[test]
fn inline_text_before_block_prevents_top_parent_child_collapse()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"<body style="margin:0"><div id="before" style="height:10px"></div><div id="parent" style="margin-top:10px;font-size:10px;line-height:10px">x<div id="child" style="height:20px;margin-top:30px"></div></div></body>"#,
        800.0,
    )?;

    let before = required_rect(&document, &snapshot, "before")?;
    let parent = required_rect(&document, &snapshot, "parent")?;
    let child = required_rect(&document, &snapshot, "child")?;

    assert!((vertical_gap(before, parent) - 10.0).abs() < 0.1);
    assert!(child.y() >= parent.y() + 39.0);
    Ok(())
}
