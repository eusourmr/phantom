//! Phantom 2G-11 Positioned Layout II regression corpus.

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
fn absolute_box_is_removed_from_normal_flow() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="absolute" style="position:absolute;left:15px;top:20px;width:80px;height:40px"></div>
          <div id="normal" style="width:100px;height:30px"></div>
        </body>
        "#,
        400.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    let normal = required_rect(&document, &snapshot, "normal")?;

    assert!((absolute.x() - 15.0).abs() < 0.1);
    assert!((absolute.y() - 20.0).abs() < 0.1);
    assert!((normal.y() - 0.0).abs() < 0.1);
    Ok(())
}

#[test]
fn nearest_positioned_ancestor_padding_box_is_containing_block()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="outer" style="position:relative;width:200px;height:120px;border:10px solid black;padding:20px">
            <div style="width:50px;height:20px">
              <div id="absolute" style="position:absolute;left:10%;top:25%;width:30px;height:10px"></div>
            </div>
          </div>
        </body>
        "#,
        500.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;

    // outer border-box width is 260, padding box begins at (10,10) and is 240x160.
    assert!((absolute.x() - 34.0).abs() < 0.1);
    assert!((absolute.y() - 50.0).abs() < 0.1);
    Ok(())
}

#[test]
fn right_and_bottom_anchor_when_start_insets_are_auto() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="container" style="position:relative;width:200px;height:100px">
            <div id="absolute" style="position:absolute;right:10px;bottom:15px;width:40px;height:20px"></div>
          </div>
        </body>
        "#,
        500.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    assert!((absolute.x() - 150.0).abs() < 0.1);
    assert!((absolute.y() - 65.0).abs() < 0.1);
    Ok(())
}

#[test]
fn absolute_flex_child_is_not_a_flex_item() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="display:flex;position:relative;width:200px;height:80px">
            <div id="absolute" style="position:absolute;left:5px;top:6px;width:30px;height:20px"></div>
            <div id="normal" style="width:50px;height:20px"></div>
          </div>
        </body>
        "#,
        400.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    let normal = required_rect(&document, &snapshot, "normal")?;
    assert!((absolute.x() - 5.0).abs() < 0.1);
    assert!((absolute.y() - 6.0).abs() < 0.1);
    assert!((normal.x() - 0.0).abs() < 0.1);
    Ok(())
}
