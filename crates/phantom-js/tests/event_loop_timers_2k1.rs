//! Phantom 2K-1 Event Loop Foundation / Timers I regression corpus.

use std::collections::BTreeMap;

use phantom_js::{
    BrowserDomElementSnapshot, BrowserDomMutation, BrowserHostSnapshot, Runtime, RuntimeErrorKind,
    Value,
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

    BrowserHostSnapshot::new("https://example.test/", "Timers", 1024.0, 768.0, 1.0)?
        .with_dom_elements(vec![target])
}

#[test]
fn set_timeout_is_deferred_until_virtual_deadline() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "var fired = 0; \
             setTimeout(function() { fired = 1; }, 10); \
             fired;",
        )?,
        Value::Number(0.0)
    );
    assert_eq!(runtime.pending_timer_count(), 1);
    assert_eq!(runtime.advance_time(9)?, 0);
    assert_eq!(runtime.execute("fired;")?, Value::Number(0.0));
    assert_eq!(runtime.advance_time(1)?, 1);
    assert_eq!(runtime.execute("fired;")?, Value::Number(1.0));
    assert_eq!(runtime.pending_timer_count(), 0);
    Ok(())
}

#[test]
fn same_deadline_timers_execute_in_registration_order() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var order = ''; \
         setTimeout(function() { order = order + 'A'; }, 5); \
         setTimeout(function() { order = order + 'B'; }, 5);",
    )?;

    assert_eq!(runtime.advance_time(5)?, 2);
    assert_eq!(runtime.execute("order;")?, Value::String("AB".to_owned()));
    Ok(())
}

#[test]
fn clear_timeout_cancels_one_shot_task() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var fired = 0; \
         var timer = setTimeout(function() { fired = 1; }, 5); \
         clearTimeout(timer);",
    )?;

    assert_eq!(runtime.pending_timer_count(), 0);
    assert_eq!(runtime.advance_time(10)?, 0);
    assert_eq!(runtime.execute("fired;")?, Value::Number(0.0));
    Ok(())
}

#[test]
fn zero_delay_timer_runs_only_when_host_pumps_time() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var fired = 0; \
         setTimeout(function() { fired = 1; }, -50);",
    )?;

    assert_eq!(runtime.execute("fired;")?, Value::Number(0.0));
    assert_eq!(runtime.advance_time(0)?, 1);
    assert_eq!(runtime.execute("fired;")?, Value::Number(1.0));
    Ok(())
}

#[test]
fn timer_callback_can_emit_live_dom_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "setTimeout(function() { \
             document.getElementById('target').textContent = 'After'; \
         }, 4);",
    )?;
    assert!(runtime.drain_dom_mutations().is_empty());

    assert_eq!(runtime.advance_time(4)?, 1);
    assert_eq!(
        runtime.drain_dom_mutations(),
        vec![BrowserDomMutation::SetTextContent {
            node_id: 7,
            value: "After".to_owned(),
        }]
    );
    Ok(())
}

#[test]
fn timer_functions_are_shared_between_window_and_global_scope()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "window.setTimeout === setTimeout && \
             window.clearTimeout === clearTimeout && \
             document.getElementById('target').remove === \
             document.getElementById('target').remove;",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn non_function_timer_callback_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    let error = match runtime.execute("setTimeout('code', 0);") {
        Ok(value) => {
            return Err(
                std::io::Error::other(format!("unexpected timer result: {value:?}")).into(),
            );
        }
        Err(error) => error,
    };

    assert_eq!(error.kind(), RuntimeErrorKind::InvalidTimerCallback);
    assert_eq!(runtime.pending_timer_count(), 0);
    Ok(())
}
