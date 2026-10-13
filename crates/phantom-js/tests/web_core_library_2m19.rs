//! Phantom 2M-19 web core library final-gate regression corpus.

use phantom_js::{BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value};

fn browser_snapshot() -> Result<BrowserHostSnapshot, Box<dyn std::error::Error>> {
    Ok(BrowserHostSnapshot::new(
        "https://core-library-final.test/",
        "Web core library final",
        800.0,
        600.0,
        1.0,
    )?)
}

fn browser_runtime() -> Result<Runtime, Box<dyn std::error::Error>> {
    let snapshot = browser_snapshot()?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn array_front_back_concat_reverse_and_at_cover_common_mutation_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "var a = [2, 3]; var n = a.unshift(0, 1); var first = a.shift(); var c = a.concat([4, 5], 6); c.reverse(); c.join(',') + '|' + c.at(-1) + '|' + first + '|' + n + '|' + a.join(',');",
    )?;
    assert_eq!(
        value,
        Value::String("6,5,4,3,2,1|1|0|4|1,2,3".to_owned())
    );
    Ok(())
}

#[test]
fn object_array_math_and_number_static_helpers_cover_common_web_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "var o = Object.fromEntries([['b', 2], ['a', 1], ['b', 3]]); Object.keys(o).join(',') + '|' + o.a + '|' + o.b + '|' + Array.of(7, 8).join('-') + '|' + Math.trunc(-3.9) + '|' + Math.sign(-9) + '|' + Number.isInteger(4) + '|' + Number.isInteger(4.2) + '|' + Number.isNaN(NaN) + '|' + Number.isNaN('x');",
    )?;
    assert_eq!(
        value,
        Value::String("a,b|1|3|7-8|-3|-1|true|false|true|false".to_owned())
    );
    Ok(())
}

#[test]
fn string_final_helpers_cover_index_substring_repeat_pad_and_replace_all()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "'phantom'.charAt(2) + '|' + 'abcdef'.substring(4, 1) + '|' + 'ha'.repeat(3) + '|' + '7'.padStart(3, '0') + '|' + 'x'.padEnd(3, '-') + '|' + 'a-b-a'.replaceAll('a', 'A') + '|' + 'ab'.replaceAll('', '.');",
    )?;
    assert_eq!(
        value,
        Value::String("a|bcd|hahaha|007|x--|A-b-A|.a.b.".to_owned())
    );
    Ok(())
}

#[test]
fn final_helpers_fail_closed_on_invalid_entry_and_repeat_count()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    match runtime.execute("Object.fromEntries([1]);") {
        Err(error) => assert_eq!(error.kind(), RuntimeErrorKind::InvalidMemberTarget),
        Ok(value) => panic!("invalid fromEntries unexpectedly returned {value:?}"),
    }
    match runtime.execute("'x'.repeat(-1);") {
        Err(error) => assert_eq!(error.kind(), RuntimeErrorKind::ArgumentLimitExceeded),
        Ok(value) => panic!("negative repeat unexpectedly returned {value:?}"),
    }
    Ok(())
}

#[test]
fn final_library_function_identity_survives_browser_host_refresh()
-> Result<(), Box<dyn std::error::Error>> {
    let snapshot = browser_snapshot()?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    runtime.execute(
        "var savedArrayOf = Array.of; var savedFromEntries = Object.fromEntries; var savedReplaceAll = 'x'.replaceAll;",
    )?;
    runtime.install_browser_host(&snapshot)?;
    let value = runtime.execute(
        "(savedArrayOf === Array.of) + '|' + (savedFromEntries === Object.fromEntries) + '|' + (savedReplaceAll === 'x'.replaceAll);",
    )?;
    assert_eq!(value, Value::String("true|true|true".to_owned()));
    Ok(())
}
