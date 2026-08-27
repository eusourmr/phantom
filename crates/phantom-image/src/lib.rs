//! Safe, renderer-independent image resource boundary for Phantom.
//!
//! This crate deliberately separates image metadata, decode policy and decoded
//! pixels from DOM, layout, paint and the native browser shell.
//!
//! The first milestone provides:
//!
//! - opaque image resource identifiers;
//! - metadata storage;
//! - PNG, GIF, JPEG and WebP dimension probing;
//! - decoded RGBA8 validation;
//! - configurable decoded-memory limits;
//! - a narrow [`ImageDecoder`] contract;
//! - a bounded PNG/JPEG/static-WebP decoder backend using a mature codec boundary.
//!
//! GIF metadata is recognized, but GIF raster decode remains deliberately
//! unsupported in this milestone. Animated WebP decode is also rejected until
//! Phantom has an explicit animation timeline and frame-budget policy.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use thiserror::Error;

/// Opaque identifier used to connect one image resource across engine stages.
///
/// The identifier has no URL semantics and does not expose DOM ownership.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
)]
pub struct ImageResourceId(u64);

impl ImageResourceId {
    /// Creates an opaque image resource identifier.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric value used by the current engine revision.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Image container format recognized by the metadata probe.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
)]
pub enum ImageFormat {
    /// Portable Network Graphics.
    Png,

    /// Graphics Interchange Format.
    Gif,

    /// Joint Photographic Experts Group image.
    Jpeg,

    /// WebP image container. Animated WebP is not enabled in this milestone.
    WebP,
}

/// Intrinsic raster dimensions in CSS-pixel-independent source pixels.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
pub struct IntrinsicSize {
    width: u32,
    height: u32,
}

impl IntrinsicSize {
    /// Creates non-zero intrinsic dimensions.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::InvalidDimensions`] when either axis is zero.
    pub fn new(
        width: u32,
        height: u32,
    ) -> Result<Self, ImageError> {
        if width == 0 || height == 0 {
            return Err(
                ImageError::InvalidDimensions,
            );
        }

        Ok(Self { width, height })
    }

    /// Returns the intrinsic width.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the intrinsic height.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns width divided by height.
    #[must_use]
    pub fn aspect_ratio(self) -> f32 {
        self.width as f32
            / self.height as f32
    }

    /// Returns the source pixel count.
    #[must_use]
    pub const fn pixels(self) -> u64 {
        self.width as u64
            * self.height as u64
    }
}

/// Immutable metadata discovered before pixel decode.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
pub struct ImageMetadata {
    format: ImageFormat,
    size: IntrinsicSize,
}

impl ImageMetadata {
    /// Creates metadata for one image resource.
    #[must_use]
    pub const fn new(
        format: ImageFormat,
        size: IntrinsicSize,
    ) -> Self {
        Self { format, size }
    }

    /// Returns the detected container format.
    #[must_use]
    pub const fn format(self) -> ImageFormat {
        self.format
    }

    /// Returns intrinsic dimensions.
    #[must_use]
    pub const fn size(self) -> IntrinsicSize {
        self.size
    }
}

/// Bounded policy applied before allocating decoded image memory.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
pub struct ImageDecodeLimits {
    max_width: u32,
    max_height: u32,
    max_pixels: u64,
    max_decoded_bytes: u64,
}

impl ImageDecodeLimits {
    /// Creates an explicit decode policy.
    #[must_use]
    pub const fn new(
        max_width: u32,
        max_height: u32,
        max_pixels: u64,
        max_decoded_bytes: u64,
    ) -> Self {
        Self {
            max_width,
            max_height,
            max_pixels,
            max_decoded_bytes,
        }
    }

    /// Returns the maximum accepted source width.
    #[must_use]
    pub const fn max_width(self) -> u32 {
        self.max_width
    }

    /// Returns the maximum accepted source height.
    #[must_use]
    pub const fn max_height(self) -> u32 {
        self.max_height
    }

    /// Returns the maximum accepted source pixel count.
    #[must_use]
    pub const fn max_pixels(self) -> u64 {
        self.max_pixels
    }

    /// Returns the maximum accepted decoded byte count.
    #[must_use]
    pub const fn max_decoded_bytes(self) -> u64 {
        self.max_decoded_bytes
    }

