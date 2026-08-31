//! Phantom 2D-1 hardened top-level document-loader cases.

use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use phantom_net::{DocumentLoadError, DocumentMediaType, NetworkClient, NetworkError};

#[test]
fn html_utf8_document_is_admitted() -> Result<(), Box<dyn Error>> {
    let body = "<!doctype html><title>Phantom</title>";
    let url = serve_once(
        "200 OK",
        &[("Content-Type", "text/html; charset=UTF-8")],
        body.as_bytes(),
    )?;

    let document = NetworkClient::new().fetch_document(&url)?;

    assert_eq!(document.media_type(), DocumentMediaType::Html);
    assert_eq!(document.body(), body);

    Ok(())
}

#[test]
fn xhtml_document_is_admitted() -> Result<(), Box<dyn Error>> {
    let body = "<html xmlns=\"http://www.w3.org/1999/xhtml\"></html>";
    let url = serve_once(
        "200 OK",
        &[("Content-Type", "application/xhtml+xml")],
        body.as_bytes(),
    )?;

    let document = NetworkClient::new().fetch_document(&url)?;

    assert_eq!(document.media_type(), DocumentMediaType::Xhtml);

    Ok(())
}

#[test]
fn utf8_bom_is_removed_before_engine_text() -> Result<(), Box<dyn Error>> {
    let mut body = vec![0xEF, 0xBB, 0xBF];
    body.extend_from_slice(b"<!doctype html><p>bom</p>");

    let url = serve_once("200 OK", &[("Content-Type", "text/html")], &body)?;

    let document = NetworkClient::new().fetch_document(&url)?;

    assert_eq!(document.body(), "<!doctype html><p>bom</p>");

    Ok(())
}

#[test]
fn unsupported_explicit_media_type_is_rejected() -> Result<(), Box<dyn Error>> {
    let url = serve_once(
        "200 OK",
        &[("Content-Type", "application/pdf")],
        b"%PDF-not-a-real-document",
    )?;

    let result = NetworkClient::new().fetch_document(&url);

    match result {
        Err(DocumentLoadError::UnsupportedMediaType(media_type)) => {
            assert_eq!(media_type, "application/pdf");
        }
        Ok(_) => {
            return Err(io::Error::other(
                "application/pdf unexpectedly admitted as a web document",
            )
            .into());
        }
        Err(error) => {
            return Err(io::Error::other(format!("unexpected document error: {error}")).into());
        }
    }

    Ok(())
}

#[test]
fn missing_content_type_uses_bounded_html_sniff() -> Result<(), Box<dyn Error>> {
    let url = serve_once(
        "200 OK",
        &[],
        b"  \n<!DOCTYPE html><html><body>ok</body></html>",
    )?;

    let document = NetworkClient::new().fetch_document(&url)?;

    assert_eq!(document.media_type(), DocumentMediaType::Html);

    Ok(())
}

#[test]
fn missing_content_type_non_html_is_rejected() -> Result<(), Box<dyn Error>> {
    let url = serve_once("200 OK", &[], b"{\"kind\":\"json\"}")?;

    let result = NetworkClient::new().fetch_document(&url);

    assert!(matches!(
        result,
        Err(DocumentLoadError::UnidentifiedMediaType)
    ));

    Ok(())
}

#[test]
fn legacy_declared_charset_is_explicitly_rejected() -> Result<(), Box<dyn Error>> {
    let url = serve_once(
        "200 OK",
        &[("Content-Type", "text/html; charset=iso-8859-1")],
        b"<html></html>",
    )?;

    let result = NetworkClient::new().fetch_document(&url);

    match result {
        Err(DocumentLoadError::Network(NetworkError::UnsupportedCharset(charset))) => {
            assert_eq!(charset, "iso-8859-1");
        }
        Ok(_) => {
            return Err(io::Error::other("legacy charset unexpectedly admitted").into());
        }
        Err(error) => {
            return Err(io::Error::other(format!("unexpected charset error: {error}")).into());
        }
    }

    Ok(())
}

#[test]
fn invalid_utf8_is_not_lossily_replaced() -> Result<(), Box<dyn Error>> {
    let url = serve_once(
        "200 OK",
        &[("Content-Type", "text/html; charset=utf-8")],
        &[b'<', b'p', b'>', 0xFF, b'<', b'/', b'p', b'>'],
    )?;

    let result = NetworkClient::new().fetch_document(&url);

    assert!(matches!(
        result,
        Err(DocumentLoadError::Network(
            NetworkError::InvalidTextEncoding
        ))
    ));

    Ok(())
}

#[test]
fn no_content_status_has_explicit_document_error() -> Result<(), Box<dyn Error>> {
    let url = serve_once("204 No Content", &[], b"")?;

    let result = NetworkClient::new().fetch_document(&url);

    assert!(matches!(
        result,
        Err(DocumentLoadError::NoContent { status: 204 })
    ));

    Ok(())
}

#[test]
fn partial_content_is_not_used_as_main_document() -> Result<(), Box<dyn Error>> {
    let url = serve_once(
        "206 Partial Content",
        &[("Content-Type", "text/html")],
        b"<html></html>",
    )?;

    let result = NetworkClient::new().fetch_document(&url);

    assert!(matches!(result, Err(DocumentLoadError::PartialContent)));

    Ok(())
}

#[test]
fn configured_document_body_limit_remains_enforced() -> Result<(), Box<dyn Error>> {
    let url = serve_once(
        "200 OK",
        &[("Content-Type", "text/html")],
        b"<html>body exceeds tiny test budget</html>",
    )?;

    let result = NetworkClient::with_max_body_bytes(8).fetch_document(&url);

    assert!(result.is_err());

    Ok(())
}

fn serve_once(
    status: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<String, Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let status = status.to_owned();
    let headers = headers
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect::<Vec<_>>();
    let body = body.to_vec();

    let _server = thread::spawn(move || -> io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        let _request = read_request(&mut stream)?;

        write!(stream, "HTTP/1.1 {status}\r\n")?;

        for (name, value) in headers {
            write!(stream, "{name}: {value}\r\n")?;
        }

        write!(
            stream,
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )?;
        stream.write_all(&body)?;
        stream.flush()?;

        Ok(())
    });

    Ok(format!("http://{address}/document"))
}

fn read_request(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;

    let mut request = Vec::new();
    let mut buffer = [0_u8; 512];

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

    Ok(request)
}
