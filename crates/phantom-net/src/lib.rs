//! Bounded HTTP and HTTPS transport boundary for Phantom.
//!
//! The web engine owns navigation and Web Platform semantics. This crate owns
//! HTTP(S) URL validation, relative HTTP(S) resource resolution, transport
//! mechanics, and response-size policy.
//!
//! Text documents and binary subresources use separate byte budgets so image
//! loading cannot silently turn the navigation path into an unbounded download
//! API.

#![forbid(unsafe_code)]

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use thiserror::Error;
use ureq::ResponseExt;
use url::Url;

const DEFAULT_MAX_TEXT_BODY_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_MAX_BINARY_BODY_BYTES: u64 = 16 * 1024 * 1024;

/// HTTP(S) URL accepted by Phantom's network boundary.
///
/// This wrapper deliberately exposes only the operations required by the
/// transport/resource coordinator. The external `url` crate remains an
/// implementation detail of `phantom-net`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct HttpUrl(Url);

impl HttpUrl {
    /// Parses and validates an absolute browser HTTP(S) URL.
    ///
    /// If no scheme is present, Phantom assumes HTTPS for top-level navigation
    /// convenience. Embedded URL credentials are rejected.
    ///
    /// # Errors
    ///
    /// Returns [`UrlError`] for empty, malformed, unsupported, credentialed, or
    /// hostless URLs.
    pub fn parse(input: &str) -> Result<Self, UrlError> {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return Err(UrlError::Empty);
        }

        let normalized = if trimmed.contains("://") {
            trimmed.to_owned()
        } else {
            format!("https://{trimmed}")
        };

        let parsed =
            Url::parse(&normalized).map_err(|error| UrlError::Invalid(error.to_string()))?;

        Self::from_url(parsed)
    }

    /// Resolves a relative Web reference against this HTTP(S) URL.
    ///
    /// # Errors
    ///
    /// Returns [`UrlError`] if URL joining fails or the resulting target
    /// violates Phantom's HTTP(S) policy.
    pub fn resolve(&self, reference: &str) -> Result<Self, UrlError> {
        let joined = self
            .0
            .join(reference)
            .map_err(|error| UrlError::Invalid(error.to_string()))?;

        Self::from_url(joined)
    }

    /// Returns the serialized absolute URL.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the URL scheme.
    #[must_use]
    pub fn scheme(&self) -> &str {
        self.0.scheme()
    }

    /// Returns the host when present.
    #[must_use]
    pub fn host_str(&self) -> Option<&str> {
        self.0.host_str()
    }

    fn from_url(url: Url) -> Result<Self, UrlError> {
        match url.scheme() {
            "http" | "https" => {}
            other => {
                return Err(UrlError::UnsupportedScheme(other.to_owned()));
            }
        }

        if url.host_str().is_none() {
            return Err(UrlError::MissingHost);
        }

        if !url.username().is_empty() || url.password().is_some() {
            return Err(UrlError::CredentialsNotAllowed);
        }

        Ok(Self(url))
    }
}

impl fmt::Display for HttpUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for HttpUrl {
    type Err = UrlError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// URL validation errors emitted by the HTTP(S) boundary.
#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum UrlError {
    /// The supplied value was empty.
    #[error("URL is empty")]
    Empty,

    /// Parsing or relative resolution failed.
    #[error("invalid URL: {0}")]
    Invalid(String),

    /// The URL does not contain a host.
    #[error("URL is missing a host")]
    MissingHost,

    /// Phantom allows only HTTP and HTTPS at this network boundary.
    #[error("unsupported URL scheme: {0}")]
    UnsupportedScheme(String),

    /// Embedded username/password credentials are intentionally rejected.
    #[error("embedded URL credentials are not allowed")]
    CredentialsNotAllowed,
}

/// HTTP response decoded as text for the document pipeline.
#[derive(Clone, Debug)]
pub struct TextResponse {
    requested_url: String,
    final_url: String,
    status: u16,
    content_type: Option<String>,
    body: String,
}

impl TextResponse {
    /// URL supplied to the network boundary.
    #[must_use]
    pub fn requested_url(&self) -> &str {
        &self.requested_url
    }

    /// Final URL after HTTP redirects.
    #[must_use]
    pub fn final_url(&self) -> &str {
        &self.final_url
    }

    /// HTTP status code.
    #[must_use]
    pub const fn status(&self) -> u16 {
        self.status
    }

    /// Response content type when supplied by the server.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// Decoded response body.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// UTF-8 byte count held by the decoded body.
    #[must_use]
    pub fn body_bytes(&self) -> usize {
        self.body.len()
    }
}

/// Bounded binary HTTP response used for image and future subresources.
#[derive(Clone, Debug)]
pub struct BinaryResponse {
    requested_url: String,
    final_url: String,
    status: u16,
    content_type: Option<String>,
    body: Box<[u8]>,
}

impl BinaryResponse {
    /// URL supplied to the network boundary.
    #[must_use]
    pub fn requested_url(&self) -> &str {
        &self.requested_url
    }

    /// Final URL after HTTP redirects.
    #[must_use]
    pub fn final_url(&self) -> &str {
        &self.final_url
    }

    /// HTTP status code.
    #[must_use]
    pub const fn status(&self) -> u16 {
        self.status
    }

    /// Response content type when supplied by the server.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// Immutable response bytes.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Number of downloaded response bytes retained in memory.
    #[must_use]
    pub fn body_bytes(&self) -> usize {
        self.body.len()
    }
}