    /// Validates dimensions before decode allocation.
    ///
    /// # Errors
    ///
    /// Returns an [`ImageError`] when any configured resource bound is
    /// exceeded.
    pub fn validate(
        self,
        size: IntrinsicSize,
    ) -> Result<(), ImageError> {
        if size.width() > self.max_width
            || size.height() > self.max_height
        {
            return Err(
                ImageError::DimensionsExceeded,
            );
        }

        let pixels = size.pixels();

        if pixels > self.max_pixels {
            return Err(
                ImageError::PixelBudgetExceeded,
            );
        }

        let rgba_bytes = pixels
            .checked_mul(4)
            .ok_or(
                ImageError::DecodedByteBudgetExceeded,
            )?;

        if rgba_bytes > self.max_decoded_bytes {
            return Err(
                ImageError::DecodedByteBudgetExceeded,
            );
        }

        Ok(())
    }
}

impl Default for ImageDecodeLimits {
    fn default() -> Self {
        // This is a Phantom resource-safety policy, not Web Platform
        // semantics. It is intentionally configurable by future engine policy.
        Self::new(
            16_384,
            16_384,
            67_108_864,
            268_435_456,
        )
    }
}

/// Validated RGBA8 pixels returned by a future decoder backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    size: IntrinsicSize,
    rgba8: Box<[u8]>,
}

impl DecodedImage {
    /// Creates a decoded RGBA8 image after validating dimensions and length.
    ///
    /// # Errors
    ///
    /// Returns an [`ImageError`] when limits are exceeded or the pixel buffer
    /// length does not equal `width * height * 4`.
    pub fn from_rgba8(
        size: IntrinsicSize,
        rgba8: Box<[u8]>,
        limits: ImageDecodeLimits,
    ) -> Result<Self, ImageError> {
        limits.validate(size)?;

        let expected = size
            .pixels()
            .checked_mul(4)
            .and_then(
                |bytes| usize::try_from(bytes).ok(),
            )
            .ok_or(
                ImageError::DecodedByteBudgetExceeded,
            )?;

        if rgba8.len() != expected {
            return Err(
                ImageError::InvalidRgbaLength,
            );
        }

        Ok(Self { size, rgba8 })
    }

    /// Returns decoded dimensions.
    #[must_use]
    pub const fn size(&self) -> IntrinsicSize {
        self.size
    }

    /// Returns immutable RGBA8 pixels.
    #[must_use]
    pub fn rgba8(&self) -> &[u8] {
        &self.rgba8
    }
}

/// Metadata associated with active image elements.
///
/// The catalog owns no decoded pixels and has no dependency on DOM types.
#[derive(Debug, Clone, Default)]
pub struct ImageCatalog {
    entries: BTreeMap<
        ImageResourceId,
        ImageMetadata,
    >,
}

impl ImageCatalog {
    /// Inserts or replaces metadata for one resource identifier.
    pub fn insert(
        &mut self,
        resource: ImageResourceId,
        metadata: ImageMetadata,
    ) {
        self.entries.insert(
            resource,
            metadata,
        );
    }

    /// Returns metadata for one image resource.
    #[must_use]
    pub fn get(
        &self,
        resource: ImageResourceId,
    ) -> Option<ImageMetadata> {
        self.entries
            .get(&resource)
            .copied()
    }

    /// Removes all metadata from the current document generation.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns the number of registered resources.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the catalog is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Narrow contract for a future production decoder backend.
///
/// Decoders receive bounded policy explicitly. A decoder implementation must
/// not own DOM, layout or paint state.
pub trait ImageDecoder:
    Send + Sync
{
    /// Reads intrinsic metadata without requiring a full pixel decode.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError`] for invalid, unsupported or over-budget input.
    fn probe(
        &self,
        bytes: &[u8],
        limits: ImageDecodeLimits,
    ) -> Result<ImageMetadata, ImageError>;

    /// Decodes one image into validated RGBA8 pixels.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError`] for invalid, unsupported or over-budget input.
    fn decode(
        &self,
        bytes: &[u8],
        limits: ImageDecodeLimits,
    ) -> Result<DecodedImage, ImageError>;
}

/// Bounded PNG/JPEG/static-WebP raster decoder used by the image pipeline.
///
/// The decoder is intentionally implemented behind [`ImageDecoder`]. Layout,
/// paint and DOM never depend on the codec library or its image types.
#[derive(Debug, Clone, Copy, Default)]
pub struct RasterImageDecoder;

impl ImageDecoder for RasterImageDecoder {
    fn probe(
        &self,
        bytes: &[u8],
        limits: ImageDecodeLimits,
    ) -> Result<ImageMetadata, ImageError> {
        probe_image(bytes, limits)
    }

