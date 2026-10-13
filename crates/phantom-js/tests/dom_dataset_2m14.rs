//! Phantom 2M-14 DOM dataset / attribute reflection Foundation I regression corpus.

use std::collections::BTreeMap;

use phantom_js::{BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value};

fn runtime_with_target(attributes: BTreeMap<String, String>) -> Result<Runtime, Box<dyn std::error::Error>> {
    let target = BrowserDomElementSnapshot::new(1, "div", attributes, "target")?;
    let snapshot = BrowserHostSnapshot::new(
        "https://dom.test/dataset",
        "DOM dataset attribute reflection",
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
fn dataset_identity_camel_case_and_assignment_reflect_to_data_attributes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target(BTreeMap::from([
        ("id".to_owned(), "target".to_owned()),
        ("data-user-id".to_owned(), "7".to_owned()),
    ]))?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var dataset = target.dataset; \
         dataset.userId = '42'; \
         (dataset === target.dataset) + '|' + dataset.userId + '|' + target.getAttribute('data-user-id');",
    )?;

    assert_eq!(value, Value::String("true|42|42".to_owned()));
    Ok(())
}

#[test]
fn dataset_is_live_across_set_attribute_and_remove_attribute()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target(BTreeMap::from([(
        "id".to_owned(),
        "target".to_owned(),
    )]))?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var dataset = target.dataset; \
         target.setAttribute('data-state', 'ready'); \
         var before = dataset.state; \
         target.removeAttribute('data-state'); \
         before + '|' + (dataset.state === undefined);",
    )?;

    assert_eq!(value, Value::String("ready|true".to_owned()));
    Ok(())
}

#[test]
fn toggle_attribute_supports_default_toggle_and_boolean_force()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target(BTreeMap::from([(
        "id".to_owned(),
        "target".to_owned(),
    )]))?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var first = target.toggleAttribute('data-enabled'); \
         var second = target.toggleAttribute('data-enabled'); \
         var forced = target.toggleAttribute('data-enabled', true); \
         first + '|' + second + '|' + forced + '|' + target.hasAttribute('data-enabled');",
    )?;

    assert_eq!(value, Value::String("true|false|true|true".to_owned()));
    Ok(())
}

#[test]
fn invalid_dataset_member_fails_before_attribute_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target(BTreeMap::from([(
        "id".to_owned(),
        "target".to_owned(),
    )]))?;
    let error = runtime
        .execute(
            "var target = document.getElementById('target'); \
             target.dataset['bad-key'] = 'x';",
        )
        .expect_err("Foundation I rejects dataset keys outside the bounded camelCase subset");
    assert_eq!(error.kind(), RuntimeErrorKind::InvalidPropertyKey);

    let value = runtime.execute(
        "document.getElementById('target').getAttribute('data-bad-key') === null;",
    )?;
    assert_eq!(value, Value::Boolean(true));
    Ok(())
}
