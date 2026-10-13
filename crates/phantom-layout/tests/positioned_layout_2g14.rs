//! Phantom 2G-14 Fixed Positioning I regression corpus.

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
fn fixed_box_is_removed_from_normal_flow() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="fixed" style="position:fixed;left:15px;top:20px;width:80px;height:40px"></div>
          <div id="normal" style="width:100px;height:30px"></div>
        </body>
        "#,
        400.0,
        240.0,
    )?;

    let fixed = required_rect(&document, &snapshot, "fixed")?;
    let normal = required_rect(&document, &snapshot, "normal")?;

    assert!((fixed.x() - 15.0).abs() < 0.1);
    assert!((fixed.y() - 20.0).abs() < 0.1);
    assert!((normal.y() - 0.0).abs() < 0.1);
    Ok(())
}

#[test]
fn fixed_ignores_positioned_ancestor_and_uses_viewport() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="position:relative;margin-left:40px;width:180px;height:90px;padding:20px">
            <div id="fixed" style="position:fixed;right:10px;bottom:15px;width:40px;height:20px"></div>
          </div>
        </body>
        "#,
        500.0,
        300.0,
    )?;

    let fixed = required_rect(&document, &snapshot, "fixed")?;
    assert!((fixed.x() - 450.0).abs() < 0.1);
    assert!((fixed.y() - 265.0).abs() < 0.1);
    Ok(())
}

#[test]
fn fixed_percentage_insets_resolve_against_viewport() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="position:relative;width:100px;height:50px">
            <div id="fixed" style="position:fixed;left:10%;top:50%;width:20px;height:10px"></div>
          </div>
        </body>
        "#,
        600.0,
        240.0,
    )?;

    let fixed = required_rect(&document, &snapshot, "fixed")?;
    assert!((fixed.x() - 60.0).abs() < 0.1);
    assert!((fixed.y() - 120.0).abs() < 0.1);
    Ok(())
}

#[test]
fn fixed_flex_child_is_not_a_flex_item() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="display:flex;width:200px;height:80px">
            <div id="fixed" style="position:fixed;left:5px;top:6px;width:30px;height:20px"></div>
            <div id="normal" style="width:50px;height:20px"></div>
          </div>
        </body>
        "#,
        400.0,
        220.0,
    )?;

    let fixed = required_rect(&document, &snapshot, "fixed")?;
    let normal = required_rect(&document, &snapshot, "normal")?;
    assert!((fixed.x() - 5.0).abs() < 0.1);
    assert!((fixed.y() - 6.0).abs() < 0.1);
    assert!((normal.x() - 0.0).abs() < 0.1);
    Ok(())
}

#[test]
fn relative_ancestor_translation_does_not_move_fixed_descendant()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="position:relative;left:70px;top:35px;width:180px;height:90px">
            <div id="fixed" style="position:fixed;left:12px;top:18px;width:30px;height:20px"></div>
          </div>
        </body>
        "#,
        500.0,
        300.0,
    )?;

    let fixed = required_rect(&document, &snapshot, "fixed")?;
    assert!((fixed.x() - 12.0).abs() < 0.1);
    assert!((fixed.y() - 18.0).abs() < 0.1);
    Ok(())
}

#[test]
fn absolute_ancestor_placement_does_not_move_fixed_descendant()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="position:absolute;left:90px;top:45px;width:180px;height:90px">
            <div id="fixed" style="position:fixed;left:14px;top:22px;width:30px;height:20px"></div>
          </div>
        </body>
        "#,
        500.0,
        300.0,
    )?;

    let fixed = required_rect(&document, &snapshot, "fixed")?;
    assert!((fixed.x() - 14.0).abs() < 0.1);
    assert!((fixed.y() - 22.0).abs() < 0.1);
    Ok(())
}