    fn decode(
        &self,
        bytes: &[u8],
        limits: ImageDecodeLimits,
    ) -> Result<DecodedImage, ImageError> {
        let metadata = probe_image(bytes, limits)?;

        let format = match metadata.format() {
            ImageFormat::Png => image::ImageFormat::Png,
            ImageFormat::Jpeg => image::ImageFormat::Jpeg,
            ImageFormat::WebP => {
                if webp_is_animated(bytes) {
                    return Err(ImageError::UnsupportedDecodeFormat);
                }
                image::ImageFormat::WebP
            }
            ImageFormat::Gif => return Err(ImageError::UnsupportedDecodeFormat),
        };

        let raster = image::load_from_memory_with_format(bytes, format)
            .map_err(|_| ImageError::DecodeFailed)?
            .to_rgba8();

        let size = IntrinsicSize::new(raster.width(), raster.height())?;
        limits.validate(size)?;

        DecodedImage::from_rgba8(
            size,
            raster.into_raw().into_boxed_slice(),
            limits,
        )
    }
}

/// Image metadata and decode-boundary failures.
#[derive(
    Debug,
    Error,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
pub enum ImageError {
    /// Input does not match a format supported by the current probe.
    #[error("unsupported image format")]
    UnsupportedFormat,

    /// Input ends before the required header or segment is complete.
    #[error("truncated image data")]
    TruncatedData,

    /// Width or height is zero.
    #[error("invalid image dimensions")]
    InvalidDimensions,

    /// Width or height exceeds the configured policy.
    #[error("image dimensions exceed configured limits")]
    DimensionsExceeded,

    /// Pixel count exceeds the configured policy.
    #[error("image pixel count exceeds configured limits")]
    PixelBudgetExceeded,

    /// Decoded memory would exceed the configured policy.
    #[error("decoded image bytes exceed configured limits")]
    DecodedByteBudgetExceeded,

    /// The metadata format is recognized but raster decoding is not enabled.
    #[error("image format is not enabled for raster decode")]
    UnsupportedDecodeFormat,

    /// The codec backend rejected malformed or unsupported image data.
    #[error("image raster decode failed")]
    DecodeFailed,

