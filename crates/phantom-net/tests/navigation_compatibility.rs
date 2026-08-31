//! Phantom 2D-4 network/document navigation compatibility suite.
//!
//! The server is local, deterministic and dependency-free. These tests validate
//! the interaction between manual redirects, document admission and the 2C
//! document cache used by the 2D navigation pipeline.

use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use phantom_net::{CacheStatus, DocumentLoadError, NetworkClient, NetworkError};

struct ResponseSpec {
    status: &'static str,
    headers: Vec<(&'static str, &'static str)>,
    body: &'static [u8],
}

fn response(
    status: &'static str,
    headers: Vec<(&'static str, &'static str)>,
    body: &'static [u8],
) -> ResponseSpec {
    ResponseSpec {
        status,
        headers,
        body,
    }
}

fn spawn_sequence(
    steps: Vec<ResponseSpec>,
) -> io::Result<(String, JoinHandle<io::Result<Vec<String>>>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let base_url = format!("http://{address}");

    let handle = thread::spawn(move || -> io::Result<Vec<String>> {
        let mut requests = Vec::with_capacity(steps.len());

        for step in steps {
            let (mut stream, _) = listener.accept()?;
            requests.push(read_request(&mut stream)?);

            write!(stream, "HTTP/1.1 {}\r\n", step.status)?;

            for (name, value) in step.headers {
                write!(stream, "{name}: {value}\r\n")?;
            }

            write!(
                stream,
                "Content-Length: {}\r\nConnection: close\r\n\r\n",
                step.body.len()
            )?;
            stream.write_all(step.body)?;
            stream.flush()?;
        }

        Ok(requests)
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

fn join_requests(
    handle: JoinHandle<io::Result<Vec<String>>>,
) -> Result<Vec<String>, Box<dyn Error>> {
    match handle.join() {
        Ok(result) => Ok(result?),
        Err(_) => Err(io::Error::other("local navigation test server panicked").into()),
    }
}

#[test]
fn relative_redirect_chain_commits_final_url_and_hop_count() -> Result<(), Box<dyn Error>> {
    let (base_url, server) = spawn_sequence(vec![
        response("302 Found", vec![("Location", "/middle")], b""),
        response(
            "307 Temporary Redirect",
            vec![("Location", "final?source=redirect")],
            b"",
        ),
        response(
            "200 OK",
            vec![("Content-Type", "text/html; charset=utf-8")],
            b"<!doctype html><title>Final</title>",
        ),
    ])?;

    let document = NetworkClient::new().fetch_document(&format!("{base_url}/start"))?;

    assert_eq!(document.status(), 200);
    assert_eq!(document.redirect_count(), 2);
    assert_eq!(
        document.final_url(),
        format!("{base_url}/final?source=redirect")
    );
    assert_eq!(document.body(), "<!doctype html><title>Final</title>");

    let requests = join_requests(server)?;
    assert_eq!(requests.len(), 3);
    assert!(requests[0].starts_with("GET /start HTTP/1.1"));
    assert!(requests[1].starts_with("GET /middle HTTP/1.1"));
    assert!(requests[2].starts_with("GET /final?source=redirect HTTP/1.1"));

    Ok(())
}

#[test]
fn redirect_loop_is_rejected_before_reissuing_seen_url() -> Result<(), Box<dyn Error>> {
    let (base_url, server) = spawn_sequence(vec![
        response("302 Found", vec![("Location", "/b")], b""),
        response("302 Found", vec![("Location", "/a")], b""),
    ])?;

    let result = NetworkClient::new().fetch_document(&format!("{base_url}/a"));

    match result {
        Err(DocumentLoadError::Network(NetworkError::RedirectLoop(url))) => {
            assert_eq!(url, format!("{base_url}/a"));
        }
        Ok(_) => {
            return Err(io::Error::other("redirect loop unexpectedly produced a document").into());
        }
        Err(error) => {
            return Err(
                io::Error::other(format!("unexpected redirect-loop error: {error}")).into(),
            );
        }
    }

    let requests = join_requests(server)?;
    assert_eq!(requests.len(), 2);

    Ok(())
}

#[test]
fn redirect_without_location_is_rejected_explicitly() -> Result<(), Box<dyn Error>> {
    let (base_url, server) = spawn_sequence(vec![response("302 Found", Vec::new(), b"")])?;

    let result = NetworkClient::new().fetch_document(&format!("{base_url}/missing"));

    match result {
        Err(DocumentLoadError::Network(NetworkError::RedirectMissingLocation { status })) => {
            assert_eq!(status, 302);
        }
        Ok(_) => {
            return Err(io::Error::other(
                "redirect without Location unexpectedly produced a document",
            )
            .into());
        }
        Err(error) => {
            return Err(
                io::Error::other(format!("unexpected missing-Location error: {error}")).into(),
            );
        }
    }

    let requests = join_requests(server)?;
    assert_eq!(requests.len(), 1);

    Ok(())
}

#[test]
fn fresh_document_navigation_reuses_cache_without_second_request() -> Result<(), Box<dyn Error>> {
    let (base_url, server) = spawn_sequence(vec![response(
        "200 OK",
        vec![
            ("Content-Type", "text/html"),
            ("Cache-Control", "max-age=3600"),
            ("ETag", "\"fresh-v1\""),
        ],
        b"<!doctype html><p>cached</p>",
    )])?;

    let client = NetworkClient::new();
    let url = format!("{base_url}/cached");

    let first = client.fetch_document(&url)?;
    let second = client.fetch_document(&url)?;

    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(second.cache_status(), CacheStatus::Fresh);
    assert_eq!(second.body(), first.body());

    let requests = join_requests(server)?;
    assert_eq!(requests.len(), 1);

    Ok(())
}

#[test]
fn reload_revalidates_cached_document_and_reuses_304_body() -> Result<(), Box<dyn Error>> {
    let (base_url, server) = spawn_sequence(vec![
        response(
            "200 OK",
            vec![
                ("Content-Type", "text/html"),
                ("Cache-Control", "max-age=3600"),
                ("ETag", "\"reload-v1\""),
            ],
            b"<!doctype html><p>version one</p>",
        ),
        response(
            "304 Not Modified",
            vec![("Cache-Control", "max-age=3600"), ("ETag", "\"reload-v1\"")],
            b"",
        ),
    ])?;

    let client = NetworkClient::new();
    let url = format!("{base_url}/reload");

    let first = client.fetch_document(&url)?;
    let reloaded = client.reload_document(&url)?;

    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(reloaded.cache_status(), CacheStatus::Revalidated);
    assert_eq!(reloaded.body(), first.body());

    let requests = join_requests(server)?;
    assert_eq!(requests.len(), 2);

    let reload_request = requests[1].to_ascii_lowercase();
    assert!(reload_request.contains("cache-control: max-age=0"));
    assert!(reload_request.contains("if-none-match: \"reload-v1\""));

    Ok(())
}

#[test]
fn html_error_status_remains_renderable_document() -> Result<(), Box<dyn Error>> {
    let (base_url, server) = spawn_sequence(vec![response(
        "404 Not Found",
        vec![("Content-Type", "text/html")],
        b"<!doctype html><title>Not found</title>",
    )])?;

    let document = NetworkClient::new().fetch_document(&format!("{base_url}/not-found"))?;

    assert_eq!(document.status(), 404);
    assert_eq!(document.body(), "<!doctype html><title>Not found</title>");

    let requests = join_requests(server)?;
    assert_eq!(requests.len(), 1);

    Ok(())
}

#[test]
fn url_fragment_is_preserved_as_requested_state_but_not_sent_over_http()
-> Result<(), Box<dyn Error>> {
    let (base_url, server) = spawn_sequence(vec![response(
        "200 OK",
        vec![("Content-Type", "text/html")],
        b"<!doctype html><p>fragment</p>",
    )])?;

    let document = NetworkClient::new().fetch_document(&format!("{base_url}/page#section"))?;

    assert_eq!(document.requested_http_url().fragment(), Some("section"));
    assert_eq!(document.final_http_url().fragment(), None);

    let requests = join_requests(server)?;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /page HTTP/1.1"));
    assert!(!requests[0].contains("#section"));

    Ok(())
}
