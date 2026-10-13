//! Phantom 2J-1 Browser Host Foundation I regression corpus.

use phantom_js::{BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value};

fn snapshot(
    url: &str,
    title: &str,
    width: f64,
    height: f64,
    dpr: f64,
) -> Result<BrowserHostSnapshot, phantom_js::RuntimeError> {
    BrowserHostSnapshot::new(url, title, width, height, dpr)
}

#[test]
fn window_document_and_location_are_installed_as_shared_globals()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot(
        "https://example.test/page",
        "Example",
        1024.0,
        768.0,
        2.0,
    )?)?;

    assert_eq!(
        runtime.execute(
            "window === self && window.document === document && \
             window.location === location && document.location === location;",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn document_metadata_and_viewport_are_visible_to_javascript()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot(
        "https://example.test/",
        "Phantom",
        1200.0,
        700.0,
        1.5,
    )?)?;

    assert_eq!(
        runtime.execute("document.title + '|' + document.URL + '|' + document.readyState;")?,
        Value::String("Phantom|https://example.test/|complete".to_owned())
    );
    assert_eq!(
        runtime.execute("window.innerWidth + window.innerHeight;")?,
        Value::Number(1900.0)
    );
    assert_eq!(
        runtime.execute("window.devicePixelRatio;")?,
        Value::Number(1.5)
    );
    Ok(())
}

#[test]
fn host_refresh_preserves_window_document_location_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot(
        "https://first.test/",
        "First",
        800.0,
        600.0,
        1.0,
    )?)?;
    runtime.execute(
        "let firstWindow = window; let firstDocument = document; let firstLocation = location;",
    )?;

    runtime.install_browser_host(&snapshot(
        "https://second.test/",
        "Second",
        1440.0,
        900.0,
        2.0,
    )?)?;

    assert_eq!(
        runtime.execute(
            "firstWindow === window && firstDocument === document && \
             firstLocation === location && document.title === 'Second' && \
             location.href === 'https://second.test/' && window.innerWidth === 1440;",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn browser_globals_cannot_be_reassigned_as_identifiers() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot(
        "https://example.test/",
        "Example",
        800.0,
        600.0,
        1.0,
    )?)?;

    let error = runtime.execute("window = 4;").err();
    assert_eq!(
        error.map(|error| error.kind()),
        Some(RuntimeErrorKind::ConstAssignment)
    );
    Ok(())
}

#[test]
fn document_title_can_be_script_mutated_until_next_host_refresh()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let initial = snapshot("https://example.test/", "Initial", 800.0, 600.0, 1.0)?;
    runtime.install_browser_host(&initial)?;

    assert_eq!(
        runtime.execute("document.title = 'Changed'; document.title;")?,
        Value::String("Changed".to_owned())
    );

    runtime.install_browser_host(&initial)?;
    assert_eq!(
        runtime.execute("document.title;")?,
        Value::String("Initial".to_owned())
    );
    Ok(())
}

#[test]
fn invalid_host_geometry_fails_closed() {
    let error =
        BrowserHostSnapshot::new("https://example.test/", "Example", f64::NAN, 600.0, 1.0).err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(RuntimeErrorKind::InvalidHostSnapshot)
    );
}

#[test]
fn browser_host_text_budgets_are_enforced() {
    let oversized_url = "x".repeat(16 * 1024 + 1);
    let error = BrowserHostSnapshot::new(oversized_url, "Example", 800.0, 600.0, 1.0).err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(RuntimeErrorKind::HostSnapshotLimitExceeded)
    );
}