    /// RGBA8 pixel buffer length does not match the declared dimensions.
    #[error("decoded RGBA8 length does not match image dimensions")]
    InvalidRgbaLength,
}

/// Probes intrinsic dimensions for PNG, GIF, JPEG or WebP without decoding pixels.
///
/// # Errors
///
/// Returns [`ImageError`] when the format is unsupported, malformed or exceeds
/// the supplied decode policy.
pub fn probe_image(
    bytes: &[u8],
    limits: ImageDecodeLimits,
) -> Result<ImageMetadata, ImageError> {
    let metadata =
        if bytes.starts_with(
            b"\x89PNG\r\n\x1a\n",
        ) {
            probe_png(bytes)?
        } else if bytes.starts_with(
            b"GIF87a",
        ) || bytes.starts_with(
            b"GIF89a",
        ) {
            probe_gif(bytes)?
        } else if bytes.starts_with(
            &[0xFF, 0xD8],
        ) {
            probe_jpeg(bytes)?
        } else if bytes.len() >= 12
            && &bytes[0..4] == b"RIFF"
            && &bytes[8..12] == b"WEBP"
        {
            probe_webp(bytes)?
        } else {
            return Err(
                ImageError::UnsupportedFormat,
            );
        };

    limits.validate(metadata.size())?;

    Ok(metadata)
}

fn probe_png(
    bytes: &[u8],
) -> Result<ImageMetadata, ImageError> {
    if bytes.len() < 24 {
        return Err(
            ImageError::TruncatedData,
        );
    }

    if &bytes[12..16] != b"IHDR" {
        return Err(
            ImageError::TruncatedData,
        );
    }

    let width = u32::from_be_bytes([
        bytes[16],
        bytes[17],
        bytes[18],
        bytes[19],
    ]);

    let height = u32::from_be_bytes([
        bytes[20],
        bytes[21],
        bytes[22],
        bytes[23],
    ]);

    let size =
        IntrinsicSize::new(
            width,
            height,
        )?;

    Ok(ImageMetadata::new(
        ImageFormat::Png,
        size,
    ))
}

fn probe_gif(
    bytes: &[u8],
) -> Result<ImageMetadata, ImageError> {
    if bytes.len() < 10 {
        return Err(
            ImageError::TruncatedData,
        );
    }

    let width = u16::from_le_bytes([
        bytes[6],
        bytes[7],
    ]) as u32;

    let height = u16::from_le_bytes([
        bytes[8],
        bytes[9],
    ]) as u32;

    let size =
        IntrinsicSize::new(
            width,
            height,
        )?;

    Ok(ImageMetadata::new(
        ImageFormat::Gif,
        size,
    ))
}

fn webp_is_animated(bytes: &[u8]) -> bool {
    bytes.len() >= 21
        && &bytes[0..4] == b"RIFF"
        && &bytes[8..12] == b"WEBP"
        && &bytes[12..16] == b"VP8X"
        && (bytes[20] & 0x02) != 0
}

fn probe_webp(
    bytes: &[u8],
) -> Result<ImageMetadata, ImageError> {
    if bytes.len() < 30 {
        return Err(ImageError::TruncatedData);
    }

    let chunk = &bytes[12..16];
    let (width, height) = if chunk == b"VP8X" {
        let width = 1_u32.saturating_add(
            u32::from(bytes[24])
                | (u32::from(bytes[25]) << 8)
                | (u32::from(bytes[26]) << 16),
        );
        let height = 1_u32.saturating_add(
            u32::from(bytes[27])
                | (u32::from(bytes[28]) << 8)
                | (u32::from(bytes[29]) << 16),
        );
        (width, height)
    } else if chunk == b"VP8L" {
        if bytes.len() < 25 || bytes[20] != 0x2F {
            return Err(ImageError::TruncatedData);
        }
        let b1 = u32::from(bytes[21]);
        let b2 = u32::from(bytes[22]);
        let b3 = u32::from(bytes[23]);
        let b4 = u32::from(bytes[24]);
        let width = 1 + (b1 | ((b2 & 0x3F) << 8));
        let height = 1 + ((b2 >> 6) | (b3 << 2) | ((b4 & 0x0F) << 10));
        (width, height)
    } else if chunk == b"VP8 " {
        if bytes.len() < 30 || &bytes[23..26] != b"\x9d\x01\x2a" {
            return Err(ImageError::TruncatedData);
        }
        let width = u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3FFF;
        let height = u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3FFF;
        (u32::from(width), u32::from(height))
    } else {
        return Err(ImageError::UnsupportedFormat);
    };

    let size = IntrinsicSize::new(width, height)?;
    Ok(ImageMetadata::new(ImageFormat::WebP, size))
}

fn probe_jpeg(
    bytes: &[u8],
) -> Result<ImageMetadata, ImageError> {
    let mut cursor = 2_usize;

    while cursor < bytes.len() {
        while cursor < bytes.len()
            && bytes[cursor] != 0xFF
        {
            cursor += 1;
        }

        while cursor < bytes.len()
            && bytes[cursor] == 0xFF
        {
            cursor += 1;
        }

        let Some(&marker) =
            bytes.get(cursor)
        else {
            return Err(
                ImageError::TruncatedData,
            );
        };

        cursor += 1;

        if marker == 0xD9
            || marker == 0xDA
        {
            break;
        }

        if marker == 0x01
            || (0xD0..=0xD7)
                .contains(&marker)
        {
            continue;
        }

        let length_end =
            cursor
                .checked_add(2)
                .ok_or(
                    ImageError::TruncatedData,
                )?;

        let length_bytes =
            bytes
                .get(cursor..length_end)
                .ok_or(
                    ImageError::TruncatedData,
                )?;

        let segment_length =
            u16::from_be_bytes([
                length_bytes[0],
                length_bytes[1],
            ]) as usize;

        if segment_length < 2 {
            return Err(
                ImageError::TruncatedData,
            );
        }

        let segment_end =
            cursor
                .checked_add(segment_length)
                .ok_or(
                    ImageError::TruncatedData,
                )?;

        if segment_end > bytes.len() {
            return Err(
                ImageError::TruncatedData,
            );
        }

        if is_sof_marker(marker) {
            if segment_length < 7 {
                return Err(
                    ImageError::TruncatedData,
                );
            }

            let height =
                u16::from_be_bytes([
                    bytes[cursor + 3],
                    bytes[cursor + 4],
                ]) as u32;

            let width =
                u16::from_be_bytes([
                    bytes[cursor + 5],
                    bytes[cursor + 6],
                ]) as u32;

            let size =
                IntrinsicSize::new(
                    width,
                    height,
                )?;

            return Ok(
                ImageMetadata::new(
                    ImageFormat::Jpeg,
                    size,
                ),
            );
        }

        cursor = segment_end;
    }

    Err(ImageError::TruncatedData)
}

fn is_sof_marker(
    marker: u8,
) -> bool {
    matches!(
        marker,
        0xC0
            | 0xC1
            | 0xC2
            | 0xC3
            | 0xC5
            | 0xC6
            | 0xC7
            | 0xC9
            | 0xCA
            | 0xCB
            | 0xCD
            | 0xCE
            | 0xCF
    )
}

#[cfg(test)]
mod tests {
    use super::{
        DecodedImage, ImageDecodeLimits,
        ImageDecoder, ImageError, ImageFormat,
        IntrinsicSize, RasterImageDecoder, probe_image,
    };

