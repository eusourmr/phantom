//! Phantom 2G-3 mascot and branding asset validation.
//!
//! The browser keeps the active application icon and opening logo small enough
//! for runtime embedding while retaining transparent 4K masters for branding
//! and future status surfaces.

use image::ImageError;

fn assert_png_asset(
    bytes: &[u8],
    expected_width: u32,
    expected_height: u32,
) -> Result<(), ImageError> {
    let image = image::load_from_memory(bytes)?.to_rgba8();

    assert_eq!(image.width(), expected_width);
    assert_eq!(image.height(), expected_height);
    assert!(image.pixels().any(|pixel| pixel.0[3] < u8::MAX));

    Ok(())
}

#[test]
fn runtime_icon_and_opening_logo_are_valid_transparent_pngs() -> Result<(), ImageError> {
    assert_png_asset(
        include_bytes!("../assets/branding/phantom-app-icon-1024.png"),
        1024,
        1024,
    )?;
    assert_png_asset(
        include_bytes!("../assets/branding/phantom-logo.png"),
        1024,
        1024,
    )?;

    Ok(())
}

#[test]
fn mascot_4k_master_variants_are_valid() -> Result<(), ImageError> {
    assert_png_asset(
        include_bytes!("../assets/branding/mascot/phantom-mascot-main-4096.png"),
        4096,
        4096,
    )?;
    assert_png_asset(
        include_bytes!("../assets/branding/mascot/phantom-mascot-bold-4096.png"),
        4096,
        4096,
    )?;
    assert_png_asset(
        include_bytes!("../assets/branding/mascot/phantom-mascot-flat-4096.png"),
        4096,
        4096,
    )?;

    Ok(())
}

#[test]
fn future_status_mascots_are_valid_transparent_4k_assets() -> Result<(), ImageError> {
    assert_png_asset(
        include_bytes!("../assets/branding/status/phantom-offline-4096.png"),
        4096,
        4096,
    )?;
    assert_png_asset(
        include_bytes!("../assets/branding/status/phantom-404-4096.png"),
        4096,
        4096,
    )?;
    assert_png_asset(
        include_bytes!("../assets/branding/status/phantom-protected-4096.png"),
        4096,
        4096,
    )?;
    assert_png_asset(
        include_bytes!("../assets/branding/phantom-logo-4096.png"),
        4096,
        4096,
    )?;

    Ok(())
}
