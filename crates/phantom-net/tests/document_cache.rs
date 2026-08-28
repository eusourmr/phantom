//! Deterministic document-cache semantics for Phantom 2C-11.

use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use phantom_net::{CacheStatus, NetworkClient};

#[test]
fn fresh_document_cache_avoids_second_network_request() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let request = read_request(&mut stream)?;
        write_response(
            &mut stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nCache-Control: max-age=60\r\nContent-Length: 15\r\nConnection: close\r\n\r\n<h1>cached</h1>",
        )?;
        Ok(request)
    });

    let url = format!("http://{address}/fresh");
    let client = NetworkClient::new();
    let first = client.fetch_text(&url)?;
    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(first.body(), "<h1>cached</h1>");
    let request = join_server(server)?;
    assert!(request.starts_with("GET /fresh "));

    let second = client.fetch_text(&url)?;
    assert_eq!(second.cache_status(), CacheStatus::Fresh);
    assert_eq!(second.body(), "<h1>cached</h1>");

    Ok(())
}

#[test]
fn reload_bypasses_freshness_and_revalidates_etag() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<Vec<String>> {
        let mut requests = Vec::new();

        let (mut first_stream, _) = listener.accept()?;
        requests.push(read_request(&mut first_stream)?);
        write_response(
            &mut first_stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nCache-Control: max-age=600\r\nETag: \"doc-v1\"\r\nContent-Length: 15\r\nConnection: close\r\n\r\n<p>version1</p>",
        )?;

        let (mut second_stream, _) = listener.accept()?;
        requests.push(read_request(&mut second_stream)?);
        write_response(
            &mut second_stream,
            "HTTP/1.1 304 Not Modified\r\nCache-Control: max-age=600\r\nETag: \"doc-v1\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;

        Ok(requests)
    });

    let url = format!("http://{address}/reload");
    let client = NetworkClient::new();
    let first = client.fetch_text(&url)?;
    let second = client.reload_text(&url)?;

    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(second.cache_status(), CacheStatus::Revalidated);
    assert_eq!(second.body(), "<p>version1</p>");

    let requests = join_server(server)?;
    let reload = requests[1].to_ascii_lowercase();
    assert!(reload.contains("if-none-match: \"doc-v1\""));
    assert!(reload.contains("cache-control: max-age=0"));

    Ok(())
}

#[test]
fn stale_document_uses_last_modified_validator() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<Vec<String>> {
        let mut requests = Vec::new();
        let modified = "Wed, 21 Oct 2015 07:28:00 GMT";

        let (mut first_stream, _) = listener.accept()?;
        requests.push(read_request(&mut first_stream)?);
        write_response(
            &mut first_stream,
            &format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nCache-Control: max-age=0\r\nLast-Modified: {modified}\r\nContent-Length: 10\r\nConnection: close\r\n\r\n<p>old</p>"
            ),
        )?;

        let (mut second_stream, _) = listener.accept()?;
        requests.push(read_request(&mut second_stream)?);
        write_response(
            &mut second_stream,
            &format!(
                "HTTP/1.1 304 Not Modified\r\nCache-Control: max-age=60\r\nLast-Modified: {modified}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
        )?;

        Ok(requests)
    });

    let url = format!("http://{address}/modified");
    let client = NetworkClient::new();
    client.fetch_text(&url)?;
    let second = client.fetch_text(&url)?;

    assert_eq!(second.cache_status(), CacheStatus::Revalidated);
    assert_eq!(second.body(), "<p>old</p>");

    let requests = join_server(server)?;
    assert!(
        requests[1]
            .to_ascii_lowercase()
            .contains("if-modified-since: wed, 21 oct 2015 07:28:00 gmt")
    );

    Ok(())
}

#[test]
fn no_store_document_is_never_reused() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<Vec<String>> {
        let mut requests = Vec::new();

        for body in ["one", "two"] {
            let (mut stream, _) = listener.accept()?;
            requests.push(read_request(&mut stream)?);
            write_response(
                &mut stream,
                &format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ),
            )?;
        }

        Ok(requests)
    });

    let url = format!("http://{address}/private");
    let client = NetworkClient::new();
    let first = client.fetch_text(&url)?;
    let second = client.fetch_text(&url)?;

    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(second.cache_status(), CacheStatus::Miss);
    assert_eq!(first.body(), "one");
    assert_eq!(second.body(), "two");
    assert_eq!(join_server(server)?.len(), 2);

    Ok(())
}

#[test]
fn redirected_navigation_can_reuse_cached_final_document() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<Vec<String>> {
        let mut requests = Vec::new();

        let (mut direct_stream, _) = listener.accept()?;
        requests.push(read_request(&mut direct_stream)?);
        write_response(
            &mut direct_stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nCache-Control: max-age=60\r\nContent-Length: 12\r\nConnection: close\r\n\r\n<p>final</p>",
        )?;

        let (mut redirect_stream, _) = listener.accept()?;
        requests.push(read_request(&mut redirect_stream)?);
        write_response(
            &mut redirect_stream,
            "HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;

        Ok(requests)
    });

    let client = NetworkClient::new();
    let direct = format!("http://{address}/final");
    let redirect = format!("http://{address}/start");
    client.fetch_text(&direct)?;
    let response = client.fetch_text(&redirect)?;

    assert_eq!(response.final_url(), direct);
    assert_eq!(response.redirect_count(), 1);
    assert_eq!(response.cache_status(), CacheStatus::Fresh);
    assert_eq!(response.body(), "<p>final</p>");
    assert_eq!(join_server(server)?.len(), 2);

    Ok(())
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
