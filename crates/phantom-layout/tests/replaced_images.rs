//! Integration tests for Phantom replaced-image layout behavior.
//!
//! These tests validate the first `<img>` sizing contract across HTML
//! dimension attributes, intrinsic image metadata, CSS sizing, aspect-ratio
//! preservation, and the default replaced-element fallback geometry.
//!
//! Decoding and raster painting intentionally remain outside this test crate.

use std::io;

use phantom_css::compute_styles;
use phantom_dom::NodeKind;
use phantom_image::{ImageCatalog, ImageFormat, ImageMetadata, ImageResourceId, IntrinsicSize};
use phantom_layout::{LayoutKind, build_layout_snapshot, build_layout_snapshot_with_images};

#[test]
fn image_dimension_attributes_create_replaced_box() -> Result<(), Box<dyn std::error::Error>> {
    let document = phantom_html::parse(
        r#"<p>before <img src="photo.png" width="120" height="80" alt="Photo"> after</p>"#,
    )?;

    let styles = compute_styles(&document);
    let layout = build_layout_snapshot(&document, &styles, 800.0)?;

    let image = layout
        .boxes()
        .iter()
        .find(|layout_box| matches!(layout_box.kind(), LayoutKind::Image { .. }))
        .ok_or_else(|| io::Error::other("image layout box missing"))?;

    assert!((image.rect().width() - 120.0).abs() < f32::EPSILON);

    assert!((image.rect().height() - 80.0).abs() < f32::EPSILON);

    assert_eq!(layout.image_source_for(image), Some("photo.png"),);

    assert_eq!(layout.image_alt_for(image), Some("Photo"),);

    Ok(())
}

#[test]
fn pending_image_uses_default_object_size() -> Result<(), Box<dyn std::error::Error>> {
    let document = phantom_html::parse(r#"<img src="pending.png" alt="Pending">"#)?;

    let styles = compute_styles(&document);
    let layout = build_layout_snapshot(&document, &styles, 800.0)?;

    let image = layout
        .boxes()
        .iter()
        .find(|layout_box| matches!(layout_box.kind(), LayoutKind::Image { .. }))
        .ok_or_else(|| io::Error::other("image layout box missing"))?;

    assert!((image.rect().width() - 300.0).abs() < f32::EPSILON);

    assert!((image.rect().height() - 150.0).abs() < f32::EPSILON);

    Ok(())
}

#[test]
fn registered_intrinsic_metadata_drives_auto_size() -> Result<(), Box<dyn std::error::Error>> {
    let document = phantom_html::parse(r#"<img src="photo.jpg" alt="Photo">"#)?;

    let image_node = document
        .nodes()
        .find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.tag_name() == "img" => Some(node.id()),

            _ => None,
        })
        .ok_or_else(|| io::Error::other("img node missing"))?;

    let mut images = ImageCatalog::default();

    images.insert(
        ImageResourceId::new(image_node.as_u64()),
        ImageMetadata::new(ImageFormat::Jpeg, IntrinsicSize::new(640, 480)?),
    );

    let styles = compute_styles(&document);

    let layout = build_layout_snapshot_with_images(&document, &styles, 1000.0, &images)?;

    let image = layout
        .boxes()
        .iter()
        .find(|layout_box| matches!(layout_box.kind(), LayoutKind::Image { .. }))
        .ok_or_else(|| io::Error::other("image layout box missing"))?;

    assert!((image.rect().width() - 640.0).abs() < f32::EPSILON);

    assert!((image.rect().height() - 480.0).abs() < f32::EPSILON);

    Ok(())
}

#[test]
fn css_width_preserves_intrinsic_ratio() -> Result<(), Box<dyn std::error::Error>> {
    let document =
        phantom_html::parse(r#"<img src="ratio.jpg" style="width: 200px" alt="Ratio">"#)?;

    let image_node = document
        .nodes()
        .find_map(|node| match node.kind() {
            NodeKind::Element(element) if element.tag_name() == "img" => Some(node.id()),

            _ => None,
        })
        .ok_or_else(|| io::Error::other("img node missing"))?;

    let mut images = ImageCatalog::default();

    images.insert(
        ImageResourceId::new(image_node.as_u64()),
        ImageMetadata::new(ImageFormat::Jpeg, IntrinsicSize::new(400, 300)?),
    );

    let styles = compute_styles(&document);

    let layout = build_layout_snapshot_with_images(&document, &styles, 800.0, &images)?;

    let image = layout
        .boxes()
        .iter()
        .find(|layout_box| matches!(layout_box.kind(), LayoutKind::Image { .. }))
        .ok_or_else(|| io::Error::other("image layout box missing"))?;

    assert!((image.rect().width() - 200.0).abs() < f32::EPSILON);

    assert!((image.rect().height() - 150.0).abs() < f32::EPSILON);

    Ok(())
}
