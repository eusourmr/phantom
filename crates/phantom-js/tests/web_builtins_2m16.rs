//! Phantom 2M-16 web globals / core built-ins Foundation I regression corpus.

use phantom_js::{BrowserHostSnapshot, Runtime, Value};

fn browser_runtime() -> Result<Runtime, Box<dyn std::error::Error>> {
    let snapshot = BrowserHostSnapshot::new(
        "https://builtins.test/",
        "Web globals builtins",
        800.0,
        600.0,
        1.0,
    )?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn math_slice_supports_common_numeric_operations_and_constants()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "Math.abs(-4) + '|' + Math.floor(3.9) + '|' + Math.ceil(3.1) + '|' + Math.round(4.5) + '|' + Math.min(7, 2, 9) + '|' + Math.max(7, 2, 9) + '|' + Math.pow(2, 3) + '|' + Math.sqrt(81);",
    )?;
    assert_eq!(value, Value::String("4|3|4|5|2|9|8|9".to_owned()));

    let pi = runtime.execute("Math.PI > 3 && Math.PI < 4;")?;
    assert_eq!(pi, Value::Boolean(true));
    Ok(())
}

#[test]
fn parsing_globals_handle_numeric_prefixes_radix_and_nan()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "parseInt('42px', 10) + '|' + parseInt('0xff', 0) + '|' + parseFloat(' -12.5rem') + '|' + isNaN('not-a-number') + '|' + isNaN('12');",
    )?;
    assert_eq!(value, Value::String("42|255|-12.5|true|false".to_owned()));

    let invalid = runtime.execute("parseInt('xyz', 10);")?;
    assert!(matches!(invalid, Value::Number(value) if value.is_nan()));
    Ok(())
}

#[test]
fn array_and_number_static_checks_keep_non_coercing_contract()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "Array.isArray([1, 2]) + '|' + Array.isArray({a: 1}) + '|' + Number.isFinite(12) + '|' + Number.isFinite('12') + '|' + Number.isFinite(Infinity);",
    )?;
    assert_eq!(value, Value::String("true|false|true|false|false".to_owned()));
    Ok(())
}

#[test]
fn console_and_builtin_object_identity_are_stable_across_host_refresh()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let first = runtime.execute(
        "console.log('phantom') === undefined && console.info('i') === undefined && console.warn('w') === undefined && console.error('e') === undefined && console === window.console && Math === window.Math;",
    )?;
    assert_eq!(first, Value::Boolean(true));

    let snapshot = BrowserHostSnapshot::new(
        "https://builtins.test/next",
        "Web globals builtins refresh",
        1024.0,
        768.0,
        1.0,
    )?;
    runtime.install_browser_host(&snapshot)?;
    let second = runtime.execute(
        "console === window.console && Math === window.Math && Array === window.Array && Number === window.Number;",
    )?;
    assert_eq!(second, Value::Boolean(true));
    Ok(())
}

#[test]
fn nan_and_infinity_globals_are_exposed_without_string_coercion()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "isNaN(NaN) && Number.isFinite(Infinity) === false && Math.max() === -Infinity && Math.min() === Infinity;",
    )?;
    assert_eq!(value, Value::Boolean(true));
    Ok(())
}
