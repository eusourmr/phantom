//! Network Isolation Key/cache regressions adapted to the 2D-5 private-network policy.
//!
//! Local test resources use explicit local top-level origins so the tests exercise
//! cache partitioning without pretending a public page may reach loopback.
//!
//! 2G-15 R2 hardens the local HTTP fixture against the transport retry already
//! supported by `NetworkClient`. The server remains alive until the test signals
//! completion, so one transient local socket reset cannot consume the listener
//! budget and turn a cache-isolation assertion into a spurious ConnectionRefused.

use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use phantom_net::{CacheStatus, HttpUrl, NetworkClient, NetworkIsolationKey};

const PARTITION_SERVER_DEADLINE: Duration = Duration::from_secs(30);

#[test]
fn same_partition_reuses_fresh_binary_response() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let (stop_tx, stop_rx) = mpsc::channel();
    let server = thread::spawn(move || -> io::Result<usize> {
        serve_partitioning_requests(listener, &stop_rx)
    });

    let resource = HttpUrl::parse(&format!("http://{address}/shared.png"))?;
    let top = HttpUrl::parse("http://127.0.0.1:41001/page")?;
    let key = NetworkIsolationKey::from_top_level(&top);
    let client = NetworkClient::new();

    let first = client.fetch_bytes_partitioned(&key, &resource)?;
    let second = client.fetch_bytes_partitioned(&key, &resource)?;
    stop_tx.send(())?;

    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(second.cache_status(), CacheStatus::Fresh);
    assert_eq!(first.body(), b"partitioned");
    assert_eq!(second.body(), b"partitioned");
    assert!(join_server(server)? >= 1);

    Ok(())
}

#[test]
fn different_top_level_partitions_do_not_share_the_same_resource() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let (stop_tx, stop_rx) = mpsc::channel();
    let server = thread::spawn(move || -> io::Result<usize> {
        serve_partitioning_requests(listener, &stop_rx)
    });

    let resource = HttpUrl::parse(&format!("http://{address}/third-party.png"))?;
    let top_a = HttpUrl::parse("http://127.0.0.1:41001/page")?;
    let top_b = HttpUrl::parse("http://127.0.0.1:41002/page")?;
    let key_a = NetworkIsolationKey::from_top_level(&top_a);
    let key_b = NetworkIsolationKey::from_top_level(&top_b);
    let client = NetworkClient::new();

    let first = client.fetch_bytes_partitioned(&key_a, &resource)?;
    let second = client.fetch_bytes_partitioned(&key_b, &resource)?;
    stop_tx.send(())?;

    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(second.cache_status(), CacheStatus::Miss);
    assert_eq!(first.body(), b"partitioned");
    assert_eq!(second.body(), b"partitioned");
    assert!(join_server(server)? >= 2);

    Ok(())
}

#[test]
fn frame_dimension_also_partitions_network_state() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let (stop_tx, stop_rx) = mpsc::channel();
    let server = thread::spawn(move || -> io::Result<usize> {
        serve_partitioning_requests(listener, &stop_rx)
    });

    let resource = HttpUrl::parse(&format!("http://{address}/frame-resource.png"))?;
    let top = HttpUrl::parse("http://127.0.0.1:41003/page")?;
    let frame_a = HttpUrl::parse("https://frame-a.example/embed")?;
    let frame_b = HttpUrl::parse("https://frame-b.example/embed")?;
    let key_a = NetworkIsolationKey::new(&top, &frame_a);
    let key_b = NetworkIsolationKey::new(&top, &frame_b);
    let client = NetworkClient::new();

    let first = client.fetch_bytes_partitioned(&key_a, &resource)?;
    let second = client.fetch_bytes_partitioned(&key_b, &resource)?;
    stop_tx.send(())?;

    assert_eq!(first.cache_status(), CacheStatus::Miss);
    assert_eq!(second.cache_status(), CacheStatus::Miss);
    assert_eq!(first.body(), b"partitioned");
    assert_eq!(second.body(), b"partitioned");
    assert!(join_server(server)? >= 2);

    Ok(())
}

fn serve_partitioning_requests(listener: TcpListener, stop_rx: &Receiver<()>) -> io::Result<usize> {
    let deadline = Instant::now() + PARTITION_SERVER_DEADLINE;
    let mut served = 0_usize;

    while Instant::now() < deadline {
        match stop_rx.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }

        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Err(error) = read_request(&mut stream) {
                    if is_transient_test_socket_error(&error) {
                        continue;
                    }
                    return Err(error);
                }

                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: image/png\r\n",
                    "Cache-Control: max-age=60\r\n",
                    "ETag: \"partitioned\"\r\n",
                    "Content-Length: 11\r\n",
                    "Connection: close\r\n\r\n",
                    "partitioned"
                );

                match write_response(&mut stream, response) {
                    Ok(()) => served = served.saturating_add(1),
                    Err(error) if is_transient_test_socket_error(&error) => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }

    Ok(served)
}

fn is_transient_test_socket_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::TimedOut
            | io::ErrorKind::WouldBlock
    )
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
