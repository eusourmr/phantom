//! Phantom 2G-1 normal-flow used-margin validation.
//!
//! CSS preserves `margin:auto` as semantic state. These cases verify that
//! block formatting resolves horizontal automatic margins into used geometry
//! without stealing Flexbox's separate auto-margin ownership.

use std::io;

use phantom_css::EdgeSizes;
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

fn geometry_by_id(
    document: &Document,
    snapshot: &LayoutSnapshot,
    id: &str,
) -> Option<(Rect, EdgeSizes)> {
    let node_id = node_by_id(document, id)?;

    snapshot.boxes().iter().find_map(|layout_box| {
        if layout_box.source_node() != node_id {
            return None;
        }

        match layout_box.kind() {
            LayoutKind::Block
            | LayoutKind::Flex
            | LayoutKind::Image { .. }
            | LayoutKind::Control { .. } => Some((layout_box.rect(), layout_box.margin())),
            LayoutKind::Root | LayoutKind::Line | LayoutKind::Text { .. } => None,
        }
    })
}

fn required_geometry(
    document: &Document,
    snapshot: &LayoutSnapshot,
    id: &str,
) -> Result<(Rect, EdgeSizes), Box<dyn std::error::Error>> {
    geometry_by_id(document, snapshot, id)
        .ok_or_else(|| io::Error::other(format!("layout geometry missing for #{id}")).into())
}

#[test]
fn paired_auto_margins_center_fixed_width_block() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="container" style="width:400px">
            <div id="child" style="width:100px;height:20px;margin-left:auto;margin-right:auto"></div>
          </div>
        </body>
        "#,
        800.0,
    )?;

    let (container, _) = required_geometry(&document, &snapshot, "container")?;
    let (child, margin) = required_geometry(&document, &snapshot, "child")?;

    assert!(((child.x() - container.x()) - 150.0).abs() < 0.1);
    assert!((margin.left() - 150.0).abs() < 0.1);
    assert!((margin.right() - 150.0).abs() < 0.1);
    Ok(())
}

#[test]
fn left_auto_margin_absorbs_remaining_inline_space() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="container" style="width:400px">
            <div id="child" style="width:100px;height:20px;margin-left:auto;margin-right:20px"></div>
          </div>
        </body>
        "#,
        800.0,
    )?;

    let (container, _) = required_geometry(&document, &snapshot, "container")?;
    let (child, margin) = required_geometry(&document, &snapshot, "child")?;

    assert!(((child.x() - container.x()) - 280.0).abs() < 0.1);
    assert!((margin.left() - 280.0).abs() < 0.1);
    assert!((margin.right() - 20.0).abs() < 0.1);
    Ok(())
}

#[test]
fn right_auto_margin_preserves_fixed_left_margin() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="container" style="width:400px">
            <div id="child" style="width:100px;height:20px;margin-left:30px;margin-right:auto"></div>
          </div>
        </body>
        "#,
        800.0,
    )?;

    let (container, _) = required_geometry(&document, &snapshot, "container")?;
    let (child, margin) = required_geometry(&document, &snapshot, "child")?;

    assert!(((child.x() - container.x()) - 30.0).abs() < 0.1);
    assert!((margin.left() - 30.0).abs() < 0.1);
    assert!((margin.right() - 270.0).abs() < 0.1);
    Ok(())
}

#[test]
fn auto_width_block_keeps_automatic_margins_at_zero() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="container" style="width:400px">
            <div id="child" style="height:20px;margin-left:auto;margin-right:auto"></div>
          </div>
        </body>
        "#,
        800.0,
    )?;

    let (container, _) = required_geometry(&document, &snapshot, "container")?;
    let (child, margin) = required_geometry(&document, &snapshot, "child")?;

    assert!((child.x() - container.x()).abs() < 0.1);
    assert!((child.width() - 400.0).abs() < 0.1);
    assert!(margin.left().abs() < 0.1);
    assert!(margin.right().abs() < 0.1);
    Ok(())
}

#[test]
fn block_replaced_image_honors_paired_auto_margins() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="container" style="width:400px">
            <img id="image" src="photo.png" style="display:block;width:100px;height:50px;margin:0 auto" alt="Photo">
          </div>
        </body>
        "#,
        800.0,
    )?;

    let (container, _) = required_geometry(&document, &snapshot, "container")?;
    let (image, margin) = required_geometry(&document, &snapshot, "image")?;

    assert!(((image.x() - container.x()) - 150.0).abs() < 0.1);
    assert!((margin.left() - 150.0).abs() < 0.1);
    assert!((margin.right() - 150.0).abs() < 0.1);
    Ok(())
}

#[test]
fn block_form_control_honors_paired_auto_margins() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="container" style="width:400px">
            <input id="control" type="text" style="display:block;width:120px;margin-left:auto;margin-right:auto">
          </div>
        </body>
        "#,
        800.0,
    )?;

    let (container, _) = required_geometry(&document, &snapshot, "container")?;
    let (control, margin) = required_geometry(&document, &snapshot, "control")?;

    assert!(((control.x() - container.x()) - 140.0).abs() < 0.1);
    assert!((margin.left() - 140.0).abs() < 0.1);
    assert!((margin.right() - 140.0).abs() < 0.1);
    Ok(())
}
