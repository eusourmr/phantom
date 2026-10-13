//! Phantom 2K-2 Microtask Queue / Promise Foundation I regression corpus.

use std::collections::BTreeMap;

use phantom_js::{
    BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value,
};

fn host() -> Result<BrowserHostSnapshot, phantom_js::RuntimeError> {
    let target = BrowserDomElementSnapshot::new(
        7,
        "p",
        BTreeMap::from([("id".to_owned(), "target".to_owned())]),
        "Before",
    )?
    .with_parent_node_id(Some(1))
    .with_connected(true);

    BrowserHostSnapshot::new("https://example.test/", "Microtasks", 1024.0, 768.0, 1.0)?
        .with_dom_elements(vec![target])
}

#[test]
fn queue_microtask_runs_after_sync_job_before_execute_returns()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "var order = ''; \
             queueMicrotask(function() { order = order + 'M'; }); \
             order = order + 'S'; \
             order;",
        )?,
        Value::String("S".to_owned())
    );
    assert_eq!(runtime.execute("order;")?, Value::String("SM".to_owned()));
    assert_eq!(runtime.pending_microtask_count(), 0);
    Ok(())
}

#[test]
fn microtasks_run_before_ready_timer_tasks() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var order = ''; \
         setTimeout(function() { order = order + 'T'; }, 0); \
         queueMicrotask(function() { order = order + 'M'; }); \
         order = order + 'S';",
    )?;

    assert_eq!(runtime.execute("order;")?, Value::String("SM".to_owned()));
    assert_eq!(runtime.advance_time(0)?, 1);
    assert_eq!(runtime.execute("order;")?, Value::String("SMT".to_owned()));
    Ok(())
}

#[test]
fn promise_resolve_then_chain_runs_in_microtask_checkpoint()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var result = 0; \
         Promise.resolve(3) \
             .then(function(value) { return value + 4; }) \
             .then(function(value) { result = value; });",
    )?;

    assert_eq!(runtime.execute("result;")?, Value::Number(7.0));
    Ok(())
}

#[test]
fn promise_reject_catch_recovers_reason() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var recovered = ''; \
         Promise.reject('failure').catch(function(reason) { \
             recovered = reason; \
             return 'ok'; \
         });",
    )?;

    assert_eq!(
        runtime.execute("recovered;")?,
        Value::String("failure".to_owned())
    );
    Ok(())
}

#[test]
fn timer_microtasks_checkpoint_before_next_timer() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var order = ''; \
         setTimeout(function() { \
             order = order + 'A'; \
             Promise.resolve(1).then(function() { order = order + 'M'; }); \
         }, 0); \
         setTimeout(function() { order = order + 'B'; }, 0);",
    )?;

    assert_eq!(runtime.advance_time(0)?, 2);
    assert_eq!(runtime.execute("order;")?, Value::String("AMB".to_owned()));
    Ok(())
}

#[test]
fn promise_microtask_can_emit_dom_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "Promise.resolve('After').then(function(value) { \
             document.getElementById('target').textContent = value; \
         });",
    )?;

    let mutations = runtime.drain_dom_mutations();
    assert_eq!(mutations.len(), 1);
    Ok(())
}

#[test]
fn promise_identity_and_browser_globals_are_stable() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let promise = Promise.resolve(1); \
             Promise.resolve(promise) === promise && \
             window.Promise === Promise && \
             window.queueMicrotask === queueMicrotask;",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn queue_microtask_rejects_non_callable_callback() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    let error = match runtime.execute("queueMicrotask('code');") {
        Ok(value) => {
            return Err(
                std::io::Error::other(format!("unexpected microtask result: {value:?}")).into(),
            );
        }
        Err(error) => error,
    };

    assert_eq!(error.kind(), RuntimeErrorKind::InvalidMicrotaskCallback);
    Ok(())
}
