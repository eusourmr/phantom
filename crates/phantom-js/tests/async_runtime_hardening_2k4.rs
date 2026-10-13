//! Phantom 2K-4 Async Runtime Hardening / 2K Final Gate.
//!
//! PHANTOM_2K4_ASYNC_RUNTIME_HARDENING_FINAL_GATE

use phantom_js::{BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value};

fn host() -> Result<BrowserHostSnapshot, phantom_js::RuntimeError> {
    BrowserHostSnapshot::new(
        "https://example.test/app",
        "2K Final Gate",
        1024.0,
        768.0,
        1.0,
    )
}

fn runtime_error<T>(
    result: Result<T, phantom_js::RuntimeError>,
    unexpected: &str,
) -> Result<phantom_js::RuntimeError, Box<dyn std::error::Error>>
where
    T: std::fmt::Debug,
{
    match result {
        Ok(value) => Err(std::io::Error::other(format!("{unexpected}: {value:?}")).into()),
        Err(error) => Ok(error),
    }
}

#[test]
fn microtask_checkpoint_rolls_back_partial_microtask_effects()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    let error = runtime_error(
        runtime.execute(
            "var marker = 'sync'; \
             queueMicrotask(function() { \
                 marker = 'partial'; \
                 missingMicrotaskIdentifier; \
             });",
        ),
        "microtask checkpoint unexpectedly succeeded",
    )?;

    assert_eq!(error.kind(), RuntimeErrorKind::ReferenceError);
    assert_eq!(
        runtime.global("marker").cloned(),
        Some(Value::String("sync".to_owned()))
    );
    assert_eq!(runtime.pending_microtask_count(), 1);
    Ok(())
}

#[test]
fn failed_timer_task_preserves_timer_and_runtime_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var marker = 'clean'; \
         var timer = setTimeout(function() { \
             marker = 'dirty'; \
             missingTimerIdentifier; \
         }, 0);",
    )?;

    let error = runtime_error(runtime.advance_time(0), "timer task unexpectedly succeeded")?;

    assert_eq!(error.kind(), RuntimeErrorKind::ReferenceError);
    assert_eq!(
        runtime.global("marker").cloned(),
        Some(Value::String("clean".to_owned()))
    );
    assert_eq!(runtime.pending_timer_count(), 1);
    Ok(())
}

#[test]
fn failed_fetch_completion_is_transactional_and_request_remains_pending()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var marker = 'clean'; \
         fetch('/transaction').then(function() { \
             marker = 'dirty'; \
             missingFetchIdentifier; \
         });",
    )?;
    let requests = runtime.drain_fetch_requests();
    assert_eq!(requests.len(), 1);

    let error = runtime_error(
        runtime.complete_fetch_success(
            requests[0].0,
            200,
            "https://example.test/transaction",
            "payload",
        ),
        "fetch completion unexpectedly succeeded",
    )?;

    assert_eq!(error.kind(), RuntimeErrorKind::ReferenceError);
    assert_eq!(
        runtime.global("marker").cloned(),
        Some(Value::String("clean".to_owned()))
    );
    assert_eq!(runtime.pending_fetch_count(), 1);

    runtime.complete_fetch_failure(requests[0].0, "fallback failure")?;
    assert_eq!(runtime.pending_fetch_count(), 0);
    Ok(())
}

#[test]
fn sync_microtask_fetch_timer_order_is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var order = ''; \
         setTimeout(function() { \
             order = order + 'T'; \
             Promise.resolve(1).then(function() { order = order + 'M2'; }); \
         }, 0); \
         queueMicrotask(function() { order = order + 'M1'; }); \
         fetch('/ordered').then(function() { \
             order = order + 'F'; \
             queueMicrotask(function() { order = order + 'FM'; }); \
         }); \
         order = order + 'S';",
    )?;

    assert_eq!(
        runtime.global("order").cloned(),
        Some(Value::String("SM1".to_owned()))
    );

    let requests = runtime.drain_fetch_requests();
    assert_eq!(requests.len(), 1);
    runtime.complete_fetch_success(requests[0].0, 204, "https://example.test/ordered", "")?;
    assert_eq!(
        runtime.global("order").cloned(),
        Some(Value::String("SM1FFM".to_owned()))
    );

    assert_eq!(runtime.advance_time(0)?, 1);
    assert_eq!(
        runtime.global("order").cloned(),
        Some(Value::String("SM1FFMTM2".to_owned()))
    );
    Ok(())
}

#[test]
fn oversized_fetch_body_fails_without_consuming_pending_request()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute("fetch('/large');")?;
    let requests = runtime.drain_fetch_requests();
    assert_eq!(requests.len(), 1);

    let oversized = "x".repeat((1024 * 1024) + 1);
    let error = runtime_error(
        runtime.complete_fetch_success(requests[0].0, 200, "https://example.test/large", oversized),
        "oversized fetch body unexpectedly succeeded",
    )?;

    assert_eq!(error.kind(), RuntimeErrorKind::FetchResponseLimitExceeded);
    assert_eq!(runtime.pending_fetch_count(), 1);

    runtime.complete_fetch_failure(requests[0].0, "body rejected by host")?;
    assert_eq!(runtime.pending_fetch_count(), 0);
    Ok(())
}

#[test]
fn duplicate_fetch_completion_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var status = 0; \
         fetch('/once').then(function(response) { status = response.status; });",
    )?;
    let requests = runtime.drain_fetch_requests();
    assert_eq!(requests.len(), 1);

    runtime.complete_fetch_success(requests[0].0, 200, "https://example.test/once", "done")?;
    assert_eq!(
        runtime.global("status").cloned(),
        Some(Value::Number(200.0))
    );

    let error = runtime_error(
        runtime.complete_fetch_success(
            requests[0].0,
            200,
            "https://example.test/once",
            "duplicate",
        ),
        "duplicate fetch completion unexpectedly succeeded",
    )?;
    assert_eq!(error.kind(), RuntimeErrorKind::UnknownFetchRequest);
    Ok(())
}

#[test]
fn pending_fetch_budget_fails_closed_at_257th_request() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "for (var i = 0; i < 256; i = i + 1) { \
             fetch('/bounded'); \
         }",
    )?;
    assert_eq!(runtime.pending_fetch_count(), 256);

    let error = runtime_error(
        runtime.execute("fetch('/overflow');"),
        "257th pending fetch unexpectedly succeeded",
    )?;
    assert_eq!(error.kind(), RuntimeErrorKind::FetchRequestLimitExceeded);
    assert_eq!(runtime.pending_fetch_count(), 256);
    Ok(())
}
