//! HTTP and HTTPS transport boundary for Phantom.
//!
//! This crate is intentionally limited to network transport responsibilities.
//! It does not parse HTML, execute scripts, perform layout, or render pages.
//!
//! Security-sensitive defaults are applied here so callers cannot accidentally
//! issue unbounded requests through the normal Phantom networking path.

#![forbid(unsafe_code)]

use std::time::Duration;

use thiserror::Error;
use ureq::http::header::CONTENT_TYPE;
use ureq::{Agent, ResponseExt};
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BODY_BYTES: u64 = 4 * 1024 * 1024;
const MAX_RESPONSE_HEADER_BYTES: usize = 64 * 1024;
const MAX_REDIRECTS: u32 = 10;

const USER_AGENT: &str = concat!("Phantom/", env!("CARGO_PKG_VERSION"));

const ACCEPT: &str = "text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.1";

const ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";

/// Errors that may occur while preparing or performing a Phantom network
/// request.
#[derive(Debug, Error)]
pub enum NetworkError {
    /// The user did not provide an address.
    #[error("the address is empty")]
    EmptyAddress,

    /// The address could not be parsed as a URL.
    #[error("invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    /// The URL uses a protocol that Phantom does not permit for web
    /// navigation.
    #[error("unsupported URL scheme: {scheme}")]
    UnsupportedScheme {
        /// Unsupported scheme supplied by the caller.
        scheme: String,
    },

    /// The URL does not identify a network host.
    #[error("the URL does not contain a host")]
    MissingHost,

    /// User information embedded directly in the URL is rejected.
    #[error("embedded credentials are not allowed in browser addresses")]
    EmbeddedCredentials,

    /// The HTTP or HTTPS request failed.
    #[error("network request failed: {0}")]
    Request(#[source] ureq::Error),

    /// The response body could not be read safely.
    #[error("response body could not be read: {0}")]
    BodyRead(#[source] ureq::Error),
}

/// Textual HTTP response returned by the Phantom transport layer.
#[derive(Debug)]
pub struct TextResponse {
    requested_url: String,
    final_url: String,
    status: u16,
    content_type: String,
    redirect_count: usize,
    body: String,
}

impl TextResponse {
    /// Returns the normalized URL originally requested by Phantom.
    #[must_use]
    pub fn requested_url(&self) -> &str {
        &self.requested_url
    }

    /// Returns the final URL after permitted HTTP redirects.
    #[must_use]
    pub fn final_url(&self) -> &str {
        &self.final_url
    }

    /// Returns the HTTP status code.
    #[must_use]
    pub const fn status(&self) -> u16 {
        self.status
    }

    /// Returns the response content type, when supplied by the server.
    #[must_use]
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    /// Returns the number of redirects followed for this navigation.
    #[must_use]
    pub const fn redirect_count(&self) -> usize {
        self.redirect_count
    }

    /// Returns the decoded textual response body.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Returns the UTF-8 byte length of the decoded response body.
    #[must_use]
    pub fn body_bytes(&self) -> usize {
        self.body.len()
    }
}

/// Reusable HTTP/HTTPS client used by Phantom.
///
/// The client applies bounded resource usage, a global request timeout,
/// restricted URL schemes, bounded redirects, and a bounded response body.
#[derive(Clone)]
pub struct NetworkClient {
    agent: Agent,
    max_body_bytes: u64,
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkClient {
    /// Creates a network client using Phantom's security-conscious defaults.
    #[must_use]
    pub fn new() -> Self {
        let config = Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .http_status_as_error(false)
            .max_redirects(MAX_REDIRECTS)
            .max_redirects_will_error(true)
            .save_redirect_history(true)
            .max_response_header_size(MAX_RESPONSE_HEADER_BYTES)
            .user_agent(USER_AGENT)
            .accept(ACCEPT)
            .build();

        let agent: Agent = config.into();

        Self {
            agent,
            max_body_bytes: MAX_BODY_BYTES,
        }
    }

    /// Fetches a textual HTTP or HTTPS resource.
    ///
    /// Addresses without an explicit scheme are interpreted as HTTPS.
    ///
    /// HTTP error pages such as `404` and `500` are returned as normal
    /// [`TextResponse`] values so the browser can render the server response.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] when the address is invalid, when the protocol
    /// is not HTTP/HTTPS, when credentials are embedded in the URL, when the
    /// request fails, or when the bounded response body cannot be read.
    pub fn fetch_text(&self, address: &str) -> Result<TextResponse, NetworkError> {
        let mut url = normalize_address(address)?;

        // URL fragments identify a location inside the resulting document and
        // are never transmitted as part of the HTTP request.
        url.set_fragment(None);

        let requested_url = url.to_string();

        let mut response = self
            .agent
            .get(url.as_str())
            .header("Accept-Language", ACCEPT_LANGUAGE)
            .call()
            .map_err(NetworkError::Request)?;

        let status = response.status().as_u16();

        let final_url = response.get_uri().to_string();

        let redirect_count = response
            .get_redirect_history()
            .map_or(0, |history| history.len().saturating_sub(1));

        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("unknown")
            .to_owned();

        let body = response
            .body_mut()
            .with_config()
            .limit(self.max_body_bytes)
            .read_to_string()
            .map_err(NetworkError::BodyRead)?;

        Ok(TextResponse {
            requested_url,
            final_url,
            status,
            content_type,
            redirect_count,
            body,
        })
    }
}

fn normalize_address(address: &str) -> Result<Url, NetworkError> {
    let trimmed = address.trim();

    if trimmed.is_empty() {
        return Err(NetworkError::EmptyAddress);
    }

    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };

    let url = Url::parse(&candidate)?;

    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(NetworkError::UnsupportedScheme {
                scheme: scheme.to_owned(),
            });
        }
    }

    if url.host_str().is_none() {
        return Err(NetworkError::MissingHost);
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(NetworkError::EmbeddedCredentials);
    }

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::{NetworkError, normalize_address};

    #[test]
    fn missing_scheme_defaults_to_https() -> Result<(), NetworkError> {
        let url = normalize_address("example.com")?;

        assert_eq!(url.as_str(), "https://example.com/");

        Ok(())
    }

    #[test]
    fn http_is_permitted() -> Result<(), NetworkError> {
        let url = normalize_address("http://example.com")?;

        assert_eq!(url.scheme(), "http");

        Ok(())
    }

    #[test]
    fn unsupported_protocol_is_rejected() {
        let result = normalize_address("file:///etc/passwd");

        assert!(matches!(
            result,
            Err(NetworkError::UnsupportedScheme { .. })
        ));
    }

    #[test]
    fn embedded_credentials_are_rejected() {
        let result = normalize_address("https://user:password@example.com");

        assert!(matches!(result, Err(NetworkError::EmbeddedCredentials)));
    }
}
