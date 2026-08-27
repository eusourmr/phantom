//! Windows resource compiler for the Phantom browser.
//!
//! This build script embeds Phantom branding and executable metadata into the
//! Windows binary. The executable icon is intentionally independent from the
//! transparent logo used inside browser pages.

#![forbid(unsafe_code)]

#[cfg(target_os = "windows")]
fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=assets/branding/phantom.ico");

    let mut resource = winresource::WindowsResource::new();

    resource
        .set_icon("assets/branding/phantom.ico")
        .set("ProductName", "Phantom")
        .set("FileDescription", "Phantom Independent Web Browser")
        .set("InternalName", "phantom-browser")
        .set("OriginalFilename", "phantom-browser.exe")
        .set("CompanyName", "Phantom")
        .set("LegalCopyright", "Phantom contributors");

    resource.compile()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    println!("cargo:rerun-if-changed=assets/branding/phantom.ico");
}