    #[test]
    fn probes_png_dimensions() -> Result<
        (),
        ImageError,
    > {
        let mut bytes =
            vec![0_u8; 24];

        bytes[0..8]
            .copy_from_slice(
                b"\x89PNG\r\n\x1a\n",
            );

        bytes[12..16]
            .copy_from_slice(b"IHDR");

        bytes[16..20]
            .copy_from_slice(
                &640_u32.to_be_bytes(),
            );

        bytes[20..24]
            .copy_from_slice(
                &480_u32.to_be_bytes(),
            );

        let metadata =
            probe_image(
                &bytes,
                ImageDecodeLimits::default(),
            )?;

        assert_eq!(
            metadata.format(),
            ImageFormat::Png,
        );

        assert_eq!(
            metadata.size().width(),
            640,
        );

        assert_eq!(
            metadata.size().height(),
            480,
        );

        Ok(())
    }

    #[test]
    fn probes_gif_dimensions() -> Result<
        (),
        ImageError,
    > {
        let mut bytes =
            b"GIF89a".to_vec();

        bytes.extend_from_slice(
            &320_u16.to_le_bytes(),
        );

        bytes.extend_from_slice(
            &200_u16.to_le_bytes(),
        );

        let metadata =
            probe_image(
                &bytes,
                ImageDecodeLimits::default(),
            )?;

        assert_eq!(
            metadata.format(),
            ImageFormat::Gif,
        );

        assert_eq!(
            metadata.size().width(),
            320,
        );

        assert_eq!(
            metadata.size().height(),
            200,
        );

        Ok(())
    }

    #[test]
    fn probes_jpeg_dimensions() -> Result<
        (),
        ImageError,
    > {
        let bytes = [
            0xFF, 0xD8,
            0xFF, 0xC0,
            0x00, 0x11,
            0x08,
            0x01, 0xE0,
            0x02, 0x80,
            0x03,
            0x01, 0x11, 0x00,
            0x02, 0x11, 0x00,
            0x03, 0x11, 0x00,
            0xFF, 0xD9,
        ];

        let metadata =
            probe_image(
                &bytes,
                ImageDecodeLimits::default(),
            )?;

        assert_eq!(
            metadata.format(),
            ImageFormat::Jpeg,
        );

        assert_eq!(
            metadata.size().width(),
            640,
        );

        assert_eq!(
            metadata.size().height(),
            480,
        );

        Ok(())
    }

    #[test]
    fn rejects_pixel_budget_overflow() -> Result<
        (),
        ImageError,
    > {
        let size =
            IntrinsicSize::new(
                100,
                100,
            )?;

        let limits =
            ImageDecodeLimits::new(
                1_000,
                1_000,
                5_000,
                100_000,
            );

        assert_eq!(
            limits.validate(size),
            Err(
                ImageError::PixelBudgetExceeded,
            ),
        );

        Ok(())
    }

    #[test]
    fn validates_decoded_rgba_length() -> Result<
        (),
        ImageError,
    > {
        let size =
            IntrinsicSize::new(
                2,
                2,
            )?;

        let image =
            DecodedImage::from_rgba8(
                size,
                vec![0_u8; 16]
                    .into_boxed_slice(),
                ImageDecodeLimits::default(),
            )?;

        assert_eq!(
            image.rgba8().len(),
            16,
        );

        Ok(())
    }

    #[test]
    fn raster_decoder_rejects_gif_until_enabled() -> Result<(), ImageError> {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());

        let decoder = RasterImageDecoder;
        let result = decoder.decode(&bytes, ImageDecodeLimits::default());

        assert_eq!(result, Err(ImageError::UnsupportedDecodeFormat));

        Ok(())
    }

}
