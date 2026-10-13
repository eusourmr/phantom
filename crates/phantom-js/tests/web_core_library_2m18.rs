//! Phantom 2M-18 web core library III regression corpus.

use phantom_js::{BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value};

fn browser_runtime() -> Result<Runtime, Box<dyn std::error::Error>> {
    let snapshot = BrowserHostSnapshot::new(
        "https://core-library-iii.test/",
        "Web core library III",
        800.0,
        600.0,
        1.0,
    )?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn object_entries_and_has_own_cover_common_enumeration_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "var e = Object.entries({b: 2, a: 1}); e[0].join(':') + '|' + e[1].join(':') + '|' + Object.hasOwn({x: 1}, 'x') + '|' + Object.hasOwn({x: 1}, 'y');",
    )?;
    assert_eq!(value, Value::String("a:1|b:2|true|false".to_owned()));
    Ok(())
}

#[test]
fn array_iteration_methods_bind_callbacks_and_this_arg() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "var a = [1, 2, 3]; var seen = ''; a.forEach(function(v, i, source) { seen = seen + v + ':' + i + ':' + (source === a) + ';'; }); var ctx = {factor: 3}; var mapped = a.map(function(v) { return v * this.factor; }, ctx); seen + '|' + mapped.join(',');",
    )?;
    assert_eq!(
        value,
        Value::String("1:0:true;2:1:true;3:2:true;|3,6,9".to_owned())
    );
    Ok(())
}

#[test]
fn array_filter_predicates_find_and_reduce_are_deterministic()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "var a = [1, 2, 3, 4]; a.filter(function(v) { return v % 2 === 0; }).join(',') + '|' + a.some(function(v) { return v === 4; }) + '|' + a.every(function(v) { return v > 0; }) + '|' + a.find(function(v) { return v > 2; }) + '|' + a.findIndex(function(v) { return v === 3; }) + '|' + a.reduce(function(acc, v) { return acc + v; }, 0);",
    )?;
    assert_eq!(value, Value::String("2,4|true|true|3|2|10".to_owned()));
    Ok(())
}

#[test]
fn string_split_and_string_replace_cover_common_text_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "var s = 'alpha,beta,gamma'; s.split(',').slice(1).join('|') + '|' + s.replace('beta', 'B') + '|' + 'abc'.split('').join('-');",
    )?;
    assert_eq!(
        value,
        Value::String("beta|gamma|alpha,B,gamma|a-b-c".to_owned())
    );
    Ok(())
}

#[test]
fn callback_and_empty_reduce_errors_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    match runtime.execute("[1].map(7);") {
        Err(error) => assert_eq!(error.kind(), RuntimeErrorKind::CallNonFunction),
        Ok(value) => panic!("non-callable map callback unexpectedly returned {value:?}"),
    }
    match runtime.execute("[].reduce(function(acc, value) { return acc + value; });") {
        Err(error) => assert_eq!(error.kind(), RuntimeErrorKind::ReduceEmptyArray),
        Ok(value) => panic!("empty reduce unexpectedly returned {value:?}"),
    }
    Ok(())
}
