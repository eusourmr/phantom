//! Phantom 2J-3 DOM Mutation / Node Creation I regression corpus.

use std::collections::BTreeMap;

use phantom_js::{
    BrowserDomElementSnapshot, BrowserDomMutation, BrowserHostSnapshot, Runtime, Value,
};

fn host() -> Result<BrowserHostSnapshot, phantom_js::RuntimeError> {
    let root = BrowserDomElementSnapshot::new(
        10,
        "main",
        BTreeMap::from([("id".to_owned(), "root".to_owned())]),
        "",
    )?
    .with_parent_node_id(Some(1))
    .with_connected(true);

    BrowserHostSnapshot::new("https://example.test/", "Example", 1024.0, 768.0, 1.0)?
        .with_dom_elements(vec![root])
}

#[test]
fn create_element_is_detached_until_append_child() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let item = document.createElement('section'); \
             item.id = 'detached'; \
             let hiddenFromQuery = document.querySelector('section') === null; \
             let hiddenFromId = document.getElementById('detached') === null; \
             document.getElementById('root').appendChild(item); \
             hiddenFromQuery && hiddenFromId && \
             document.querySelector('section') === item && \
             document.getElementById('detached') === item;",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn created_element_identity_survives_runtime_node_remap() -> Result<(), Box<dyn std::error::Error>>
{
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    runtime.execute(
        "var item = document.createElement('div'); \
         document.getElementById('root').appendChild(item);",
    )?;

    let mutations = runtime.drain_dom_mutations();
    let temporary = mutations.iter().find_map(|mutation| match mutation {
        BrowserDomMutation::CreateElement { node_id, .. } => Some(*node_id),
        _ => None,
    });
    let Some(temporary) = temporary else {
        return Err(std::io::Error::other("temporary node id missing").into());
    };

    runtime.remap_dom_node_ids(&[(temporary, 77)]);
    assert_eq!(
        runtime.execute("item === item && item.tagName === 'DIV';")?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn class_name_and_expanded_selectors_work_on_connected_nodes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let card = document.createElement('article'); \
             card.id = 'card'; card.className = 'feature selected'; \
             card.setAttribute('data-kind', 'news'); \
             document.getElementById('root').appendChild(card); \
             document.querySelector('.selected') === card && \
             document.querySelector('article.feature') === card && \
             document.querySelector('article#card') === card && \
             document.querySelector('[data-kind]') === card && \
             document.querySelector('[data-kind=news]') === card;",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn nested_created_elements_can_be_connected_as_one_subtree()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let parent = document.createElement('div'); \
             let child = document.createElement('span'); \
             child.className = 'inside'; \
             parent.appendChild(child); \
             document.getElementById('root').appendChild(parent); \
             document.querySelector('.inside') === child;",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}

#[test]
fn remove_detaches_but_preserves_javascript_identity() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.install_browser_host(&host()?)?;

    assert_eq!(
        runtime.execute(
            "let item = document.createElement('p'); item.id = 'temp'; \
             document.getElementById('root').appendChild(item); \
             let same = document.getElementById('temp') === item; \
             item.remove(); same && document.getElementById('temp') === null && item.id === 'temp';",
        )?,
        Value::Boolean(true)
    );
    Ok(())
}
