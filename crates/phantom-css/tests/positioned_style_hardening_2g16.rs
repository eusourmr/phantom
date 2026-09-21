//! Phantom 2G-16 positioned-style hardening corpus.

// PHANTOM_2G16_POSITIONED_STYLE_HARDENING

use std::collections::BTreeMap;
use std::io;

use phantom_css::{Position, ZIndex, compute_styles};
use phantom_dom::{Document, ElementData, NodeId, NodeKind};

fn append_styled(
    document: &mut Document,
    parent: NodeId,
    style: &str,
) -> Result<NodeId, phantom_dom::DomError> {
    let mut attributes = BTreeMap::new();
    attributes.insert("style".to_owned(), style.to_owned());
    document.append_child(
        parent,
        NodeKind::Element(ElementData::with_attributes("div", attributes)),
    )
}

#[test]
fn style_interner_keeps_distinct_positioned_z_index_values()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let low = append_styled(&mut document, root, "position:relative;z-index:-3")?;
    let zero = append_styled(&mut document, root, "position:relative;z-index:0")?;
    let high = append_styled(&mut document, root, "position:relative;z-index:7")?;

    let styles = compute_styles(&document);
    assert_eq!(
        styles
            .get(low)
            .ok_or_else(|| io::Error::other("low z-index style missing"))?
            .z_index(),
        ZIndex::Integer(-3)
    );
    assert_eq!(
        styles
            .get(zero)
            .ok_or_else(|| io::Error::other("zero z-index style missing"))?
            .z_index(),
        ZIndex::Integer(0)
    );
    assert_eq!(
        styles
            .get(high)
            .ok_or_else(|| io::Error::other("high z-index style missing"))?
            .z_index(),
        ZIndex::Integer(7)
    );
    Ok(())
}

#[test]
fn unset_z_index_resolves_to_auto_instead_of_inheriting() -> Result<(), Box<dyn std::error::Error>>
{
    let mut document = Document::new();
    let root = document.root();
    let parent = append_styled(&mut document, root, "position:relative;z-index:12")?;
    let child = append_styled(&mut document, parent, "position:absolute;z-index:unset")?;

    let styles = compute_styles(&document);
    let child_style = styles
        .get(child)
        .ok_or_else(|| io::Error::other("unset child style missing"))?;

    assert_eq!(child_style.position(), Position::Absolute);
    assert_eq!(child_style.z_index(), ZIndex::Auto);
    Ok(())
}

#[test]
fn static_z_index_remains_computed_but_position_stays_static()
-> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    let root = document.root();
    let node = append_styled(&mut document, root, "z-index:99")?;

    let styles = compute_styles(&document);
    let style = styles
        .get(node)
        .ok_or_else(|| io::Error::other("static z-index style missing"))?;

    assert_eq!(style.position(), Position::Static);
    assert_eq!(style.z_index(), ZIndex::Integer(99));
    Ok(())
}
