//! Deterministic HTTP cache and image-recovery tests for Phantom 2C-7.

use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use phantom_net::{CacheStatus, HttpUrl, NetworkClient, NetworkIsolationKey};

#[test]
fn fresh_binary_cache_avoids_a_second_network_request() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let request = read_request(&mut stream)?;
        write_response(
            &mut stream,
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: max-age=60\r\nETag: \"fresh-v1\"\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
        )?;
        Ok(request)
    });

    let url = HttpUrl::parse(&format!("http://{address}/fresh.png"))?;
    let client = NetworkClient::new();
    let first = fetch_partitioned(&client, &url)?;

    assert_eq!(first.body(), b"abc");
    assert_eq!(first.cache_status(), CacheStatus::Miss);

    let request = join_server(server)?;
    assert!(request.starts_with("GET /fresh.png "));

    let second = fetch_partitioned(&client, &url)?;
    assert_eq!(second.body(), b"abc");
    assert_eq!(second.cache_status(), CacheStatus::Fresh);

    Ok(())
}

#[test]
fn stale_etag_entry_revalidates_with_304() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<Vec<String>> {
        let mut requests = Vec::new();

        let (mut first_stream, _) = listener.accept()?;
        requests.push(read_request(&mut first_stream)?);
        write_response(
            &mut first_stream,
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: max-age=0\r\nETag: \"etag-v1\"\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
        )?;

        let (mut second_stream, _) = listener.accept()?;
        requests.push(read_request(&mut second_stream)?);
        write_response(
            &mut second_stream,
            "HTTP/1.1 304 Not Modified\r\nCache-Control: max-age=60\r\nETag: \"etag-v1\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;

        Ok(requests)
    });

    let url = HttpUrl::parse(&format!("http://{address}/etag.png"))?;
    let client = NetworkClient::new();
    let first = fetch_partitioned(&client, &url)?;
    let second = fetch_partitioned(&client, &url)?;

    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(second.cache_status(), CacheStatus::Revalidated);
    assert_eq!(second.body(), b"abc");

    let requests = join_server(server)?;
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1]
            .to_ascii_lowercase()
            .contains("if-none-match: \"etag-v1\"")
    );

    Ok(())
}

#[test]
fn transient_server_failure_is_retried_once() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<usize> {
        let mut requests = 0_usize;

        let (mut first_stream, _) = listener.accept()?;
        read_request(&mut first_stream)?;
        requests = requests.saturating_add(1);
        write_response(
            &mut first_stream,
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;

        let (mut second_stream, _) = listener.accept()?;
        read_request(&mut second_stream)?;
        requests = requests.saturating_add(1);
        write_response(
            &mut second_stream,
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: no-store\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        )?;

        Ok(requests)
    });

    let url = HttpUrl::parse(&format!("http://{address}/retry.png"))?;
    let client = NetworkClient::new();
    let response = fetch_partitioned(&client, &url)?;

    assert_eq!(response.status(), 200);
    assert_eq!(response.body(), b"ok");
    assert_eq!(response.cache_status(), CacheStatus::Miss);
    assert_eq!(join_server(server)?, 2);

    Ok(())
}

#[test]
fn stale_if_error_recovers_after_transport_failure() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        read_request(&mut stream)?;
        write_response(
            &mut stream,
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: max-age=0, stale-if-error=60\r\nETag: \"recover-v1\"\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
        )?;
        Ok(())
    });

    let url = HttpUrl::parse(&format!("http://{address}/recover.png"))?;
    let client = NetworkClient::new();
    let first = fetch_partitioned(&client, &url)?;

    assert_eq!(first.body(), b"abc");
    assert_eq!(first.cache_status(), CacheStatus::Miss);
    join_server(server)?;

    let recovered = fetch_partitioned(&client, &url)?;
    assert_eq!(recovered.body(), b"abc");
    assert_eq!(recovered.cache_status(), CacheStatus::StaleIfError);

    Ok(())
}

#[test]
fn must_revalidate_prevents_stale_if_error_fallback() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        read_request(&mut stream)?;
        write_response(
            &mut stream,
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: max-age=0, stale-if-error=60, must-revalidate\r\nETag: \"strict-v1\"\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
        )?;
        Ok(())
    });

    let url = HttpUrl::parse(&format!("http://{address}/strict.png"))?;
    let client = NetworkClient::new();
    let first = fetch_partitioned(&client, &url)?;
    assert_eq!(first.body(), b"abc");
    join_server(server)?;

    assert!(fetch_partitioned(&client, &url).is_err());

    Ok(())
}

fn fetch_partitioned(
    client: &NetworkClient,
    url: &HttpUrl,
) -> Result<phantom_net::BinaryResponse, phantom_net::NetworkError> {
    let isolation_key = NetworkIsolationKey::from_top_level(url);
    client.fetch_bytes_partitioned(&isolation_key, url)
}

fn read_request(stream: &mut TcpStream) -> io::Result<String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];

    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        bytes.extend_from_slice(&buffer[..read]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn write_response(stream: &mut TcpStream, response: &str) -> io::Result<()> {
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn join_server<T>(server: thread::JoinHandle<io::Result<T>>) -> Result<T, Box<dyn Error>> {
    match server.join() {
        Ok(result) => Ok(result?),
        Err(_) => Err(io::Error::other("test HTTP server thread panicked").into()),
    }
}
