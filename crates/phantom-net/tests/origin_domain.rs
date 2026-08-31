//! Phantom 2D-2 typed URL/origin domain compatibility tests.

use std::error::Error;

use phantom_net::{HttpUrl, NetworkIsolationKey, OriginScheme};

#[test]
fn default_ports_canonicalize_to_the_same_origin() -> Result<(), Box<dyn Error>> {
    let explicit_https = HttpUrl::parse("https://EXAMPLE.com:443/a")?;
    let implicit_https = HttpUrl::parse("https://example.com/b")?;
    let explicit_http = HttpUrl::parse("http://example.com:80/a")?;
    let implicit_http = HttpUrl::parse("http://example.com/b")?;

    assert!(explicit_https.same_origin(&implicit_https));
    assert!(explicit_http.same_origin(&implicit_http));
    assert_eq!(explicit_https.origin().as_str(), "https://example.com");
    assert_eq!(explicit_http.origin().as_str(), "http://example.com");

    Ok(())
}

#[test]
fn scheme_host_and_effective_port_define_origin_identity() -> Result<(), Box<dyn Error>> {
    let secure = HttpUrl::parse("https://example.com/")?;
    let insecure = HttpUrl::parse("http://example.com/")?;
    let other_host = HttpUrl::parse("https://www.example.com/")?;
    let other_port = HttpUrl::parse("https://example.com:444/")?;

    assert!(!secure.same_origin(&insecure));
    assert!(!secure.same_origin(&other_host));
    assert!(!secure.same_origin(&other_port));

    let origin = other_port.origin();
    assert_eq!(origin.scheme(), OriginScheme::Https);
    assert_eq!(origin.host(), "example.com");
    assert_eq!(origin.effective_port(), 444);
    assert_eq!(origin.as_str(), "https://example.com:444");

    Ok(())
}

#[test]
fn network_isolation_key_exposes_typed_origins_without_breaking_string_accessors()
-> Result<(), Box<dyn Error>> {
    let top = HttpUrl::parse("https://container.example:443/page")?;
    let frame = HttpUrl::parse("https://frame.example:8443/embed")?;
    let key = NetworkIsolationKey::new(&top, &frame);

    assert_eq!(key.top_level_origin(), "https://container.example");
    assert_eq!(key.frame_origin(), "https://frame.example:8443");
    assert!(key.top_level_origin_value().same_origin(&top.origin()));
    assert!(key.frame_origin_value().same_origin(&frame.origin()));

    Ok(())
}
