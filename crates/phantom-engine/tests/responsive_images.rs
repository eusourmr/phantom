//! Executable responsive-image selection tests for Phantom 2C-3.
//!
//! These tests cover the current WHATWG-aligned slice without claiming full
//! responsive-images conformance.

use phantom_engine::{Engine, EngineError};

#[test]
fn selects_density_candidate_for_device_pixel_ratio() -> Result<(), EngineError> {
    let mut engine = Engine::new();
    engine.load_html(r#"<img src="fallback.jpg" srcset="one.jpg 1x, two.jpg 2x" alt="hero">"#)?;

    let requests = engine.image_requests_for_device(2.0);
    assert_eq!(
        requests.first().map(|request| request.source()),
        Some("two.jpg")
    );
    Ok(())
}

#[test]
fn selects_width_candidate_from_sizes_slot() -> Result<(), EngineError> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport(
        r#"<img src="fallback.jpg" srcset="small.jpg 400w, large.jpg 800w" sizes="400px">"#,
        1_024.0,
    )?;

    let requests = engine.image_requests_for_device(2.0);
    assert_eq!(
        requests.first().map(|request| request.source()),
        Some("large.jpg")
    );
    Ok(())
}

#[test]
fn picture_uses_first_matching_source_before_img() -> Result<(), EngineError> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport(
        r#"<picture>
              <source media="(max-width: 700px)" type="image/webp" srcset="mobile.webp 1x">
              <source type="image/jpeg" srcset="desktop.jpg 1x">
              <img src="fallback.jpg" alt="hero">
           </picture>"#,
        640.0,
    )?;

    let requests = engine.image_requests_for_device(1.0);
    assert_eq!(
        requests.first().map(|request| request.source()),
        Some("mobile.webp")
    );
    Ok(())
}

#[test]
fn picture_falls_back_to_img_when_media_does_not_match() -> Result<(), EngineError> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport(
        r#"<picture>
              <source media="(max-width: 500px)" srcset="mobile.webp 1x">
              <img src="fallback.jpg" alt="hero">
           </picture>"#,
        900.0,
    )?;

    let requests = engine.image_requests_for_device(1.0);
    assert_eq!(
        requests.first().map(|request| request.source()),
        Some("fallback.jpg")
    );
    Ok(())
}
