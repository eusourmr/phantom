//! Executable Flexbox conformance slice aligned with CSSWG and WPT behavior.
//!
//! These tests are Phantom-native assertions. They do not claim to be the
//! official WPT runner yet. Each case records the standards family it protects
//! so the same behavior can later be mapped to pinned upstream WPT files.

use phantom_dom::{Document, NodeId, NodeKind};
use phantom_layout::{LayoutKind, LayoutSnapshot, Rect, build_layout_snapshot};

const CSS_FLEXBOX_SPEC: &str = "https://drafts.csswg.org/css-flexbox/";
const WPT_FLEXBOX_ROOT: &str = "css/css-flexbox/";

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
fn wpt_slice_row_main_axis_auto_margin_absorbs_free_space() -> Result<(), Box<dyn std::error::Error>>
{
    let _upstream_family = (CSS_FLEXBOX_SPEC, WPT_FLEXBOX_ROOT);

    let (document, snapshot) = layout(
        r#"
        <div id="container"
             style="display:flex;width:300px;height:100px">
          <div id="a" style="width:50px;height:20px"></div>
          <div id="b"
               style="width:50px;height:20px;margin-left:auto"></div>
        </div>
        "#,
    )?;

    let a = rect_by_id(&document, &snapshot, "a").unwrap_or_default();
    let b = rect_by_id(&document, &snapshot, "b").unwrap_or_default();

    assert!((a.x() - 0.0).abs() < 0.1);
    assert!((b.x() - 250.0).abs() < 0.1);

    Ok(())
}

#[test]
fn wpt_slice_two_auto_margins_center_flex_item() -> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <div id="container"
             style="display:flex;width:300px;height:100px">
          <div id="item"
               style="width:50px;height:20px;margin-left:auto;margin-right:auto">
          </div>
        </div>
        "#,
    )?;

    let item = rect_by_id(&document, &snapshot, "item").unwrap_or_default();

    assert!((item.x() - 125.0).abs() < 0.1);

    Ok(())
}

#[test]
fn wpt_slice_cross_axis_auto_margin_overrides_align_items() -> Result<(), Box<dyn std::error::Error>>
{
    let (document, snapshot) = layout(
        r#"
        <div id="container"
             style="display:flex;width:200px;height:100px;align-items:flex-start">
          <div id="item"
               style="width:50px;height:20px;margin-top:auto">
          </div>
        </div>
        "#,
    )?;

    let item = rect_by_id(&document, &snapshot, "item").unwrap_or_default();

    assert!((item.y() - 80.0).abs() < 0.1);

    Ok(())
}

#[test]
fn wpt_slice_column_main_axis_auto_margin_absorbs_free_space()
-> Result<(), Box<dyn std::error::Error>> {
    let (document, snapshot) = layout(
        r#"
        <div id="container"
             style="display:flex;flex-direction:column;width:100px;height:200px">
          <div id="item"
               style="width:40px;height:50px;margin-top:auto">
          </div>
        </div>
        "#,
    )?;

    let item = rect_by_id(&document, &snapshot, "item").unwrap_or_default();

    assert!((item.y() - 150.0).abs() < 0.1);

    Ok(())
}

#[test]
fn wpt_slice_flex_one_items_share_main_axis() -> Result<(), Box<dyn std::error::Error>> {
    let _upstream_reference = "css/css-flexbox/flex-one-sets-flex-basis-to-zero-px.html";

    let (document, snapshot) = layout(
        r#"
        <div id="container"
             style="display:flex;width:400px;height:80px">
          <div id="a" style="flex:1;min-width:0"></div>
          <div id="b" style="flex:1;min-width:0"></div>
        </div>
        "#,
    )?;

    let a = rect_by_id(&document, &snapshot, "a").unwrap_or_default();
    let b = rect_by_id(&document, &snapshot, "b").unwrap_or_default();

    assert!((a.width() - 200.0).abs() < 0.1);
    assert!((b.width() - 200.0).abs() < 0.1);
    assert!((b.x() - 200.0).abs() < 0.1);

    Ok(())
}
