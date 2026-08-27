//! Renderer-independent font metrics and text shaping boundary for Phantom.
//!
//! The layout engine must not know how a concrete font backend, operating
//! system API, or GPU renderer shapes text. This crate defines that wall.
//!
//! The first backend, [`FallbackTextShaper`], is intentionally small and
//! deterministic. It provides stable metrics while Phantom's own layout,
//! line-breaking, paint, and compositor architecture matures. It is not
//! presented as standards-complete font shaping.
//!
//! A future production backend can implement [`TextShaper`] without changing
//! the layout contract.

#![forbid(unsafe_code)]

/// Coarse font-family class requested by layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    /// Sans-serif text.
    SansSerif,

    /// Monospace text.
    Monospace,
}

/// Coarse font weight requested by layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    /// Normal text.
    Normal,

    /// Bold text.
    Bold,
}

/// Font posture requested by layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSlant {
    /// Upright text.
    Normal,

    /// Italic or oblique text.
    Italic,
}

/// Renderer-independent font request used for measuring and shaping text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    font_size: f32,
    family: FontFamily,
    weight: FontWeight,
    slant: FontSlant,
}

impl TextStyle {
    /// Creates a text-style request.
    #[must_use]
    pub fn new(font_size: f32, family: FontFamily, weight: FontWeight, slant: FontSlant) -> Self {
        let font_size = if font_size.is_finite() {
            font_size.max(1.0)
        } else {
            16.0
        };

        Self {
            font_size,
            family,
            weight,
            slant,
        }
    }

    /// Returns the requested logical font size.
    #[must_use]
    pub const fn font_size(self) -> f32 {
        self.font_size
    }

    /// Returns the requested coarse font family.
    #[must_use]
    pub const fn family(self) -> FontFamily {
        self.family
    }

    /// Returns the requested font weight.
    #[must_use]
    pub const fn weight(self) -> FontWeight {
        self.weight
    }

    /// Returns the requested font posture.
    #[must_use]
    pub const fn slant(self) -> FontSlant {
        self.slant
    }
}

/// Vertical metrics for one font request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    ascent: f32,
    descent: f32,
    line_gap: f32,
}

impl FontMetrics {
    /// Creates vertical font metrics.
    #[must_use]
    pub const fn new(ascent: f32, descent: f32, line_gap: f32) -> Self {
        Self {
            ascent,
            descent,
            line_gap,
        }
    }

    /// Returns the distance above the baseline.
    #[must_use]
    pub const fn ascent(self) -> f32 {
        self.ascent
    }

    /// Returns the distance below the baseline.
    #[must_use]
    pub const fn descent(self) -> f32 {
        self.descent
    }

    /// Returns additional leading between adjacent lines.
    #[must_use]
    pub const fn line_gap(self) -> f32 {
        self.line_gap
    }

    /// Returns the total default line height.
    #[must_use]
    pub fn line_height(self) -> f32 {
        self.ascent + self.descent + self.line_gap
    }
}

/// Allocation-free measurement result used by line breaking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMeasurement {
    advance: f32,
    metrics: FontMetrics,
}

impl TextMeasurement {
    /// Creates one text-measurement result.
    #[must_use]
    pub const fn new(advance: f32, metrics: FontMetrics) -> Self {
        Self { advance, metrics }
    }

    /// Returns the horizontal advance.
    #[must_use]
    pub const fn advance(self) -> f32 {
        self.advance
    }

    /// Returns the vertical metrics used by this measurement.
    #[must_use]
    pub const fn metrics(self) -> FontMetrics {
        self.metrics
    }
}

/// Backend-neutral glyph identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphId(u32);

impl GlyphId {
    /// Creates a glyph identifier from a backend value.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the backend-neutral numeric value.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

/// One glyph produced by a shaping backend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    glyph_id: GlyphId,
    cluster: u32,
    advance: f32,
    x_offset: f32,
    y_offset: f32,
}

impl ShapedGlyph {
    /// Creates one shaped glyph.
    #[must_use]
    pub const fn new(
        glyph_id: GlyphId,
        cluster: u32,
        advance: f32,
        x_offset: f32,
        y_offset: f32,
    ) -> Self {
        Self {
            glyph_id,
            cluster,
            advance,
            x_offset,
            y_offset,
        }
    }

    /// Returns the backend-neutral glyph identifier.
    #[must_use]
    pub const fn glyph_id(self) -> GlyphId {
        self.glyph_id
    }

    /// Returns the UTF-8 byte cluster offset into the source run.
    #[must_use]
    pub const fn cluster(self) -> u32 {
        self.cluster
    }

    /// Returns the horizontal advance.
    #[must_use]
    pub const fn advance(self) -> f32 {
        self.advance
    }

    /// Returns the horizontal shaping offset.
    #[must_use]
    pub const fn x_offset(self) -> f32 {
        self.x_offset
    }

    /// Returns the vertical shaping offset.
    #[must_use]
    pub const fn y_offset(self) -> f32 {
        self.y_offset
    }
}

/// Complete shaped result for one source text run.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedRun {
    glyphs: Box<[ShapedGlyph]>,
    advance: f32,
    metrics: FontMetrics,
}

impl ShapedRun {
    /// Creates one shaped run.
    #[must_use]
    pub fn new(glyphs: Box<[ShapedGlyph]>, advance: f32, metrics: FontMetrics) -> Self {
        Self {
            glyphs,
            advance,
            metrics,
        }
    }

    /// Returns shaped glyphs in visual source order for this fallback stage.
    #[must_use]
    pub fn glyphs(&self) -> &[ShapedGlyph] {
        &self.glyphs
    }

