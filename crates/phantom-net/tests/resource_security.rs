//! Phantom 2D-5 network/resource security regression suite.

use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use phantom_net::{DocumentLoadError, HttpUrl, NetworkClient, NetworkError, NetworkIsolationKey};

struct LocalResponse {
    headers: Vec<(&'static str, &'static str)>,
    body: &'static [u8],
}

fn spawn_once(response: LocalResponse) -> io::Result<(String, JoinHandle<io::Result<String>>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let base_url = format!("http://{address}");

    let handle = thread::spawn(move || -> io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let request = read_request(&mut stream)?;

        write!(stream, "HTTP/1.1 200 OK\r\n")?;
        for (name, value) in response.headers {
            write!(stream, "{name}: {value}\r\n")?;
        }
        write!(
            stream,
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            response.body.len()
        )?;
        stream.write_all(response.body)?;
        stream.flush()?;

        Ok(request)
    });

    Ok((base_url, handle))
}

fn read_request(stream: &mut TcpStream) -> io::Result<String> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];

    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    String::from_utf8(request).map_err(io::Error::other)
}

fn join_request(handle: JoinHandle<io::Result<String>>) -> Result<String, Box<dyn Error>> {
    match handle.join() {
        Ok(result) => Ok(result?),
        Err(_) => Err(io::Error::other("local security test server panicked").into()),
    }
}

#[test]
fn secure_document_blocks_http_subresource_before_transport() -> Result<(), Box<dyn Error>> {
    let top = HttpUrl::parse("https://example.com/")?;
    let target = HttpUrl::parse("http://example.net/image.png")?;
    let key = NetworkIsolationKey::from_top_level(&top);

    let result = NetworkClient::new().fetch_bytes_partitioned(&key, &target);

    assert!(matches!(
        result,
        Err(NetworkError::MixedContentBlocked { .. })
    ));

    Ok(())
}

#[test]
fn public_document_blocks_loopback_subresource_before_transport() -> Result<(), Box<dyn Error>> {
    let top = HttpUrl::parse("http://example.com/")?;
    let target = HttpUrl::parse("http://127.0.0.1:65535/image.png")?;
    let key = NetworkIsolationKey::from_top_level(&top);

    let result = NetworkClient::new().fetch_bytes_partitioned(&key, &target);

    assert!(matches!(
        result,
        Err(NetworkError::PrivateNetworkBlocked { .. })
    ));

    Ok(())
}

#[test]
fn explicit_private_top_level_can_load_its_own_loopback_resource() -> Result<(), Box<dyn Error>> {
    let (base_url, server) = spawn_once(LocalResponse {
        headers: vec![("Content-Type", "image/png")],
        body: b"not-a-real-png",
    })?;

    let top = HttpUrl::parse(&format!("{base_url}/page"))?;
    let target = HttpUrl::parse(&format!("{base_url}/asset"))?;
    let key = NetworkIsolationKey::from_top_level(&top);

    let response = NetworkClient::new().fetch_bytes_partitioned_with_limit(&key, &target, 1024)?;

    assert_eq!(response.status(), 200);
    assert_eq!(response.body(), b"not-a-real-png");

    let request = join_request(server)?;
    assert!(request.starts_with("GET /asset HTTP/1.1"));

    Ok(())
}

#[test]
fn gzip_expansion_is_bounded_after_decompression() -> Result<(), Box<dyn Error>> {
    const GZIP_4096_A: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xed, 0xc1, 0x01, 0x0d, 0x00,
        0x00, 0x00, 0xc2, 0xa0, 0x6c, 0xef, 0x5f, 0xca, 0x1e, 0x0e, 0x28, 0x00, 0x00, 0x00, 0xe0,
        0xdd, 0x00, 0x40, 0x34, 0xa6, 0xfe, 0x00, 0x10, 0x00, 0x00,
    ];

    let (base_url, server) = spawn_once(LocalResponse {
        headers: vec![
            ("Content-Type", "application/octet-stream"),
            ("Content-Encoding", "gzip"),
        ],
        body: GZIP_4096_A,
    })?;

    let top = HttpUrl::parse(&format!("{base_url}/page"))?;
    let target = HttpUrl::parse(&format!("{base_url}/gzip"))?;
    let key = NetworkIsolationKey::from_top_level(&top);

    let result = NetworkClient::new().fetch_bytes_partitioned_with_limit(&key, &target, 512);

    assert!(matches!(
        result,
        Err(NetworkError::DecodedBodyLimitExceeded { limit: 512 })
    ));

    let _ = join_request(server)?;
    Ok(())
}

#[test]
fn gzip_document_expansion_is_bounded_after_decompression() -> Result<(), Box<dyn Error>> {
    const GZIP_4096_A: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xed, 0xc1, 0x01, 0x0d, 0x00,
        0x00, 0x00, 0xc2, 0xa0, 0x6c, 0xef, 0x5f, 0xca, 0x1e, 0x0e, 0x28, 0x00, 0x00, 0x00, 0xe0,
        0xdd, 0x00, 0x40, 0x34, 0xa6, 0xfe, 0x00, 0x10, 0x00, 0x00,
    ];

    let (base_url, server) = spawn_once(LocalResponse {
        headers: vec![
            ("Content-Type", "text/html; charset=utf-8"),
            ("Content-Encoding", "gzip"),
        ],
        body: GZIP_4096_A,
    })?;

    let result =
        NetworkClient::with_body_limits(512, 1024).fetch_document(&format!("{base_url}/document"));

    assert!(matches!(
        result,
        Err(DocumentLoadError::Network(
            NetworkError::DecodedBodyLimitExceeded { limit: 512 }
        ))
    ));

    let _ = join_request(server)?;
    Ok(())
}

#[test]
fn brotli_expansion_is_bounded_after_decompression() -> Result<(), Box<dyn Error>> {
    const BROTLI_4096_A: &[u8] = &[
        0x1b, 0xff, 0x0f, 0xf8, 0x25, 0x82, 0xe2, 0xb1, 0x40, 0x20, 0xf7, 0x00, 0x00,
    ];

    let (base_url, server) = spawn_once(LocalResponse {
        headers: vec![
            ("Content-Type", "application/octet-stream"),
            ("Content-Encoding", "br"),
        ],
        body: BROTLI_4096_A,
    })?;

    let top = HttpUrl::parse(&format!("{base_url}/page"))?;
    let target = HttpUrl::parse(&format!("{base_url}/brotli"))?;
    let key = NetworkIsolationKey::from_top_level(&top);

    let result = NetworkClient::new().fetch_bytes_partitioned_with_limit(&key, &target, 512);

    assert!(matches!(
        result,
        Err(NetworkError::DecodedBodyLimitExceeded { limit: 512 })
    ));

    let _ = join_request(server)?;
    Ok(())
}
