//! Public box-and-visual compatibility corpus for Phantom cycle 2F-5.
//!
//! This slice stays inside values already represented by `ComputedStyle` and
//! already consumed by layout/paint. It does not introduce new layout or paint
//! concepts.

use std::collections::BTreeMap;

use phantom_css::{BorderStyle, Rgba, StyleMap, compute_styles};
use phantom_dom::{Document, ElementData, NodeId, NodeKind};

fn element_with_style(style: &str) -> Result<(Document, NodeId), phantom_dom::DomError> {
    let mut document = Document::new();
    let root = document.root();
    let mut attributes = BTreeMap::new();
    attributes.insert("style".to_owned(), style.to_owned());
    let node = document.append_child(
        root,
        NodeKind::Element(ElementData::with_attributes("div", attributes)),
    )?;
    Ok((document, node))
}

fn style_for(styles: &StyleMap, node: NodeId) -> phantom_css::ComputedStyle {
    styles.get(node).cloned().unwrap_or_default()
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn border_width_keywords_map_to_bounded_css_pixels() -> Result<(), phantom_dom::DomError> {
    let (document, node) =
        element_with_style("border-style:solid;border-width:thin medium thick 0")?;
    let styles = compute_styles(&document);
    let style = style_for(&styles, node);
    let border = style.border_width();

    assert_close(border.top(), 1.0);
    assert_close(border.right(), 3.0);
    assert_close(border.bottom(), 5.0);
    assert_close(border.left(), 0.0);
    Ok(())
}

#[test]
fn border_shorthand_resets_omitted_width_and_color() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style(
        "color:blue;border-width:10px;border-color:red;border-style:solid;border:solid",
    )?;
    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_eq!(style.border_style(), BorderStyle::Solid);
    assert_close(style.border_width().top(), 3.0);
    assert_eq!(style.border_color(), Rgba::new(0, 0, 255, 255));
    Ok(())
}

#[test]
fn border_color_only_shorthand_resets_style_to_none() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("border:5px solid blue;border:red")?;
    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_eq!(style.border_style(), BorderStyle::None);
    assert_close(style.border_width().top(), 0.0);
    assert_eq!(style.border_color(), Rgba::new(255, 0, 0, 255));
    Ok(())
}

#[test]
fn border_thick_solid_named_color_is_computed() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("border:thick solid navy")?;
    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_eq!(style.border_style(), BorderStyle::Solid);
    assert_close(style.border_width().top(), 5.0);
    assert_eq!(style.border_color(), Rgba::new(0, 0, 128, 255));
    Ok(())
}

#[test]
fn background_none_resets_prior_background_color() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("background-color:red;background:none")?;
    let styles = compute_styles(&document);

    assert_eq!(style_for(&styles, node).background_color(), None);
    Ok(())
}

#[test]
fn four_digit_hex_color_expands_alpha_nibble() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("background-color:#0f08")?;
    let styles = compute_styles(&document);

    assert_eq!(
        style_for(&styles, node).background_color(),
        Some(Rgba::new(0, 255, 0, 136))
    );
    Ok(())
}

#[test]
fn eight_digit_hex_color_preserves_explicit_alpha() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("background-color:#12345680")?;
    let styles = compute_styles(&document);

    assert_eq!(
        style_for(&styles, node).background_color(),
        Some(Rgba::new(18, 52, 86, 128))
    );
    Ok(())
}

#[test]
fn rgb_percentage_channels_are_supported() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("color:rgb(100%,50%,0%)")?;
    let styles = compute_styles(&document);

    assert_eq!(
        style_for(&styles, node).color(),
        Rgba::new(255, 128, 0, 255)
    );
    Ok(())
}

#[test]
fn rgba_percentage_alpha_is_supported() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("color:rgba(0%,100%,0%,50%)")?;
    let styles = compute_styles(&document);

    assert_eq!(style_for(&styles, node).color(), Rgba::new(0, 255, 0, 128));
    Ok(())
}

#[test]
fn basic_css_named_colors_extend_the_existing_palette() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style("color:aqua;background-color:fuchsia")?;
    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_eq!(style.color(), Rgba::new(0, 255, 255, 255));
    assert_eq!(style.background_color(), Some(Rgba::new(255, 0, 255, 255)));
    Ok(())
}

#[test]
fn invalid_later_visual_values_do_not_erase_prior_winners() -> Result<(), phantom_dom::DomError> {
    let (document, node) = element_with_style(
        "background-color:red;background-color:#xyz;border-style:solid;border-width:4px;border-width:huge",
    )?;
    let styles = compute_styles(&document);
    let style = style_for(&styles, node);

    assert_eq!(style.background_color(), Some(Rgba::new(255, 0, 0, 255)));
    assert_close(style.border_width().top(), 4.0);
    Ok(())
}
