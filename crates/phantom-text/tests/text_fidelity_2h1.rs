//! Phantom 2H-1 text/font fidelity regression corpus.

use phantom_text::{FallbackTextShaper, FontFamily, FontSlant, FontWeight, TextShaper, TextStyle};

fn sans(size: f32) -> TextStyle {
    TextStyle::new(
        size,
        FontFamily::SansSerif,
        FontWeight::Normal,
        FontSlant::Normal,
    )
}

#[test]
fn combining_marks_do_not_consume_horizontal_advance() {
    let shaper = FallbackTextShaper;
    let style = sans(16.0);

    let composed = shaper.measure("é", style).advance();
    let decomposed = shaper.measure("e\u{301}", style).advance();
    let base = shaper.measure("e", style).advance();

    assert!((decomposed - base).abs() <= f32::EPSILON);
    assert!((composed - base).abs() <= f32::EPSILON);
}

#[test]
fn latin_letters_with_diacritics_follow_lowercase_width_class() {
    let shaper = FallbackTextShaper;
    let style = sans(16.0);

    let plain = shaper.measure("a", style).advance();
    let accented = shaper.measure("á", style).advance();

    assert!((plain - accented).abs() <= f32::EPSILON);
}

#[test]
fn zero_width_format_scalars_remain_zero_advance() {
    let shaper = FallbackTextShaper;
    let style = sans(16.0);

    for scalar in ['\u{200B}', '\u{200C}', '\u{200D}', '\u{FE0F}'] {
        assert!(shaper.measure(&scalar.to_string(), style).advance().abs() <= f32::EPSILON);
    }
}

#[test]
fn measurement_and_shape_stay_identical_for_portuguese_text() {
    let shaper = FallbackTextShaper;
    let style = sans(18.0);
    let source = "Navegação, notícias e informação";

    let measurement = shaper.measure(source, style);
    let shaped = shaper.shape(source, style);

    assert!((measurement.advance() - shaped.advance()).abs() <= f32::EPSILON);
}
