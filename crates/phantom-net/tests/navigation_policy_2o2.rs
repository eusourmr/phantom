//! Phantom 2O-2: deterministic URL, origin, and redirect policy regressions.
//!
//! This suite uses the existing phantom-net public boundary and local-only I/O.
//! No external host or DNS lookup is required.

use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use phantom_net::{HttpUrl, NetworkClient, NetworkError, NetworkIsolationKey, UrlError};

#[test]
fn relative_reference_cannot_escape_http_transport_boundary() -> Result<(), Box<dyn Error>> {
    let base = HttpUrl::parse("https://example.com/dir/page")?;
    for reference in [
        "file:///tmp/secret",
        "data:text/plain,secret",
        "javascript:alert(1)",
    ] {
        assert!(
            matches!(base.resolve(reference), Err(UrlError::UnsupportedScheme(_))),
            "unexpectedly admitted reference: {reference}"
        );
    }
    Ok(())
}

#[test]
fn relative_reference_cannot_introduce_credentials() -> Result<(), Box<dyn Error>> {
    let base = HttpUrl::parse("https://example.com/start")?;
    assert!(matches!(
        base.resolve("https://user:password@example.org/private"),
        Err(UrlError::CredentialsNotAllowed)
    ));
    assert!(matches!(
        base.resolve("//user:password@example.org/private"),
        Err(UrlError::CredentialsNotAllowed)
    ));
    Ok(())
}

#[test]
fn path_query_and_fragment_do_not_change_origin() -> Result<(), Box<dyn Error>> {
    let first = HttpUrl::parse("https://EXAMPLE.com:443/a?x=1#first")?;
    let second = HttpUrl::parse("https://example.com/b?y=2#second")?;
    assert!(first.same_origin(&second));
    assert_eq!(first.origin().as_str(), "https://example.com");
    assert!(!first.same_document_except_fragment(&second));
    Ok(())
}

#[test]
fn scheme_and_port_changes_do_change_origin() -> Result<(), Box<dyn Error>> {
    let base = HttpUrl::parse("https://example.com/page")?;
    let downgraded = HttpUrl::parse("http://example.com/page")?;
    let other_port = HttpUrl::parse("https://example.com:444/page")?;
    assert!(!base.same_origin(&downgraded));
    assert!(!base.same_origin(&other_port));
    Ok(())
}

#[test]
fn network_partition_keeps_top_level_and_frame_origins_distinct() -> Result<(), Box<dyn Error>> {
    let top = HttpUrl::parse("https://example.com/")?;
    let frame = HttpUrl::parse("https://frame.example.com/")?;
    let key = NetworkIsolationKey::new(&top, &frame);
    assert_eq!(key.top_level_origin(), "https://example.com");
    assert_eq!(key.frame_origin(), "https://frame.example.com");
    assert!(
        !key.top_level_origin_value()
            .same_origin(key.frame_origin_value())
    );
    Ok(())
}

#[test]
fn fragment_only_reference_retains_network_document_identity() -> Result<(), Box<dyn Error>> {
    let base = HttpUrl::parse("https://example.com/path?q=1#old")?;
    let target = base.resolve("#new")?;
    assert!(base.same_document_except_fragment(&target));
    assert_eq!(target.fragment(), Some("new"));
    assert_eq!(target.without_fragment(), base.without_fragment());
    Ok(())
}

fn read_headers(stream: &mut TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut data = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "request ended before headers",
            ));
        }
        data.extend_from_slice(&chunk[..count]);
        if data.windows(4).any(|part| part == b"\r\n\r\n") {
            return Ok(());
        }
        if data.len() > 16 * 1024 {
            return Err(io::Error::other(
                "test request headers exceeded local budget",
            ));
        }
    }
}

#[test]
fn redirect_cannot_introduce_credentialed_target() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        read_headers(&mut stream)?;
        stream.write_all(
            b"HTTP/1.1 302 Found\r\nLocation: http://user:secret@example.org/hidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
        stream.flush()
    });

    let result = NetworkClient::new().fetch_text(&format!("http://{address}/start"));
    assert!(matches!(
        result,
        Err(NetworkError::Url(UrlError::CredentialsNotAllowed))
    ));

    match server.join() {
        Ok(result) => result?,
        Err(_) => return Err(io::Error::other("local redirect server panicked").into()),
    }
    Ok(())
}
