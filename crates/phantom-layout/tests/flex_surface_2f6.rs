//! Layout validation for Phantom cycle 2F-6 Flexbox surface completion.
//!
//! The cases protect the geometry added for `space-around`. Existing flex
//! behavior remains covered by the broader layout regression suite.

use phantom_dom::{Document, NodeId, NodeKind};
use phantom_layout::{LayoutKind, LayoutSnapshot, Rect, build_layout_snapshot};

fn layout(source: &str) -> Result<(Document, LayoutSnapshot), Box<dyn std::error::Error>> {
    let document = phantom_html::parse(source)?;
    let styles = phantom_css::compute_styles(&document);
    let snapshot = build_layout_snapshot(&document, &styles, 1024.0)?;

    Ok((document, snapshot))
}

fn node_by_id(document: &Document, id: &str) -> Option<NodeId> {
    document.nodes().find_map(|node| {
        let NodeKind::Element(element) = node.kind() else {
            return None;
        };

        if element.attribute("id") == Some(id) {
            Some(node.id())
        } else {
            None
        }
    })
}

fn rect_by_id(document: &Document, snapshot: &LayoutSnapshot, id: &str) -> Option<Rect> {
    let node_id = node_by_id(document, id)?;

    snapshot
        .boxes()
        .iter()
        .find(|layout_box| {
            layout_box.source_node() == node_id
                && matches!(layout_box.kind(), LayoutKind::Block | LayoutKind::Flex)
        })
        .map(|layout_box| layout_box.rect())
}

#[test]
fn justify_content_space_around_distributes_items() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="container"
               style="display:flex;width:300px;height:100px;justify-content:space-around">
            <div id="a" style="width:50px;height:20px"></div>
            <div id="b" style="width:50px;height:20px"></div>
          </div>
        </body>
        "#,
    )?;

    let container = rect_by_id(&document, &snapshot, "container").unwrap_or_default();
    let a = rect_by_id(&document, &snapshot, "a").unwrap_or_default();
    let b = rect_by_id(&document, &snapshot, "b").unwrap_or_default();

    assert!(((a.x() - container.x()) - 50.0).abs() < 0.1);
    assert!(((b.x() - container.x()) - 200.0).abs() < 0.1);
    Ok(())
}

#[test]
fn align_content_space_around_distributes_flex_lines() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <body style="margin:0">
          <div id="container"
               style="display:flex;flex-wrap:wrap;width:100px;height:300px;align-content:space-around">
            <div id="a" style="width:100px;height:50px"></div>
            <div id="b" style="width:100px;height:50px"></div>
          </div>
        </body>
        "#,
    )?;

    let container = rect_by_id(&document, &snapshot, "container").unwrap_or_default();
    let a = rect_by_id(&document, &snapshot, "a").unwrap_or_default();
    let b = rect_by_id(&document, &snapshot, "b").unwrap_or_default();

    assert!(((a.y() - container.y()) - 50.0).abs() < 0.1);
    assert!(((b.y() - container.y()) - 200.0).abs() < 0.1);
    Ok(())
}
