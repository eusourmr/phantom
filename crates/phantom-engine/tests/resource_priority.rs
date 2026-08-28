//! Executable 2C-9 slice for image fetch priority and preload discovery.

use phantom_engine::{Engine, ImageLoading, ResourcePriority};

#[test]
fn image_fetchpriority_is_normalized() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"
        <img src="high.png" fetchpriority="HIGH">
        <img src="auto.png" fetchpriority="unexpected">
        <img src="low.png" fetchpriority="low" loading="lazy">
        "#,
    )?;

    let requests = engine.image_requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].priority(), ResourcePriority::High);
    assert_eq!(requests[1].priority(), ResourcePriority::Auto);
    assert_eq!(requests[2].priority(), ResourcePriority::Low);
    assert_eq!(requests[2].loading(), ImageLoading::Lazy);

    Ok(())
}

#[test]
fn image_preload_discovers_href_and_priority() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html(
        r#"
        <link rel="stylesheet preload" as="image" href="hero.webp" fetchpriority="high">
        <link rel="preload" as="style" href="ignored.css">
        "#,
    )?;

    let preloads = engine.image_preload_requests();
    assert_eq!(preloads.len(), 1);
    assert_eq!(preloads[0].source(), "hero.webp");
    assert_eq!(preloads[0].priority(), ResourcePriority::High);

    Ok(())
}

#[test]
fn image_preload_selects_imagesrcset_for_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html_with_viewport(
        r#"
        <link
            rel="preload"
            as="image"
            href="fallback.png"
            imagesrcset="small.png 1x, large.png 2x"
            fetchpriority="auto">
        "#,
        800.0,
    )?;

    let preloads = engine.image_preload_requests_for_device(2.0);
    assert_eq!(preloads.len(), 1);
    assert_eq!(preloads[0].source(), "large.png");

    Ok(())
}

#[test]
fn image_preload_respects_simple_media_and_supported_type() -> Result<(), Box<dyn std::error::Error>>
{
    let mut engine = Engine::new();
    engine.load_html_with_viewport(
        r#"
        <link rel="preload" as="image" href="wide.webp" media="(min-width: 900px)" type="image/webp">
        <link rel="preload" as="image" href="narrow.png" media="(max-width: 800px)" type="image/png">
        <link rel="preload" as="image" href="unsupported.avif" type="image/avif">
        "#,
        720.0,
    )?;

    let preloads = engine.image_preload_requests();
    assert_eq!(preloads.len(), 1);
    assert_eq!(preloads[0].source(), "narrow.png");

    Ok(())
}
