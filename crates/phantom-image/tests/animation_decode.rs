//! Integration tests for Phantom's bounded animated-image decode boundary.
//!
//! The fixtures contain two tiny deterministic frames. Tests verify that GIF
//! and animated WebP are decoded behind Phantom-owned types without exposing
//! codec-library frame objects to the browser engine.

use phantom_image::{
    AnimatedImageDecoder, AnimationDecodeLimits, AnimationLoopCount, ImageDecodeLimits,
    ImageDecoder, ImageFormat, RasterImageDecoder, image_is_animated,
};

const GIF_BYTES: &[u8] = include_bytes!("fixtures/animated-2x1.gif");
const WEBP_BYTES: &[u8] = include_bytes!("fixtures/animated-2x1.webp");

#[test]
fn decodes_animated_gif_frames() -> Result<(), Box<dyn std::error::Error>> {
    let decoder = RasterImageDecoder;
    let image_limits = ImageDecodeLimits::default();
    let animation_limits = AnimationDecodeLimits::default();
    let metadata = decoder.probe(GIF_BYTES, image_limits)?;
    let animation = decoder.decode_animation(GIF_BYTES, image_limits, animation_limits)?;

    assert_eq!(metadata.format(), ImageFormat::Gif);
    assert!(image_is_animated(GIF_BYTES, metadata));
    assert_eq!(animation.size().width(), 2);
    assert_eq!(animation.size().height(), 1);
    assert_eq!(animation.frames().len(), 2);
    assert_eq!(animation.loop_count(), AnimationLoopCount::Infinite);
    assert!(
        animation
            .frames()
            .iter()
            .all(|frame| frame.image().rgba8().len() == 8)
    );

    Ok(())
}

#[test]
fn decodes_animated_webp_frames() -> Result<(), Box<dyn std::error::Error>> {
    let decoder = RasterImageDecoder;
    let image_limits = ImageDecodeLimits::default();
    let animation_limits = AnimationDecodeLimits::default();
    let metadata = decoder.probe(WEBP_BYTES, image_limits)?;
    let animation = decoder.decode_animation(WEBP_BYTES, image_limits, animation_limits)?;

    assert_eq!(metadata.format(), ImageFormat::WebP);
    assert!(image_is_animated(WEBP_BYTES, metadata));
    assert_eq!(animation.size().width(), 2);
    assert_eq!(animation.size().height(), 1);
    assert_eq!(animation.frames().len(), 2);
    assert_eq!(animation.loop_count(), AnimationLoopCount::Infinite);
    assert!(animation.total_raster_bytes() >= 16);

    Ok(())
}

#[test]
fn animation_budget_rejects_excess_frames() -> Result<(), Box<dyn std::error::Error>> {
    let decoder = RasterImageDecoder;
    let image_limits = ImageDecodeLimits::default();
    let animation_limits = AnimationDecodeLimits::new(1, 1024);
    let result = decoder.decode_animation(GIF_BYTES, image_limits, animation_limits);

    assert!(result.is_err());
    Ok(())
}
