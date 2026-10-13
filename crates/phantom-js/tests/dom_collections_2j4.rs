//! Phantom 2J-4 DOM APIs / Collections Hardening I regression corpus.

use std::collections::BTreeMap;

use phantom_js::{BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, Value};

fn element(
    node_id: u64,
    tag: &str,
    parent: Option<u64>,
    attrs: &[(&str, &str)],
) -> Result<BrowserDomElementSnapshot, phantom_js::RuntimeError> {
    BrowserDomElementSnapshot::new(
        node_id,
        tag,
        attrs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>(),
        "",
    )
    .map(|snapshot| snapshot.with_parent_node_id(parent).with_connected(true))
}

fn host() -> Result<BrowserHostSnapshot, phantom_js::RuntimeError> {
    BrowserHostSnapshot::new("https://example.test/", "Example", 1024.0, 768.0, 1.0)?
        .with_dom_elements(vec![
            element(1, "html", None, &[("id", "html")])?,
            element(2, "body", Some(1), &[("id", "body")])?,
            element(
                3,
                "section",
                Some(2),
                &[("id", "first"), ("class", "card featured")],
            )?,
            element(
                4,
                "span",
                Some(3),
                &[("id", "inside"), ("class", "featured")],
            )?,
            element(
                5,
                "section",
                Some(2),
                &[("id", "second"), ("class", "card")],
            )?,
        ])
}

#[test]
fn document_exposes_root_body_and_snapshot_collections() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let cards = document.querySelectorAll('.card'); \
             document.documentElement.tagName === 'HTML' && \
             document.body.id === 'body' && cards.length === 2 && \
             cards[0].id === 'first' && cards[1].id === 'second';",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn class_and_tag_collections_preserve_host_document_order() -> Result<(), Box<dyn std::error::Error>>
{
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let featured = document.getElementsByClassName('featured'); \
             let exact = document.getElementsByClassName('card featured'); \
             let all = document.getElementsByTagName('*'); \
             featured.length === 2 && featured[0].id === 'first' && \
             featured[1].id === 'inside' && exact.length === 1 && \
             exact[0].id === 'first' && all.length === 5;",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn element_scoped_queries_do_not_escape_the_subtree() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let first = document.getElementById('first'); \
             let matches = first.querySelectorAll('.featured'); \
             matches.length === 1 && matches[0].id === 'inside' && \
             first.querySelector('#second') === null;",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn element_predicates_cover_attributes_ancestry_and_containment()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let first = document.getElementById('first'); \
             let inside = document.getElementById('inside'); \
             first.hasAttribute('class') && first.matches('section.card') && \
             inside.closest('.card') === first && first.contains(inside) && \
             first.contains(first) && !inside.contains(first);",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn collections_reorder_immediately_after_reparent_in_one_execution()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let first = document.getElementById('first'); \
             let second = document.getElementById('second'); \
             second.appendChild(first); \
             let sections = document.querySelectorAll('section'); \
             sections[0] === second && sections[1] === first;",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn element_host_methods_do_not_consume_native_function_heap_per_element()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let all = document.getElementsByTagName('*'); \
             all[0].getAttribute('id') === 'html' && \
             all[4].hasAttribute('class');",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}
