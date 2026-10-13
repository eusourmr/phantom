//! Phantom 2I-1 JavaScript Runtime Core I regression corpus.

use phantom_js::{Runtime, RuntimeErrorKind, Value};

#[test]
fn executes_scalar_bindings_arithmetic_and_assignment() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let value = runtime.execute("let total = 1; total = total + 2 * 3; total;")?;

    assert_eq!(value, Value::Number(7.0));
    assert_eq!(runtime.global("total"), Some(&Value::Number(7.0)));
    Ok(())
}

#[test]
fn block_let_is_lexical_while_var_escapes_to_global() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let value = runtime.execute(
        "let outer = 1; { let outer = 8; outer = outer + 1; var leaked = outer; } outer + leaked;",
    )?;

    assert_eq!(value, Value::Number(10.0));
    assert_eq!(runtime.global("outer"), Some(&Value::Number(1.0)));
    assert_eq!(runtime.global("leaked"), Some(&Value::Number(9.0)));
    Ok(())
}

#[test]
fn executes_if_while_and_classic_for() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let value = runtime.execute(
        "let total = 0; \
         for (let i = 0; i < 4; i = i + 1) { total = total + i; } \
         let n = 0; \
         while (n < 2) { total = total + 10; n = n + 1; } \
         if (total === 26) { total = total + 1; } else { total = 0; } \
         total;",
    )?;

    assert_eq!(value, Value::Number(27.0));
    Ok(())
}

#[test]
fn logical_operators_short_circuit_and_return_operand_values()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let value = runtime
        .execute("let touched = 0; false && (touched = 1); true || (touched = 2); touched;")?;

    assert_eq!(value, Value::Number(0.0));
    Ok(())
}

#[test]
fn string_concatenation_and_common_escapes_execute() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    let value = runtime.execute(r#"let name = "Phantom"; "Hello " + name + "\n";"#)?;

    assert_eq!(value, Value::String("Hello Phantom\n".to_owned()));
    Ok(())
}

#[test]
fn const_assignment_fails_closed() {
    let mut runtime = Runtime::new();
    let error = runtime.execute("const answer = 42; answer = 7;").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(RuntimeErrorKind::ConstAssignment)
    );
}

#[test]
fn undeclared_identifier_is_a_reference_error() {
    let mut runtime = Runtime::new();
    let error = runtime.execute("missing;").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(RuntimeErrorKind::ReferenceError)
    );
}

#[test]
fn browser_host_context_remains_deferred_after_runtime_core_i() {
    // PHANTOM_2M24_LEGACY_BROWSER_HOST_BOUNDARY_I
    // Historical test name retained for continuity. Browser-host support
    // arrived after 2I; the invariant that still belongs here is that a raw
    // Runtime does not expose page globals until the host installs them.
    let runtime = Runtime::new();

    assert!(runtime.global("window").is_none());
    assert!(runtime.global("document").is_none());
    assert!(runtime.global("location").is_none());
}
#[test]
fn infinite_loop_is_stopped_by_runtime_budget() {
    let mut runtime = Runtime::new();
    let error = runtime.execute("while (true) { }").err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(RuntimeErrorKind::StepLimitExceeded)
    );
}

#[test]
fn runtime_preserves_globals_between_successive_scripts() -> Result<(), Box<dyn std::error::Error>>
{
    let mut runtime = Runtime::new();
    runtime.execute("var counter = 2;")?;
    let value = runtime.execute("counter = counter + 3; counter;")?;

    assert_eq!(value, Value::Number(5.0));
    Ok(())
}
