//! Phantom 2M-15 HTMLElement attribute/property reflection II regression corpus.

use std::collections::BTreeMap;

use phantom_js::{
    BrowserDomElementSnapshot, BrowserDomMutation, BrowserHostSnapshot, Runtime, Value,
};

fn runtime_with_target(
    attributes: BTreeMap<String, String>,
) -> Result<Runtime, Box<dyn std::error::Error>> {
    let target = BrowserDomElementSnapshot::new(1, "div", attributes, "target")?;
    let snapshot = BrowserHostSnapshot::new(
        "https://dom.test/reflection",
        "HTMLElement reflection",
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
fn string_reflected_properties_are_live_across_property_and_attribute_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target(BTreeMap::from([
        ("id".to_owned(), "target".to_owned()),
        ("title".to_owned(), "old".to_owned()),
    ]))?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         target.title = 'property'; target.lang = 'pt-BR'; target.dir = 'ltr'; \
         target.setAttribute('title', 'attribute'); \
         target.title + '|' + target.lang + '|' + target.dir + '|' + target.getAttribute('lang');",
    )?;
    assert_eq!(value, Value::String("attribute|pt-BR|ltr|pt-BR".to_owned()));
    Ok(())
}

#[test]
fn hidden_reflects_boolean_attribute_presence_and_truthiness()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime =
        runtime_with_target(BTreeMap::from([("id".to_owned(), "target".to_owned())]))?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var a = target.hidden; target.hidden = 'yes'; \
         var b = target.hidden + '|' + target.hasAttribute('hidden'); \
         target.hidden = 0; var c = target.hidden + '|' + target.hasAttribute('hidden'); \
         target.setAttribute('hidden', 'hidden'); \
         a + '|' + b + '|' + c + '|' + target.hidden;",
    )?;
    assert_eq!(
        value,
        Value::String("false|true|true|false|false|true".to_owned())
    );
    Ok(())
}

#[test]
fn get_attribute_names_returns_bounded_deterministic_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target(BTreeMap::from([
        ("title".to_owned(), "tip".to_owned()),
        ("id".to_owned(), "target".to_owned()),
        ("data-a".to_owned(), "1".to_owned()),
    ]))?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var names = target.getAttributeNames(); \
         names.length + '|' + names[0] + '|' + names[1] + '|' + names[2];",
    )?;
    assert_eq!(value, Value::String("3|data-a|id|title".to_owned()));
    Ok(())
}

#[test]
fn reflected_property_writes_emit_real_dom_attribute_mutations()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime =
        runtime_with_target(BTreeMap::from([("id".to_owned(), "target".to_owned())]))?;
    runtime.execute(
        "var target = document.getElementById('target'); \
         target.title = 'tip'; target.hidden = true; target.hidden = false;",
    )?;
    let mutations = runtime.drain_dom_mutations();
    assert_eq!(mutations.len(), 3);
    assert!(matches!(
        &mutations[0],
        BrowserDomMutation::SetAttribute { name, value, .. }
            if name == "title" && value == "tip"
    ));
    assert!(matches!(
        &mutations[1],
        BrowserDomMutation::SetAttribute { name, value, .. }
            if name == "hidden" && value.is_empty()
    ));
    assert!(matches!(
        &mutations[2],
        BrowserDomMutation::RemoveAttribute { name, .. } if name == "hidden"
    ));
    Ok(())
}
