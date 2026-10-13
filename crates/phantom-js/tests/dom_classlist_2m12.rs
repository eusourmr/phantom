//! Phantom 2M-12 DOMTokenList / classList Foundation I regression corpus.

use std::collections::BTreeMap;

use phantom_js::{BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value};

fn runtime_with_target(class_name: &str) -> Result<Runtime, Box<dyn std::error::Error>> {
    let mut attributes = BTreeMap::from([("id".to_owned(), "target".to_owned())]);
    if !class_name.is_empty() {
        attributes.insert("class".to_owned(), class_name.to_owned());
    }
    let target = BrowserDomElementSnapshot::new(1, "div", attributes, "target")?;
    let snapshot = BrowserHostSnapshot::new(
        "https://dom.test/classlist",
        "DOMTokenList classList",
        800.0,
        600.0,
        1.0,
    )?
    .with_dom_elements(vec![target])?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn class_list_add_remove_contains_preserve_order_and_deduplicate()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target("alpha beta alpha")?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var before = target.classList.contains('alpha'); \
         target.classList.add('beta', 'gamma'); \
         var afterAdd = target.className; \
         target.classList.remove('alpha', 'missing'); \
         before + '|' + afterAdd + '|' + target.className + '|' + target.classList.contains('gamma');",
    )?;

    assert_eq!(
        value,
        Value::String("true|alpha beta gamma|beta gamma|true".to_owned())
    );
    Ok(())
}

#[test]
fn class_list_toggle_supports_boolean_force_and_returns_presence()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target("one")?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var a = target.classList.toggle('two'); \
         var b = target.classList.toggle('two'); \
         var c = target.classList.toggle('three', true); \
         var d = target.classList.toggle('three', false); \
         a + '|' + b + '|' + c + '|' + d + '|' + target.className;",
    )?;

    assert_eq!(value, Value::String("true|false|true|false|one".to_owned()));
    Ok(())
}

#[test]
fn invalid_class_list_token_fails_before_partial_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target("base")?;
    let error = runtime
        .execute(
            "var target = document.getElementById('target'); \
             target.classList.add('good', 'bad token');",
        )
        .expect_err("ASCII whitespace in a DOM token must fail closed");
    assert_eq!(error.kind(), RuntimeErrorKind::InvalidDomToken);

    let value = runtime.execute(
        "document.getElementById('target').className;",
    )?;
    assert_eq!(value, Value::String("base".to_owned()));
    Ok(())
}

#[test]
fn class_list_identity_survives_element_refresh_and_class_name_assignment()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target("")?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var list = target.classList; \
         target.className = 'fresh'; \
         var again = document.getElementById('target'); \
         (list === again.classList) + '|' + again.classList.contains('fresh');",
    )?;

    assert_eq!(value, Value::String("true|true".to_owned()));
    Ok(())
}
