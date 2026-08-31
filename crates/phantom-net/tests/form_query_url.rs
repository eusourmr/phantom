//! Phantom 2C-13 query serialization tests.

use std::error::Error;

use phantom_net::HttpUrl;

#[test]
fn query_pairs_are_encoded_by_url_crate() -> Result<(), Box<dyn Error>> {
    let base = HttpUrl::parse("https://example.com/search?old=1#section")?;
    let fields = vec![
        ("q".to_owned(), "phantom browser".to_owned()),
        ("lang".to_owned(), "pt-BR".to_owned()),
        ("symbol".to_owned(), "a&b=c".to_owned()),
    ];

    let target = base.with_query_pairs(&fields);

    assert_eq!(
        target.as_str(),
        "https://example.com/search?q=phantom+browser&lang=pt-BR&symbol=a%26b%3Dc#section"
    );

    Ok(())
}

#[test]
fn empty_form_pairs_remove_existing_query() -> Result<(), Box<dyn Error>> {
    let base = HttpUrl::parse("https://example.com/search?old=1")?;
    let target = base.with_query_pairs(&[]);

    assert_eq!(target.as_str(), "https://example.com/search");

    Ok(())
}
