//! Phantom 2M-13 CSSStyleDeclaration / Element.style Foundation I regression corpus.

use std::collections::BTreeMap;

use phantom_js::{
    BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value,
};

fn runtime_with_target(style: &str) -> Result<Runtime, Box<dyn std::error::Error>> {
    let mut attributes = BTreeMap::from([("id".to_owned(), "target".to_owned())]);
    if !style.is_empty() {
        attributes.insert("style".to_owned(), style.to_owned());
    }
    let target = BrowserDomElementSnapshot::new(1, "div", attributes, "target")?;
    let snapshot = BrowserHostSnapshot::new(
        "https://dom.test/style",
        "CSSStyleDeclaration style",
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
fn style_direct_members_and_property_methods_are_live() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target("color: red; width: 10px")?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var style = target.style; \
         style.backgroundColor = '#1565c0'; \
         style.color = '#ffffff'; \
         style.setProperty('font-weight', '700'); \
         (style === target.style) + '|' + style.backgroundColor + '|' + style.getPropertyValue('width') + '|' + style.fontWeight;",
    )?;

    assert_eq!(value, Value::String("true|#1565c0|10px|700".to_owned()));
    Ok(())
}

#[test]
fn style_remove_property_returns_previous_value_and_empty_assignment_removes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target("color: red; width: 10px; display: block")?;
    let value = runtime.execute(
        "var style = document.getElementById('target').style; \
         var oldWidth = style.removeProperty('width'); \
         style.display = ''; \
         oldWidth + '|' + style.getPropertyValue('width') + '|' + style.display + '|' + style.color;",
    )?;

    assert_eq!(value, Value::String("10px|||red".to_owned()));
    Ok(())
}

#[test]
fn css_text_assignment_is_canonical_and_style_identity_survives_set_attribute()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target("")?;
    let value = runtime.execute(
        "var target = document.getElementById('target'); \
         var style = target.style; \
         style.cssText = 'color:#6a1b9a; font-weight: 700'; \
         var first = style.cssText; \
         target.setAttribute('style', 'color: #1565c0; width: 20px'); \
         (style === target.style) + '|' + first + '|' + style.color + '|' + style.width;",
    )?;

    assert_eq!(
        value,
        Value::String("true|color: #6a1b9a; font-weight: 700|#1565c0|20px".to_owned())
    );
    Ok(())
}

#[test]
fn invalid_style_property_name_fails_before_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = runtime_with_target("color: red")?;
    let error = runtime
        .execute(
            "var style = document.getElementById('target').style; \
             style.setProperty('bad name', 'blue');",
        )
        .expect_err("whitespace in a bounded CSS property name must fail closed");
    assert_eq!(error.kind(), RuntimeErrorKind::InvalidCssPropertyName);

    let value = runtime.execute("document.getElementById('target').style.color;")?;
    assert_eq!(value, Value::String("red".to_owned()));
    Ok(())
}
