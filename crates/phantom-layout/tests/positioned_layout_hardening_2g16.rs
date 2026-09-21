//! Phantom 2G-16 positioned-layout hardening corpus.

// PHANTOM_2G16_POSITIONED_LAYOUT_HARDENING

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
fn fixed_auto_width_stretches_between_viewport_insets() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="fixed" style="position:fixed;left:20px;right:30px;width:auto;height:20px"></div>
        </body>
        "#,
        500.0,
        300.0,
    )?;

    let fixed = required_rect(&document, &snapshot, "fixed")?;
    assert!((fixed.x() - 20.0).abs() < 0.1);
    assert!((fixed.width() - 450.0).abs() < 0.1);
    Ok(())
}

#[test]
fn absolute_and_fixed_percentages_keep_distinct_containing_blocks()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="position:relative;margin-left:40px;width:200px;height:100px;border:5px solid black;padding:10px">
            <div id="absolute" style="position:absolute;left:10%;top:20%;width:20px;height:10px"></div>
            <div id="fixed" style="position:fixed;left:10%;top:20%;width:20px;height:10px"></div>
          </div>
        </body>
        "#,
        500.0,
        300.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    let fixed = required_rect(&document, &snapshot, "fixed")?;

    // Positioned ancestor padding box: origin (45, 5), size 220x120.
    assert!((absolute.x() - 67.0).abs() < 0.1);
    assert!((absolute.y() - 29.0).abs() < 0.1);
    // Fixed always resolves percentages against the viewport.
    assert!((fixed.x() - 50.0).abs() < 0.1);
    assert!((fixed.y() - 60.0).abs() < 0.1);
    Ok(())
}

#[test]
fn relative_translation_keeps_flow_position_and_does_not_drag_fixed_descendant()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="relative" style="position:relative;left:30px;top:20px;width:100px;height:40px">
            <div id="absolute" style="position:absolute;left:5px;top:6px;width:20px;height:10px"></div>
            <div id="fixed" style="position:fixed;left:7px;top:8px;width:20px;height:10px"></div>
          </div>
          <div id="normal" style="width:80px;height:20px"></div>
        </body>
        "#,
        400.0,
        240.0,
    )?;

    let relative = required_rect(&document, &snapshot, "relative")?;
    let absolute = required_rect(&document, &snapshot, "absolute")?;
    let fixed = required_rect(&document, &snapshot, "fixed")?;
    let normal = required_rect(&document, &snapshot, "normal")?;

    assert!((relative.x() - 30.0).abs() < 0.1);
    assert!((relative.y() - 20.0).abs() < 0.1);
    assert!((absolute.x() - 35.0).abs() < 0.1);
    assert!((absolute.y() - 26.0).abs() < 0.1);
    assert!((fixed.x() - 7.0).abs() < 0.1);
    assert!((fixed.y() - 8.0).abs() < 0.1);
    // Relative visual translation must not consume extra normal-flow space.
    assert!((normal.y() - 40.0).abs() < 0.1);
    Ok(())
}

#[test]
fn flex_pressure_does_not_turn_absolute_or_fixed_children_into_flex_items()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="display:flex;position:relative;width:200px;height:80px">
            <div id="absolute" style="position:absolute;right:10px;top:10px;width:30px;height:20px"></div>
            <div id="fixed" style="position:fixed;right:10px;top:10px;width:30px;height:20px"></div>
            <div id="normal" style="width:50px;height:20px"></div>
          </div>
        </body>
        "#,
        500.0,
        300.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    let fixed = required_rect(&document, &snapshot, "fixed")?;
    let normal = required_rect(&document, &snapshot, "normal")?;

    assert!((absolute.x() - 160.0).abs() < 0.1);
    assert!((fixed.x() - 460.0).abs() < 0.1);
    assert!((normal.x() - 0.0).abs() < 0.1);
    Ok(())
}

#[test]
fn replaced_absolute_and_fixed_images_keep_their_coordinate_spaces()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="position:relative;margin-left:50px;width:160px;height:80px">
            <img id="absolute" src="a.png" width="40" height="20" style="position:absolute;left:10px;top:12px">
            <img id="fixed" src="b.png" width="40" height="20" style="position:fixed;right:10px;bottom:15px">
          </div>
        </body>
        "#,
        500.0,
        300.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    let fixed = required_rect(&document, &snapshot, "fixed")?;

    assert!((absolute.x() - 60.0).abs() < 0.1);
    assert!((absolute.y() - 12.0).abs() < 0.1);
    assert!((fixed.x() - 450.0).abs() < 0.1);
    assert!((fixed.y() - 265.0).abs() < 0.1);
    Ok(())
}
