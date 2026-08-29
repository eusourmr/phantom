//! Phantom 2C-14 fragment URL semantics.

use std::error::Error;

use phantom_net::HttpUrl;

#[test]
fn fragment_does_not_change_network_document_identity() -> Result<(), Box<dyn Error>> {
    let left = HttpUrl::parse("https://example.com/page?q=1#one")?;
    let right = HttpUrl::parse("https://example.com/page?q=1#two")?;

    assert!(left.same_document_except_fragment(&right));
    assert_eq!(left.fragment(), Some("one"));
    assert_eq!(right.fragment(), Some("two"));

    Ok(())
}

#[test]
fn query_change_is_not_same_document_fragment_navigation() -> Result<(), Box<dyn Error>> {
    let left = HttpUrl::parse("https://example.com/page?q=1#one")?;
    let right = HttpUrl::parse("https://example.com/page?q=2#one")?;

    assert!(!left.same_document_except_fragment(&right));

    Ok(())
}

#[test]
fn fragment_can_be_removed_or_replaced_without_manual_url_parsing() -> Result<(), Box<dyn Error>> {
    let source = HttpUrl::parse("https://example.com/page#one")?;

    assert_eq!(
        source.without_fragment().as_str(),
        "https://example.com/page"
    );
    assert_eq!(
        source.with_fragment(Some("two")).as_str(),
        "https://example.com/page#two"
    );

    Ok(())
}
