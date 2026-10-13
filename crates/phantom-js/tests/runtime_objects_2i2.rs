//! Phantom 2I-2 Objects / Arrays / Functions Runtime I regression corpus.

use phantom_js::{Runtime, RuntimeErrorKind, Value};

#[test]
fn arrays_support_index_read_write_and_length() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let value = runtime.execute(
        "let items = [2, 4]; items[1] = 9; items[3] = 5; items.length + items[1] + items[3];",
    )?;

    assert_eq!(value, Value::Number(18.0));
    Ok(())
}

#[test]
fn object_literals_support_static_and_computed_members() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let value = runtime.execute(
        r#"let data = { left: 2, "right": 3 }; data.total = data.left + data["right"]; data.total;"#,
    )?;

    assert_eq!(value, Value::Number(5.0));
    Ok(())
}

#[test]
fn function_arguments_and_return_values_execute() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let value = runtime.execute("let add = function(a, b) { return a + b; }; add(4, 7);")?;

    assert_eq!(value, Value::Number(11.0));
    Ok(())
}

#[test]
fn named_function_expression_supports_bounded_recursion() -> Result<(), Box<dyn std::error::Error>>
{
    let mut runtime = Runtime::new();
    let value = runtime.execute(
        "let factorial = function fact(n) { \
            if (n <= 1) { return 1; } \
            return n * fact(n - 1); \
        }; factorial(5);",
    )?;

    assert_eq!(value, Value::Number(120.0));
    Ok(())
}

#[test]
fn function_var_is_local_but_existing_global_can_be_assigned()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let value = runtime.execute(
        "var total = 1; \
         let work = function(value) { var local = value * 2; total = total + local; return local; }; \
         let result = work(3); total + result;",
    )?;

    assert_eq!(value, Value::Number(13.0));
    assert!(runtime.global("local").is_none());
    assert_eq!(runtime.global("total"), Some(&Value::Number(7.0)));
    Ok(())
}

#[test]
fn function_values_can_live_in_objects_and_be_called() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let value = runtime.execute(
        "let increment = function(value) { return value + 1; }; \
         let tools = { run: increment }; tools.run(9);",
    )?;

    assert_eq!(value, Value::Number(10.0));
    Ok(())
}

#[test]
fn functions_survive_across_successive_execute_calls() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.execute("var multiply = function(a, b) { return a * b; };")?;
    let value = runtime.execute("multiply(6, 7);")?;

    assert_eq!(value, Value::Number(42.0));
    Ok(())
}

#[test]
fn missing_object_property_and_array_slot_return_undefined()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();

    assert_eq!(
        runtime.execute("let item = { value: 1 }; item.missing;")?,
        Value::Undefined
    );
    assert_eq!(
        runtime.execute("let list = [1]; list[8];")?,
        Value::Undefined
    );
    Ok(())
}

#[test]
fn calling_non_function_fails_closed() {
    let mut runtime = Runtime::new();
    let error = runtime.execute("let value = 7; value();").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(RuntimeErrorKind::CallNonFunction)
    );
}

#[test]
fn this_and_closure_capture_execute_with_function_context() -> Result<(), Box<dyn std::error::Error>>
{
    let mut runtime = Runtime::new();
    let method_value = runtime.execute(
        "let holder = { value: 7, read: function() { return this.value; } }; holder.read();",
    )?;
    assert_eq!(method_value, Value::Number(7.0));

    let mut runtime = Runtime::new();
    let closure_value = runtime.execute(
        "let make = function(value) { \
            return function() { value = value + 1; return value; }; \
         }; let closure = make(3); closure(); closure();",
    )?;
    assert_eq!(closure_value, Value::Number(5.0));
    Ok(())
}
