//! Phantom 2M-22 DOM mutation API II regression corpus.

use std::collections::BTreeMap;

use phantom_js::{
    BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, RuntimeErrorKind, Value,
};

fn attrs(id: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("id".to_owned(), id.to_owned())])
}

fn element(
    node_id: u64,
    tag: &str,
    id: &str,
    parent: Option<u64>,
) -> Result<BrowserDomElementSnapshot, Box<dyn std::error::Error>> {
    Ok(BrowserDomElementSnapshot::new(node_id, tag, attrs(id), "")?
        .with_parent_node_id(parent)
        .with_connected(true))
}

fn mutation_runtime() -> Result<Runtime, Box<dyn std::error::Error>> {
    let snapshot = BrowserHostSnapshot::new(
        "https://dom-mutation.test/",
        "DOM mutation API II",
        800.0,
        600.0,
        1.0,
    )?
    .with_dom_elements(vec![
        element(1, "html", "html", Some(99))?,
        element(2, "body", "body", Some(1))?,
        element(3, "section", "parent", Some(2))?,
        element(4, "span", "a", Some(3))?,
        element(5, "span", "b", Some(3))?,
        element(6, "span", "c", Some(3))?,
        element(7, "section", "other", Some(2))?,
        element(8, "span", "d", Some(7))?,
        element(9, "span", "e", Some(7))?,
    ])?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn append_and_prepend_move_elements_and_preserve_requested_order()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = mutation_runtime()?;
    let value = runtime.execute(
        "var p = document.getElementById('parent'); var b = document.getElementById('b'); var d = document.getElementById('d'); var e = document.getElementById('e'); p.prepend(d); p.append(b, e); p.children[0].id + '|' + p.children[1].id + '|' + p.children[2].id + '|' + p.children[3].id + '|' + p.children[4].id + '|' + p.childElementCount;",
    )?;
    assert_eq!(value, Value::String("d|a|c|b|e|5".to_owned()));
    Ok(())
}

#[test]
fn before_after_and_replace_with_reorder_existing_siblings_coherently()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = mutation_runtime()?;
    let value = runtime.execute(
        "var p = document.getElementById('parent'); var a = document.getElementById('a'); var b = document.getElementById('b'); var c = document.getElementById('c'); var d = document.getElementById('d'); var e = document.getElementById('e'); b.before(d); b.after(e); a.after(c); b.replaceWith(e); p.children[0].id + '|' + p.children[1].id + '|' + p.children[2].id + '|' + p.children[3].id + '|' + b.isConnected + '|' + e.previousElementSibling.id;",
    )?;
    assert_eq!(value, Value::String("a|c|d|e|false|d".to_owned()));
    Ok(())
}

#[test]
fn replace_children_and_remove_child_update_identity_parent_and_connection()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = mutation_runtime()?;
    let value = runtime.execute(
        "var p = document.getElementById('parent'); var a = document.getElementById('a'); var b = document.getElementById('b'); var c = document.getElementById('c'); var d = document.getElementById('d'); p.replaceChildren(d, c, a); var removed = p.removeChild(c); (removed === c) + '|' + p.children[0].id + '|' + p.children[1].id + '|' + p.childElementCount + '|' + c.isConnected + '|' + b.isConnected + '|' + (c.parentElement === null);",
    )?;
    assert_eq!(
        value,
        Value::String("true|d|a|2|false|false|true".to_owned())
    );
    Ok(())
}

#[test]
fn invalid_multi_node_mutation_fails_before_partial_reparenting()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = mutation_runtime()?;
    match runtime.execute(
        "var p = document.getElementById('parent'); var d = document.getElementById('d'); p.append(d, p);",
    ) {
        Err(error) => assert_eq!(error.kind(), RuntimeErrorKind::InvalidDomHierarchy),
        Ok(value) => panic!("invalid hierarchy unexpectedly returned {value:?}"),
    }
    assert!(runtime.drain_dom_mutations().is_empty());
    let value = runtime.execute(
        "var p = document.getElementById('parent'); var d = document.getElementById('d'); p.children[0].id + '|' + p.children[1].id + '|' + p.children[2].id + '|' + d.parentElement.id;",
    )?;
    assert_eq!(value, Value::String("a|b|c|other".to_owned()));
    Ok(())
}

#[test]
fn remove_child_rejects_non_child_without_mutating_tree() -> Result<(), Box<dyn std::error::Error>>
{
    let mut runtime = mutation_runtime()?;
    match runtime.execute(
        "var p = document.getElementById('parent'); var d = document.getElementById('d'); p.removeChild(d);",
    ) {
        Err(error) => assert_eq!(error.kind(), RuntimeErrorKind::InvalidDomHierarchy),
        Ok(value) => panic!("non-child removal unexpectedly returned {value:?}"),
    }
    assert!(runtime.drain_dom_mutations().is_empty());
    let value = runtime.execute("document.getElementById('d').parentElement.id;")?;
    assert_eq!(value, Value::String("other".to_owned()));
    Ok(())
}
