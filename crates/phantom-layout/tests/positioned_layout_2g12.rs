//! Phantom 2G-12 Positioned Layout III regression corpus.

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
fn auto_width_stretches_between_left_and_right_insets() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="container" style="position:relative;width:200px;height:100px">
            <div id="absolute" style="position:absolute;left:20px;right:30px;width:auto;height:20px"></div>
          </div>
        </body>
        "#,
        500.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    assert!((absolute.x() - 20.0).abs() < 0.1);
    assert!((absolute.width() - 150.0).abs() < 0.1);
    Ok(())
}

#[test]
fn auto_width_inset_solving_accounts_for_fixed_horizontal_margins()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="position:relative;width:200px;height:100px">
            <div id="absolute" style="position:absolute;left:10px;right:20px;width:auto;height:20px;margin-left:5px;margin-right:15px"></div>
          </div>
        </body>
        "#,
        500.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    assert!((absolute.x() - 15.0).abs() < 0.1);
    assert!((absolute.width() - 150.0).abs() < 0.1);
    assert!((absolute.right() - 165.0).abs() < 0.1);
    Ok(())
}

#[test]
fn percentage_insets_participate_in_auto_width_solving() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="position:relative;width:300px;height:100px">
            <div id="absolute" style="position:absolute;left:10%;right:20%;width:auto;height:20px"></div>
          </div>
        </body>
        "#,
        500.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    assert!((absolute.x() - 30.0).abs() < 0.1);
    assert!((absolute.width() - 210.0).abs() < 0.1);
    Ok(())
}

#[test]
fn explicit_width_keeps_left_precedence_when_both_insets_are_present()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div style="position:relative;width:200px;height:100px">
            <div id="absolute" style="position:absolute;left:10px;right:20px;width:40px;height:20px"></div>
          </div>
        </body>
        "#,
        500.0,
    )?;

    let absolute = required_rect(&document, &snapshot, "absolute")?;
    assert!((absolute.x() - 10.0).abs() < 0.1);
    assert!((absolute.width() - 40.0).abs() < 0.1);
    Ok(())
}