    /// Returns the total horizontal advance.
    #[must_use]
    pub const fn advance(&self) -> f32 {
        self.advance
    }

    /// Returns vertical metrics for the run.
    #[must_use]
    pub const fn metrics(&self) -> FontMetrics {
        self.metrics
    }
}

/// Contract implemented by all Phantom text backends.
///
/// Implementations must be safe to call from parallel layout workers in the
/// future, so the boundary requires `Send + Sync`.
pub trait TextShaper: Send + Sync {
    /// Returns vertical metrics for a style without shaping source text.
    fn font_metrics(&self, style: TextStyle) -> FontMetrics;

    /// Measures source text without requiring allocation of a glyph vector.
    ///
    /// Layout uses this fast path for line breaking.
    fn measure(&self, text: &str, style: TextStyle) -> TextMeasurement;

    /// Shapes source text into backend-neutral glyph records.
    ///
    /// The current layout stage does not retain these glyphs yet. This method
    /// defines the stable boundary required by the future glyph-paint stage.
    fn shape(&self, text: &str, style: TextStyle) -> ShapedRun;
}

/// Lightweight deterministic text backend used until real font shaping lands.
///
/// It performs no operating-system calls and owns no font database. Glyph IDs
/// are currently derived from Unicode scalar values solely so the shaping
/// contract can be exercised end-to-end.
#[derive(Debug, Clone, Copy, Default)]
pub struct FallbackTextShaper;

impl TextShaper for FallbackTextShaper {
    fn font_metrics(&self, style: TextStyle) -> FontMetrics {
        let size = style.font_size();

        FontMetrics::new(size * 0.80, size * 0.20, size * 0.25)
    }

    fn measure(&self, text: &str, style: TextStyle) -> TextMeasurement {
        let advance = text
            .chars()
            .map(|character| glyph_advance(character, style))
            .sum();

        TextMeasurement::new(advance, self.font_metrics(style))
    }

    fn shape(&self, text: &str, style: TextStyle) -> ShapedRun {
        let metrics = self.font_metrics(style);
        let mut glyphs = Vec::with_capacity(text.chars().count());
        let mut advance = 0.0;

        for (byte_offset, character) in text.char_indices() {
            let glyph_advance = glyph_advance(character, style);
            let cluster = u32::try_from(byte_offset).map_or(u32::MAX, |value| value);

            glyphs.push(ShapedGlyph::new(
                GlyphId::new(character as u32),
                cluster,
                glyph_advance,
                0.0,
                0.0,
            ));

            advance += glyph_advance;
        }

        ShapedRun::new(glyphs.into_boxed_slice(), advance, metrics)
    }
}

fn glyph_advance(character: char, style: TextStyle) -> f32 {
    let family_factor = match style.family() {
        FontFamily::Monospace => 0.62,

        FontFamily::SansSerif => {
            if character.is_whitespace() {
                0.33
            } else if matches!(
                character,
                'i' | 'l' | 'I' | '.' | ',' | '\'' | '!' | '|' | ':' | ';'
            ) {
                0.29
            } else if matches!(character, 'M' | 'W' | '@' | '#' | '%' | '&') {
                0.82
            } else if character.is_ascii_uppercase() {
                0.63
            } else if character.is_ascii_digit() {
                0.56
            } else if character.is_ascii_lowercase() {
                0.52
            } else if character as u32 >= 0x2E80 {
                1.0
            } else {
                0.58
            }
        }
    };

    let weight_factor = match style.weight() {
        FontWeight::Normal => 1.0,
        FontWeight::Bold => 1.035,
    };

    let slant_factor = match style.slant() {
        FontSlant::Normal => 1.0,
        FontSlant::Italic => 1.01,
    };

    family_factor * style.font_size() * weight_factor * slant_factor
}

#[cfg(test)]
mod tests {
    use super::{FallbackTextShaper, FontFamily, FontSlant, FontWeight, TextShaper, TextStyle};

    #[test]
    fn measurement_and_shape_share_the_same_advance() {
        let shaper = FallbackTextShaper;

        let style = TextStyle::new(
            16.0,
            FontFamily::SansSerif,
            FontWeight::Normal,
            FontSlant::Normal,
        );

        let measurement = shaper.measure("Phantom", style);

        let shaped = shaper.shape("Phantom", style);

        assert!((measurement.advance() - shaped.advance()).abs() < f32::EPSILON);
    }

    #[test]
    fn monospace_characters_have_equal_advance() {
        let shaper = FallbackTextShaper;

        let style = TextStyle::new(
            16.0,
            FontFamily::Monospace,
            FontWeight::Normal,
            FontSlant::Normal,
        );

        let narrow = shaper.measure("i", style);
        let wide = shaper.measure("W", style);

        assert!((narrow.advance() - wide.advance()).abs() < f32::EPSILON);
    }

    #[test]
    fn utf8_clusters_use_source_byte_offsets() {
        let shaper = FallbackTextShaper;

        let style = TextStyle::new(
            16.0,
            FontFamily::SansSerif,
            FontWeight::Normal,
            FontSlant::Normal,
        );

        let shaped = shaper.shape("éa", style);

        assert_eq!(shaped.glyphs().len(), 2);
        assert_eq!(shaped.glyphs()[0].cluster(), 0);
        assert_eq!(shaped.glyphs()[1].cluster(), 2);
    }

    #[test]
    fn font_metrics_produce_positive_line_height() {
        let shaper = FallbackTextShaper;

        let style = TextStyle::new(
            16.0,
            FontFamily::SansSerif,
            FontWeight::Bold,
            FontSlant::Italic,
        );

        assert!(shaper.font_metrics(style).line_height() > 0.0);
    }
}
