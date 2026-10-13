//! Phantom 2M-21 DOM selector API compatibility regression corpus.

use std::collections::BTreeMap;

use phantom_js::{
    BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value,
};

fn attrs(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn element(
    node_id: u64,
    tag: &str,
    attributes: &[(&str, &str)],
    parent: Option<u64>,
    connected: bool,
) -> Result<BrowserDomElementSnapshot, Box<dyn std::error::Error>> {
    Ok(
        BrowserDomElementSnapshot::new(node_id, tag, attrs(attributes), "")?
            .with_parent_node_id(parent)
            .with_connected(connected),
    )
}

fn selector_runtime() -> Result<Runtime, Box<dyn std::error::Error>> {
    let snapshot = BrowserHostSnapshot::new(
        "https://selectors.test/",
        "DOM selector API",
        800.0,
        600.0,
        1.0,
    )?
    .with_dom_elements(vec![
        element(1, "html", &[("id", "html")], Some(99), true)?,
        element(2, "body", &[("id", "body")], Some(1), true)?,
        element(
            3,
            "div",
            &[("id", "app"), ("class", "shell")],
            Some(2),
            true,
        )?,
        element(
            4,
            "section",
            &[
                ("id", "primary"),
                ("class", "panel primary"),
                ("data-state", "ready"),
            ],
            Some(3),
            true,
        )?,
        element(
            5,
            "article",
            &[("id", "card-1"), ("class", "card featured")],
            Some(4),
            true,
        )?,
        element(
            6,
            "span",
            &[
                ("id", "hero"),
                ("class", "label hero"),
                ("data-kind", "hero"),
                ("data-label", "hero card"),
            ],
            Some(5),
            true,
        )?,
        element(
            7,
            "article",
            &[("id", "card-2"), ("class", "card")],
            Some(4),
            true,
        )?,
        element(
            8,
            "span",
            &[("id", "plain"), ("class", "label")],
            Some(7),
            true,
        )?,
        element(
            9,
            "section",
            &[("id", "secondary"), ("class", "panel secondary")],
            Some(3),
            true,
        )?,
        element(
            10,
            "span",
            &[("id", "side"), ("class", "label"), ("data-kind", "side")],
            Some(9),
            true,
        )?,
        element(
            11,
            "span",
            &[("id", "detached"), ("class", "label")],
            None,
            false,
        )?,
    ])?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn compound_descendant_child_and_attribute_selectors_find_expected_element()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = selector_runtime()?;
    let value = runtime.execute(
        "var hero = document.querySelector('#app > section.panel.primary[data-state=ready] article.card.featured > span.label[data-kind=hero][data-label=\"hero card\"]'); hero.id + '|' + hero.parentElement.id + '|' + hero.closest('section.panel.primary').id;",
    )?;
    assert_eq!(value, Value::String("hero|card-1|primary".to_owned()));
    Ok(())
}

#[test]
fn selector_lists_are_deduplicated_and_keep_document_order()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = selector_runtime()?;
    let value = runtime.execute(
        "var list = document.querySelectorAll('#app section.panel .label, #app > section.secondary > span.label[data-kind=side]'); list.length + '|' + list[0].id + '|' + list[1].id + '|' + list[2].id;",
    )?;
    assert_eq!(value, Value::String("3|hero|plain|side".to_owned()));
    Ok(())
}

#[test]
fn element_scope_excludes_self_and_universal_selector_stays_within_descendants()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = selector_runtime()?;
    let value = runtime.execute(
        "var primary = document.getElementById('primary'); var labels = primary.querySelectorAll('article.card > span.label'); var scoped = primary.querySelector('#primary') === null; scoped + '|' + labels.length + '|' + labels[0].id + '|' + labels[1].id + '|' + primary.querySelectorAll('*').length;",
    )?;
    assert_eq!(value, Value::String("true|2|hero|plain|4".to_owned()));
    Ok(())
}

#[test]
fn matches_and_closest_accept_compounds_chains_and_selector_lists()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = selector_runtime()?;
    let value = runtime.execute(
        "var hero = document.getElementById('hero'); var primary = document.getElementById('primary'); hero.matches('span.label[data-kind=hero], div.nope') + '|' + primary.matches('#app > section.panel.primary') + '|' + hero.closest('#app > section.panel.primary, body.nope').id;",
    )?;
    assert_eq!(value, Value::String("true|true|primary".to_owned()));
    Ok(())
}

#[test]
fn unsupported_or_malformed_selector_syntax_fails_closed() -> Result<(), Box<dyn std::error::Error>>
{
    for source in [
        "document.querySelector('div + span');",
        "document.querySelector('section:not(.x)');",
        "document.querySelector('div,,span');",
        "document.querySelector('div > > span');",
    ] {
        let mut runtime = selector_runtime()?;
        match runtime.execute(source) {
            Err(error) => assert_eq!(error.kind(), RuntimeErrorKind::UnsupportedHostSelector),
            Ok(value) => panic!("unsupported selector unexpectedly returned {value:?}"),
        }
    }
    Ok(())
}
