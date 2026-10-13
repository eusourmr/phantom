//! Phantom 2K-3 Fetch / Async Resource Foundation I regression corpus.

use phantom_js::{BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value};

fn host() -> Result<BrowserHostSnapshot, phantom_js::RuntimeError> {
    BrowserHostSnapshot::new("https://example.test/app", "Fetch", 1024.0, 768.0, 1.0)
}

#[test]
fn fetch_returns_promise_and_exposes_bounded_host_request() -> Result<(), Box<dyn std::error::Error>>
{
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var settled = false; \
         fetch('/data').then(function() { settled = true; });",
    )?;

    assert_eq!(runtime.execute("settled;")?, Value::Boolean(false));
    assert_eq!(runtime.pending_fetch_count(), 1);
    assert_eq!(
        runtime.drain_fetch_requests(),
        vec![(1, "/data".to_owned())]
    );
    assert_eq!(runtime.pending_fetch_count(), 1);
    Ok(())
}

#[test]
fn successful_fetch_resolves_response_metadata_and_text() -> Result<(), Box<dyn std::error::Error>>
{
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var status = 0; \
         var ok = false; \
         var responseUrl = ''; \
         var body = ''; \
         fetch('/data').then(function(response) { \
             status = response.status; \
             ok = response.ok; \
             responseUrl = response.url; \
             response.text().then(function(text) { body = text; }); \
         });",
    )?;
    let requests = runtime.drain_fetch_requests();
    assert_eq!(requests.len(), 1);

    runtime.complete_fetch_success(
        requests[0].0,
        200,
        "https://example.test/data",
        "hello phantom",
    )?;

    assert_eq!(runtime.execute("status;")?, Value::Number(200.0));
    assert_eq!(runtime.execute("ok;")?, Value::Boolean(true));
    assert_eq!(
        runtime.execute("responseUrl;")?,
        Value::String("https://example.test/data".to_owned())
    );
    assert_eq!(
        runtime.execute("body;")?,
        Value::String("hello phantom".to_owned())
    );
    assert_eq!(runtime.pending_fetch_count(), 0);
    Ok(())
}

#[test]
fn failed_fetch_rejects_and_catch_runs_as_microtask() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var error = ''; \
         fetch('/offline').catch(function(reason) { error = reason; });",
    )?;
    let request = runtime.drain_fetch_requests().remove(0);

    runtime.complete_fetch_failure(request.0, "network unavailable")?;

    assert_eq!(
        runtime.execute("error;")?,
        Value::String("network unavailable".to_owned())
    );
    Ok(())
}

#[test]
fn response_text_is_one_shot_and_body_used_updates() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var responseRef = null; \
         var first = ''; \
         var second = ''; \
         fetch('/body').then(function(response) { \
             responseRef = response; \
             response.text().then(function(text) { first = text; }); \
             response.text().catch(function(reason) { second = reason; }); \
         });",
    )?;
    let request = runtime.drain_fetch_requests().remove(0);
    runtime.complete_fetch_success(request.0, 200, "https://example.test/body", "payload")?;

    assert_eq!(
        runtime.execute("responseRef.bodyUsed;")?,
        Value::Boolean(true)
    );
    assert_eq!(
        runtime.execute("first;")?,
        Value::String("payload".to_owned())
    );
    assert_eq!(
        runtime.execute("second;")?,
        Value::String("response body already consumed".to_owned())
    );
    Ok(())
}

#[test]
fn fetch_rejects_options_and_non_string_url() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    let options_error = match runtime.execute("fetch('/data', 7)") {
        Ok(value) => {
            return Err(std::io::Error::other(format!(
                "unexpected fetch options result: {value:?}"
            ))
            .into());
        }
        Err(error) => error,
    };
    assert_eq!(
        options_error.kind(),
        RuntimeErrorKind::UnsupportedFetchOptions
    );

    let url_error = match runtime.execute("fetch(42);") {
        Ok(value) => {
            return Err(std::io::Error::other(format!(
                "unexpected non-string fetch result: {value:?}"
            ))
            .into());
        }
        Err(error) => error,
    };
    assert_eq!(url_error.kind(), RuntimeErrorKind::InvalidFetchUrl);
    Ok(())
}

#[test]
fn fetch_global_identity_is_shared_with_window() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute("window.fetch === fetch;")?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn unknown_fetch_completion_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    let error = match runtime.complete_fetch_success(999, 200, "https://example.test/", "x") {
        Ok(()) => {
            return Err(
                std::io::Error::other("unexpected success completing unknown fetch id").into(),
            );
        }
        Err(error) => error,
    };
    assert_eq!(error.kind(), RuntimeErrorKind::UnknownFetchRequest);
    Ok(())
}
