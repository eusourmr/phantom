//! Phantom 2J-2 Live DOM Bridge I JavaScript-side regression corpus.

use std::collections::BTreeMap;

use phantom_js::{
    BrowserDomElementSnapshot, BrowserDomMutation, BrowserHostSnapshot, Runtime, Value,
};

fn host() -> Result<BrowserHostSnapshot, phantom_js::RuntimeError> {
    let hero = BrowserDomElementSnapshot::new(
        7,
        "p",
        BTreeMap::from([
            ("id".to_owned(), "hero".to_owned()),
            ("data-state".to_owned(), "before".to_owned()),
        ]),
        "Before",
    )?;
    let secondary = BrowserDomElementSnapshot::new(
        8,
        "span",
        BTreeMap::from([("id".to_owned(), "secondary".to_owned())]),
        "Secondary",
    )?;

    BrowserHostSnapshot::new("https://example.test/", "Example", 1024.0, 768.0, 1.0)?
        .with_dom_elements(vec![hero, secondary])
}

#[test]
fn get_element_by_id_is_lazy_and_identity_stable() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let a = document.getElementById('hero'); \
             let b = document.getElementById('hero'); \
             a === b && a.nodeType === 1 && a.tagName === 'P';",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn query_selector_supports_id_and_simple_tag() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "document.querySelector('#hero') === document.getElementById('hero') && \
             document.querySelector('span').id === 'secondary';",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn text_content_assignment_emits_bounded_dom_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute("let hero = document.getElementById('hero'); hero.textContent = 'After';")?;

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
fn attribute_methods_read_write_remove_and_emit_mutations() -> Result<(), Box<dyn std::error::Error>>
{
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    let value = runtime.execute(
        "let hero = document.getElementById('hero'); \
         let before = hero.getAttribute('data-state'); \
         hero.setAttribute('data-state', 'after'); \
         let changed = hero.getAttribute('data-state'); \
         hero.removeAttribute('data-state'); \
         before + '|' + changed + '|' + hero.getAttribute('data-state');",
    )?;

    assert_eq!(value, Value::String("before|after|null".to_owned()));
    assert_eq!(
        runtime.drain_dom_mutations(),
        vec![
            BrowserDomMutation::SetAttribute {
                node_id: 7,
                name: "data-state".to_owned(),
                value: "after".to_owned(),
            },
            BrowserDomMutation::RemoveAttribute {
                node_id: 7,
                name: "data-state".to_owned(),
            },
        ]
    );
    Ok(())
}

#[test]
fn missing_element_returns_null() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute("document.getElementById('missing') === null;")?,
        Value::Boolean(true)
    );
    Ok(())
}