/// Reusable network client used by browser resource coordinators.
#[derive(Clone)]
pub struct NetworkClient {
    agent: ureq::Agent,
    max_text_body_bytes: u64,
    max_binary_body_bytes: u64,
}

impl Default for NetworkClient {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(10)
            .max_response_header_size(64 * 1024)
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent("Phantom/0.0.1 (+https://github.com/eusourmr/phantom)")
            .build();

        Self {
            agent: ureq::Agent::new_with_config(config),
            max_text_body_bytes: DEFAULT_MAX_TEXT_BODY_BYTES,
            max_binary_body_bytes: DEFAULT_MAX_BINARY_BODY_BYTES,
        }
    }
}

impl NetworkClient {
    /// Creates a client using Phantom's default bounded response policy.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a client with one custom text-response size limit.
    ///
    /// The binary-resource limit remains at the safe default.
    #[must_use]
    pub fn with_max_body_bytes(max_body_bytes: u64) -> Self {
        Self {
            max_text_body_bytes: max_body_bytes.max(1),
            ..Self::default()
        }
    }

    /// Builds a client with explicit document and binary-resource limits.
    #[must_use]
    pub fn with_body_limits(max_text_body_bytes: u64, max_binary_body_bytes: u64) -> Self {
        Self {
            max_text_body_bytes: max_text_body_bytes.max(1),
            max_binary_body_bytes: max_binary_body_bytes.max(1),
            ..Self::default()
        }
    }

    /// Fetches an HTTP(S) resource and decodes it as text.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for rejected URLs, transport failures, invalid
    /// redirect targets, or responses exceeding the configured text budget.
    pub fn fetch_text(&self, input: &str) -> Result<TextResponse, NetworkError> {
        let url = HttpUrl::parse(input)?;
        let requested_url = url.as_str().to_owned();

        let mut response = self
            .agent
            .get(url.as_str())
            .header(
                "Accept",
                "text/html,application/xhtml+xml,text/css,text/plain;q=0.9,*/*;q=0.5",
            )
            .call()
            .map_err(|error| NetworkError::Request(error.to_string()))?;

        let status = response.status().as_u16();

        let final_url = HttpUrl::parse(&response.get_uri().to_string())?;

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        let body = response
            .body_mut()
            .with_config()
            .limit(self.max_text_body_bytes)
            .lossy_utf8(true)
            .read_to_string()
            .map_err(|error| NetworkError::Body(error.to_string()))?;

        Ok(TextResponse {
            requested_url,
            final_url: final_url.as_str().to_owned(),
            status,
            content_type,
            body,
        })
    }

    /// Fetches a bounded binary HTTP(S) subresource.
    ///
    /// The request advertises the image formats enabled by the current Phantom
    /// decoder while retaining a conservative generic image fallback.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for transport failures, invalid redirect
    /// targets, or a response exceeding the configured binary byte budget.
    pub fn fetch_bytes(&self, url: &HttpUrl) -> Result<BinaryResponse, NetworkError> {
        let requested_url = url.as_str().to_owned();

        let mut response = self
            .agent
            .get(url.as_str())
            .header(
                "Accept",
                "image/webp,image/png,image/jpeg,image/gif;q=0.95,image/*;q=0.5,*/*;q=0.1",
            )
            .call()
            .map_err(|error| NetworkError::Request(error.to_string()))?;

        let status = response.status().as_u16();

        let final_url = HttpUrl::parse(&response.get_uri().to_string())?;

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        let body = response
            .body_mut()
            .with_config()
            .limit(self.max_binary_body_bytes)
            .read_to_vec()
            .map_err(|error| NetworkError::Body(error.to_string()))?
            .into_boxed_slice();

        Ok(BinaryResponse {
            requested_url,
            final_url: final_url.as_str().to_owned(),
            status,
            content_type,
            body,
        })
    }
}

/// Errors emitted by Phantom's network boundary.
#[derive(Debug, Error)]
pub enum NetworkError {
    /// The HTTP transport rejected or failed the request.
    #[error("network request failed: {0}")]
    Request(String),

    /// The response body could not be read within configured limits.
    #[error("response body failed: {0}")]
    Body(String),

    /// The supplied or redirected URL violates Phantom HTTP(S) policy.
    #[error(transparent)]
    Url(#[from] UrlError),
}

#[cfg(test)]
mod tests {
    use super::{HttpUrl, NetworkClient, UrlError};

    #[test]
    fn assumes_https_when_scheme_is_missing() -> Result<(), UrlError> {
        let url = HttpUrl::parse("example.com/a")?;

        assert_eq!(url.as_str(), "https://example.com/a");

        Ok(())
    }

    #[test]
    fn resolves_relative_resources() -> Result<(), UrlError> {
        let base = HttpUrl::parse("https://example.com/news/page.html")?;

        let image = base.resolve("../img/photo.jpg")?;

        assert_eq!(image.as_str(), "https://example.com/img/photo.jpg");

        Ok(())
    }

    #[test]
    fn rejects_embedded_credentials() {
        let result = HttpUrl::parse("https://user:secret@example.com/");

        assert_eq!(result, Err(UrlError::CredentialsNotAllowed));
    }

    #[test]
    fn accepts_a_custom_text_body_limit() {
        let client = NetworkClient::with_max_body_bytes(1024);

        assert_eq!(client.max_text_body_bytes, 1024);
    }

    #[test]
    fn accepts_separate_binary_budget() {
        let client = NetworkClient::with_body_limits(1024, 4096);

        assert_eq!(client.max_text_body_bytes, 1024);

        assert_eq!(client.max_binary_body_bytes, 4096);
    }
}
