//! Phantom 2G-10 Positioned Layout I regression corpus.

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

#[test]
fn relative_offsets_translate_without_consuming_normal_flow_space()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="moved" style="position:relative;top:20px;left:15px;width:100px;height:50px"></div>
          <div id="sibling" style="width:100px;height:30px"></div>
        </body>
        "#,
        400.0,
    )?;

    let moved = required_rect(&document, &snapshot, "moved")?;
    let sibling = required_rect(&document, &snapshot, "sibling")?;

    assert!((moved.x() - 15.0).abs() < 0.1);
    assert!((moved.y() - 20.0).abs() < 0.1);
    assert!((sibling.y() - 50.0).abs() < 0.1);
    Ok(())
}

#[test]
fn relative_translation_moves_the_complete_subtree() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="parent" style="position:relative;left:30px;top:12px;width:200px;height:100px">
            <div id="child" style="width:40px;height:20px"></div>
          </div>
        </body>
        "#,
        400.0,
    )?;

    let parent = required_rect(&document, &snapshot, "parent")?;
    let child = required_rect(&document, &snapshot, "child")?;

    assert!((parent.x() - 30.0).abs() < 0.1);
    assert!((parent.y() - 12.0).abs() < 0.1);
    assert!((child.x() - 30.0).abs() < 0.1);
    assert!((child.y() - 12.0).abs() < 0.1);
    Ok(())
}

#[test]
fn nested_relative_offsets_compose_after_parent_translation()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="parent" style="position:relative;left:10px;top:20px;width:200px;height:100px">
            <div id="child" style="position:relative;left:5px;top:7px;width:40px;height:20px"></div>
          </div>
        </body>
        "#,
        400.0,
    )?;

    let child = required_rect(&document, &snapshot, "child")?;

    assert!((child.x() - 15.0).abs() < 0.1);
    assert!((child.y() - 27.0).abs() < 0.1);
    Ok(())
}

#[test]
fn percentages_resolve_against_the_position_containing_block()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="container" style="width:200px;height:100px;padding:20px">
            <div id="child" style="position:relative;left:10%;top:20%;width:40px;height:20px"></div>
          </div>
        </body>
        "#,
        400.0,
    )?;

    let child = required_rect(&document, &snapshot, "child")?;

    // Static content origin is (20, 20); percentages use the 200x100
    // containing content box, not the parent's padded border box.
    assert!((child.x() - 40.0).abs() < 0.1);
    assert!((child.y() - 40.0).abs() < 0.1);
    Ok(())
}

#[test]
fn left_and_top_win_when_both_physical_insets_are_specified()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="child" style="position:relative;left:12px;right:80px;top:9px;bottom:70px;width:40px;height:20px"></div>
        </body>
        "#,
        400.0,
    )?;

    let child = required_rect(&document, &snapshot, "child")?;

    assert!((child.x() - 12.0).abs() < 0.1);
    assert!((child.y() - 9.0).abs() < 0.1);
    Ok(())
}
