//! Deterministic document-redirect tests for Phantom 2C-10.

use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use phantom_net::{NetworkClient, NetworkError, UrlError};

#[test]
fn relative_redirect_is_followed_and_counted() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<Vec<String>> {
        let mut requests = Vec::new();

        let (mut first, _) = listener.accept()?;
        requests.push(read_request(&mut first)?);
        write_response(
            &mut first,
            "HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;

        let (mut second, _) = listener.accept()?;
        requests.push(read_request(&mut second)?);
        write_response(
            &mut second,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 16\r\nConnection: close\r\n\r\n<h1>Phantom</h1>",
        )?;

        Ok(requests)
    });

    let client = NetworkClient::new();
    let response = client.fetch_text(&format!("http://{address}/start"))?;

    assert_eq!(response.status(), 200);
    assert_eq!(response.redirect_count(), 1);
    assert_eq!(response.final_url(), format!("http://{address}/final"));
    assert_eq!(response.body(), "<h1>Phantom</h1>");

    let requests = join_server(server)?;
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("GET /start "));
    assert!(requests[1].starts_with("GET /final "));

    Ok(())
}

#[test]
fn redirect_loop_is_rejected_before_repeating_network_io() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<usize> {
        let mut count = 0_usize;

        let (mut first, _) = listener.accept()?;
        read_request(&mut first)?;
        count = count.saturating_add(1);
        write_response(
            &mut first,
            "HTTP/1.1 302 Found\r\nLocation: /b\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;

        let (mut second, _) = listener.accept()?;
        read_request(&mut second)?;
        count = count.saturating_add(1);
        write_response(
            &mut second,
            "HTTP/1.1 302 Found\r\nLocation: /a\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;

        Ok(count)
    });

    let client = NetworkClient::new();
    let error = match client.fetch_text(&format!("http://{address}/a")) {
        Ok(_) => return Err(io::Error::other("redirect loop unexpectedly succeeded").into()),
        Err(error) => error,
    };

    assert!(matches!(error, NetworkError::RedirectLoop(_)));
    assert_eq!(join_server(server)?, 2);

    Ok(())
}

#[test]
fn redirect_target_must_remain_inside_http_boundary() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        read_request(&mut stream)?;
        write_response(
            &mut stream,
            "HTTP/1.1 302 Found\r\nLocation: file:///tmp/not-allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
        Ok(())
    });

    let client = NetworkClient::new();
    let error = match client.fetch_text(&format!("http://{address}/escape")) {
        Ok(_) => return Err(io::Error::other("non-HTTP redirect unexpectedly succeeded").into()),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        NetworkError::Url(UrlError::UnsupportedScheme(_))
    ));
    join_server(server)?;

    Ok(())
}

#[test]
fn redirect_without_location_is_rejected() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        read_request(&mut stream)?;
        write_response(
            &mut stream,
            "HTTP/1.1 301 Moved Permanently\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
        Ok(())
    });

    let client = NetworkClient::new();
    let error = match client.fetch_text(&format!("http://{address}/missing")) {
        Ok(_) => {
            return Err(
                io::Error::other("redirect without Location unexpectedly succeeded").into(),
            );
        }
        Err(error) => error,
    };

    assert!(matches!(
        error,
        NetworkError::RedirectMissingLocation { status: 301 }
    ));
    join_server(server)?;

    Ok(())
}

#[test]
fn fragment_only_redirect_is_detected_as_same_network_target() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        read_request(&mut stream)?;
        write_response(
            &mut stream,
            "HTTP/1.1 302 Found\r\nLocation: #section\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
        Ok(())
    });

    let client = NetworkClient::new();
    let error = match client.fetch_text(&format!("http://{address}/page")) {
        Ok(_) => return Err(io::Error::other("fragment redirect unexpectedly succeeded").into()),
        Err(error) => error,
    };

    assert!(matches!(error, NetworkError::RedirectLoop(_)));
    join_server(server)?;

    Ok(())
}

#[test]
fn redirect_chain_is_bounded_to_ten_hops() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<usize> {
        let mut count = 0_usize;

        for hop in 0..=10 {
            let (mut stream, _) = listener.accept()?;
            read_request(&mut stream)?;
            count = count.saturating_add(1);
            let next = hop + 1;
            write_response(
                &mut stream,
                &format!(
                    "HTTP/1.1 302 Found\r\nLocation: /{next}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ),
            )?;
        }

        Ok(count)
    });

    let client = NetworkClient::new();
    let error = match client.fetch_text(&format!("http://{address}/0")) {
        Ok(_) => {
            return Err(io::Error::other("oversized redirect chain unexpectedly succeeded").into());
        }
        Err(error) => error,
    };

    assert!(matches!(
        error,
        NetworkError::RedirectLimitExceeded { limit: 10 }
    ));
    assert_eq!(join_server(server)?, 11);

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
