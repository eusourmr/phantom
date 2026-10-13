//! Phantom 2M-17 web core library II regression corpus.

use phantom_js::{BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value};

fn browser_runtime() -> Result<Runtime, Box<dyn std::error::Error>> {
    let snapshot = BrowserHostSnapshot::new(
        "https://core-library.test/",
        "Web core library II",
        800.0,
        600.0,
        1.0,
    )?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn array_instance_methods_are_receiver_bound_and_mutating() -> Result<(), Box<dyn std::error::Error>>
{
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "var a = [1, 2]; var n = a.push(3, 4); var p = a.pop(); n + '|' + p + '|' + a.join('-') + '|' + a.indexOf(2) + '|' + a.includes(9) + '|' + a.slice(1, 3).join(',');",
    )?;
    assert_eq!(value, Value::String("4|4|1-2-3|1|false|2,3".to_owned()));
    Ok(())
}

#[test]
fn string_instance_methods_cover_common_web_transforms() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "var s = '  Phantom Browser  '.trim(); s.length + '|' + s.toLowerCase() + '|' + s.toUpperCase().startsWith('PHANTOM') + '|' + s.includes('Browser') + '|' + s.endsWith('Browser') + '|' + s.indexOf('Browser') + '|' + s.slice(8);",
    )?;
    assert_eq!(
        value,
        Value::String("15|phantom browser|true|true|true|8|Browser".to_owned())
    );
    Ok(())
}

#[test]
fn object_keys_values_and_assign_use_bounded_ordinary_objects()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "var target = {b: 2, a: 1}; Object.assign(target, {c: 3}); Object.keys(target).join(',') + '|' + Object.values(target).join(',') + '|' + target.c;",
    )?;
    assert_eq!(value, Value::String("a,b,c|1,2,3|3".to_owned()));
    Ok(())
}

#[test]
fn json_parse_and_stringify_round_trip_nested_arrays_and_objects()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    let value = runtime.execute(
        "var data = JSON.parse('{\"name\":\"Phantom\",\"scores\":[2,4]}'); data.scores.push(6); JSON.stringify(data) + '|' + data.name + '|' + data.scores.join('-');",
    )?;
    assert_eq!(
        value,
        Value::String("{\"name\":\"Phantom\",\"scores\":[2,4,6]}|Phantom|2-4-6".to_owned())
    );
    Ok(())
}

#[test]
fn invalid_json_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = browser_runtime()?;
    match runtime.execute("JSON.parse('{bad json}');") {
        Err(error) => assert_eq!(error.kind(), RuntimeErrorKind::InvalidJson),
        Ok(value) => panic!("invalid JSON unexpectedly returned {value:?}"),
    }
    Ok(())
}
