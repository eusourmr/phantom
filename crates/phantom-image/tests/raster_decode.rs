//! Integration tests for Phantom's first bounded raster decoder backend.
//!
//! The fixtures are tiny deterministic PNG/JPEG resources used only to verify
//! that codec output is converted into Phantom-owned RGBA8 buffers behind the
//! image boundary.

use phantom_image::{ImageDecodeLimits, ImageDecoder, ImageFormat, RasterImageDecoder};

const PNG_BYTES: &[u8] = include_bytes!("fixtures/rgba-2x1.png");
const JPEG_BYTES: &[u8] = include_bytes!("fixtures/rgb-2x1.jpg");

#[test]
fn decodes_png_into_rgba8() -> Result<(), Box<dyn std::error::Error>> {
    let decoder = RasterImageDecoder;
    let limits = ImageDecodeLimits::default();
    let metadata = decoder.probe(PNG_BYTES, limits)?;
    let decoded = decoder.decode(PNG_BYTES, limits)?;

    assert_eq!(metadata.format(), ImageFormat::Png);
    assert_eq!(decoded.size().width(), 2);
    assert_eq!(decoded.size().height(), 1);
    assert_eq!(decoded.rgba8().len(), 8);

    Ok(())
}

#[test]
fn decodes_jpeg_into_rgba8() -> Result<(), Box<dyn std::error::Error>> {
    let decoder = RasterImageDecoder;
    let limits = ImageDecodeLimits::default();
    let metadata = decoder.probe(JPEG_BYTES, limits)?;
    let decoded = decoder.decode(JPEG_BYTES, limits)?;

    assert_eq!(metadata.format(), ImageFormat::Jpeg);
    assert_eq!(decoded.size().width(), 2);
    assert_eq!(decoded.size().height(), 1);
    assert_eq!(decoded.rgba8().len(), 8);

    Ok(())
}
