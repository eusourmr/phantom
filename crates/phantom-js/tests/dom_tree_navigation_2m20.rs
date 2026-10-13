//! Phantom 2M-20 DOM tree navigation and mutation-coherence regression corpus.

use std::collections::BTreeMap;

use phantom_js::{BrowserDomElementSnapshot, BrowserHostSnapshot, Runtime, Value};

fn attrs(id: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("id".to_owned(), id.to_owned())])
}

fn element(
    node_id: u64,
    tag: &str,
    id: &str,
    parent: Option<u64>,
    connected: bool,
) -> Result<BrowserDomElementSnapshot, Box<dyn std::error::Error>> {
    Ok(BrowserDomElementSnapshot::new(node_id, tag, attrs(id), "")?
        .with_parent_node_id(parent)
        .with_connected(connected))
}

fn tree_runtime() -> Result<Runtime, Box<dyn std::error::Error>> {
    let snapshot = BrowserHostSnapshot::new(
        "https://dom-tree.test/",
        "DOM tree navigation",
        800.0,
        600.0,
        1.0,
    )?
    .with_dom_elements(vec![
        element(1, "html", "html", Some(99), true)?,
        element(2, "body", "body", Some(1), true)?,
        element(3, "section", "parent", Some(2), true)?,
        element(4, "span", "a", Some(3), true)?,
        element(5, "span", "b", Some(3), true)?,
        element(6, "span", "c", Some(3), true)?,
        element(7, "section", "other", Some(2), true)?,
        element(8, "span", "d", Some(7), true)?,
    ])?;
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&snapshot)?;
    Ok(runtime)
}

#[test]
fn parent_children_first_last_count_and_siblings_follow_dom_order()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = tree_runtime()?;
    let value = runtime.execute(
        "var p = document.getElementById('parent'); var a = document.getElementById('a'); var b = document.getElementById('b'); var c = document.getElementById('c'); p.childElementCount + '|' + p.children.length + '|' + p.firstElementChild.id + '|' + p.lastElementChild.id + '|' + b.previousElementSibling.id + '|' + b.nextElementSibling.id + '|' + b.parentElement.id + '|' + (a.previousElementSibling === null) + '|' + (c.nextElementSibling === null);",
    )?;
    assert_eq!(
        value,
        Value::String("3|3|a|c|a|c|parent|true|true".to_owned())
    );
    Ok(())
}

#[test]
fn traversal_preserves_element_identity_and_stops_at_non_element_parent()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = tree_runtime()?;
    let value = runtime.execute(
        "var p = document.getElementById('parent'); var a = document.getElementById('a'); var html = document.getElementById('html'); (p.firstElementChild === a) + '|' + (p.children[0] === a) + '|' + (a.parentElement === p) + '|' + (html.parentElement === null);",
    )?;
    assert_eq!(value, Value::String("true|true|true|true".to_owned()));
    Ok(())
}

#[test]
fn append_child_moves_existing_subtree_to_end_and_updates_live_navigation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = tree_runtime()?;
    let value = runtime.execute(
        "var parent = document.getElementById('parent'); var other = document.getElementById('other'); var b = document.getElementById('b'); var d = document.getElementById('d'); other.appendChild(b); parent.childElementCount + '|' + parent.lastElementChild.id + '|' + other.childElementCount + '|' + other.firstElementChild.id + '|' + other.lastElementChild.id + '|' + b.parentElement.id + '|' + d.nextElementSibling.id + '|' + b.previousElementSibling.id;",
    )?;
    assert_eq!(value, Value::String("2|c|2|d|b|other|b|d".to_owned()));
    Ok(())
}

#[test]
fn remove_and_detached_creation_keep_parent_children_and_is_connected_coherent()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = tree_runtime()?;
    let value = runtime.execute(
        "var parent = document.createElement('div'); var child = document.createElement('span'); parent.appendChild(child); var before = parent.childElementCount + '|' + (parent.firstElementChild === child) + '|' + (child.parentElement === parent) + '|' + child.isConnected; child.remove(); before + '|' + parent.childElementCount + '|' + (parent.firstElementChild === null) + '|' + (child.parentElement === null) + '|' + child.isConnected;",
    )?;
    assert_eq!(
        value,
        Value::String("1|true|true|false|0|true|true|false".to_owned())
    );
    Ok(())
}
