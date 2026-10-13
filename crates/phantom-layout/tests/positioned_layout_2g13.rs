//! Phantom 2G-13 viewport geometry / initial containing block regression corpus.

use std::io;

use phantom_dom::{Document, NodeId, NodeKind};
use phantom_layout::{LayoutKind, LayoutSnapshot, Rect, build_layout_snapshot_with_viewport};

fn layout(
    source: &str,
    viewport_width: f32,
    viewport_height: f32,
) -> Result<(Document, LayoutSnapshot), Box<dyn std::error::Error>> {
    let document = phantom_html::parse(source)?;
    let styles = phantom_css::compute_styles(&document);
    let snapshot =
        build_layout_snapshot_with_viewport(&document, &styles, viewport_width, viewport_height)?;
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
fn snapshot_retains_two_dimensional_viewport_geometry() -> Result<(), Box<dyn std::error::Error>> {
    let (_document, snapshot) = layout("<body style=\"margin:0\"></body>", 640.0, 360.0)?;

    assert!((snapshot.viewport_width() - 640.0).abs() < f32::EPSILON);
    assert!((snapshot.viewport_height() - 360.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn root_absolute_bottom_anchors_to_viewport_not_document_height()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="height:20px"></div>
          <div id="absolute" style="position:absolute;right:10px;bottom:15px;width:40px;height:20px"></div>
        </body>
        "#,
        400.0,
        300.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    assert!((absolute.x() - 350.0).abs() < 0.1);
    assert!((absolute.y() - 265.0).abs() < 0.1);
    Ok(())
}

#[test]
fn root_percentage_vertical_inset_resolves_against_viewport_height()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="height:30px"></div>
          <div id="absolute" style="position:absolute;left:0;top:50%;width:20px;height:10px"></div>
        </body>
        "#,
        500.0,
        240.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    assert!((absolute.y() - 120.0).abs() < 0.1);
    Ok(())
}

#[test]
fn positioned_ancestor_keeps_its_own_padding_box_geometry() -> Result<(), Box<dyn std::error::Error>>
{
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="position:relative;width:200px;height:100px">
            <div id="absolute" style="position:absolute;right:10px;bottom:15px;width:40px;height:20px"></div>
          </div>
        </body>
        "#,
        700.0,
        500.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    assert!((absolute.x() - 150.0).abs() < 0.1);
    assert!((absolute.y() - 65.0).abs() < 0.1);
    Ok(())
}
