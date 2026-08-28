//! Deterministic Network Isolation Key and partitioned HTTP-cache tests for Phantom 2C-8.

use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use phantom_net::{CacheStatus, HttpUrl, NetworkClient, NetworkIsolationKey};

#[test]
fn same_partition_reuses_fresh_binary_response() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<usize> {
        let (mut stream, _) = listener.accept()?;
        read_request(&mut stream)?;
        write_response(
            &mut stream,
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: max-age=60\r\nETag: \"partition-v1\"\r\nContent-Length: 3\r\nConnection: close\r\n\r\none",
        )?;
        Ok(1)
    });

    let resource = HttpUrl::parse(&format!("http://{address}/shared.png"))?;
    let top = HttpUrl::parse("https://site-a.example/page")?;
    let key = NetworkIsolationKey::from_top_level(&top);
    let client = NetworkClient::new();

    let first = client.fetch_bytes_partitioned(&key, &resource)?;
    let second = client.fetch_bytes_partitioned(&key, &resource)?;

    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(second.cache_status(), CacheStatus::Fresh);
    assert_eq!(second.body(), b"one");
    assert_eq!(join_server(server)?, 1);

    Ok(())
}

#[test]
fn different_top_level_partitions_do_not_share_the_same_resource() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<usize> { serve_up_to_two_requests(listener) });

    let resource = HttpUrl::parse(&format!("http://{address}/third-party.png"))?;
    let top_a = HttpUrl::parse("https://site-a.example/page")?;
    let top_b = HttpUrl::parse("https://site-b.example/page")?;
    let key_a = NetworkIsolationKey::from_top_level(&top_a);
    let key_b = NetworkIsolationKey::from_top_level(&top_b);
    let client = NetworkClient::new();

    let first = client.fetch_bytes_partitioned(&key_a, &resource)?;
    let second = client.fetch_bytes_partitioned(&key_b, &resource)?;

    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(first.body(), b"one");
    assert_eq!(second.cache_status(), CacheStatus::Miss);
    assert_eq!(second.body(), b"two");
    assert_eq!(join_server(server)?, 2);

    Ok(())
}

#[test]
fn frame_dimension_also_partitions_network_state() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<usize> { serve_up_to_two_requests(listener) });

    let resource = HttpUrl::parse(&format!("http://{address}/frame-resource.png"))?;
    let top = HttpUrl::parse("https://container.example/page")?;
    let frame_a = HttpUrl::parse("https://frame-a.example/embed")?;
    let frame_b = HttpUrl::parse("https://frame-b.example/embed")?;
    let key_a = NetworkIsolationKey::new(&top, &frame_a);
    let key_b = NetworkIsolationKey::new(&top, &frame_b);
    let client = NetworkClient::new();

    let first = client.fetch_bytes_partitioned(&key_a, &resource)?;
    let second = client.fetch_bytes_partitioned(&key_b, &resource)?;

    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(second.cache_status(), CacheStatus::Miss);
    assert_eq!(join_server(server)?, 2);

    Ok(())
}

fn serve_up_to_two_requests(listener: TcpListener) -> io::Result<usize> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut served = 0_usize;

    while served < 2 && Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                read_request(&mut stream)?;
                served = served.saturating_add(1);
                let body = if served == 1 { "one" } else { "two" };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: max-age=60\r\nETag: \"partition-{served}\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len(),
                );
                write_response(&mut stream, &response)?;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }

    Ok(served)
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
