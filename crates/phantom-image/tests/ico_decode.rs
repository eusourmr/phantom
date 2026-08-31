//! Phantom 2C-15 bounded ICO decode regression tests.

use std::error::Error;

use phantom_image::{ImageDecodeLimits, ImageDecoder, ImageFormat, RasterImageDecoder};

fn one_pixel_ico() -> Vec<u8> {
    let mut bytes = Vec::new();

    // ICONDIR
    bytes.extend_from_slice(&[0x00, 0x00, 0x01, 0x00, 0x01, 0x00]);

    // ICONDIRENTRY: 1x1, 32 bpp, 48 bytes of DIB data, offset 22.
    bytes.extend_from_slice(&[
        0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x20, 0x00, 0x30, 0x00, 0x00, 0x00, 0x16, 0x00, 0x00,
        0x00,
    ]);

    // BITMAPINFOHEADER.
    bytes.extend_from_slice(&[
        0x28, 0x00, 0x00, 0x00, // header size
        0x01, 0x00, 0x00, 0x00, // width
        0x02, 0x00, 0x00, 0x00, // height: XOR + AND planes
        0x01, 0x00, // planes
        0x20, 0x00, // 32 bpp
        0x00, 0x00, 0x00, 0x00, // compression
        0x04, 0x00, 0x00, 0x00, // XOR bytes
        0x00, 0x00, 0x00, 0x00, // x ppm
        0x00, 0x00, 0x00, 0x00, // y ppm
        0x00, 0x00, 0x00, 0x00, // colors used
        0x00, 0x00, 0x00, 0x00, // important colors
    ]);

    // One opaque red BGRA pixel and one padded transparent AND-mask row.
    bytes.extend_from_slice(&[0x00, 0x00, 0xFF, 0xFF]);
    bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

    bytes
}

#[test]
fn bounded_ico_probe_and_decode() -> Result<(), Box<dyn Error>> {
    let bytes = one_pixel_ico();
    let decoder = RasterImageDecoder;
    let limits = ImageDecodeLimits::new(512, 512, 262_144, 1_048_576);

    let metadata = decoder.probe(&bytes, limits)?;
    assert_eq!(metadata.format(), ImageFormat::Ico);
    assert_eq!(metadata.size().width(), 1);
    assert_eq!(metadata.size().height(), 1);

    let decoded = decoder.decode(&bytes, limits)?;
    assert_eq!(decoded.size().width(), 1);
    assert_eq!(decoded.size().height(), 1);
    assert_eq!(decoded.rgba8().len(), 4);

    Ok(())
}

#[test]
fn ico_directory_is_rejected_before_unbounded_decode() {
    let truncated = [
        0x00, 0x00, 0x01, 0x00, 0x02, 0x00, // claims two entries, provides none
    ];

    let decoder = RasterImageDecoder;
    assert!(
        decoder
            .probe(&truncated, ImageDecodeLimits::default())
            .is_err()
    );
}
