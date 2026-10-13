//! Bounded HTTP and HTTPS transport boundary for Phantom.
//!
//! The web engine owns navigation and Web Platform semantics. This crate owns
//! HTTP(S) URL validation, relative HTTP(S) resource resolution, transport
//! mechanics, bounded response policy, and partitioned in-memory HTTP caches
//! used by documents and binary subresources.
//!
//! Text documents and binary subresources use separate byte budgets so image
//! loading cannot silently turn the navigation path into an unbounded download
//! API.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use thiserror::Error;
use ureq::ResponseExt;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};
use url::Url;

const DEFAULT_MAX_TEXT_BODY_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_MAX_BINARY_BODY_BYTES: u64 = 16 * 1024 * 1024;
const DEFAULT_MAX_TEXT_CACHE_BYTES: u64 = 16 * 1024 * 1024;
const DEFAULT_MAX_TEXT_CACHE_ENTRIES: usize = 32;
const DEFAULT_MAX_BINARY_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_BINARY_CACHE_ENTRIES: usize = 128;
const MAX_BINARY_FETCH_ATTEMPTS: usize = 2;
const MAX_DOCUMENT_REDIRECTS: usize = 10;
const MAX_CORS_SAFELISTED_RESPONSE_HEADER_VALUE_BYTES: usize = 8 * 1024;
const MAX_CORS_SAFELISTED_REQUEST_HEADER_VALUE_BYTES: usize = 128;
const MAX_CORS_SAFELISTED_REQUEST_HEADER_TOTAL_BYTES: usize = 1024;
const DEFAULT_MAX_CORS_PREFLIGHT_CACHE_ENTRIES: usize = 64;
const DEFAULT_CORS_PREFLIGHT_MAX_AGE_SECS: u64 = 5;
const MAX_CORS_PREFLIGHT_MAX_AGE_SECS: u64 = 600;
const MAX_CORS_PREFLIGHT_RESPONSE_HEADER_BYTES: usize = 16 * 1024;
const DEFAULT_MAX_COOKIE_STORE_ENTRIES: usize = 128;
const MAX_COOKIE_PAIR_BYTES: usize = 4096;
const MAX_SET_COOKIE_BYTES: usize = 8192;
const MAX_COOKIE_HEADER_BYTES: usize = 8192;
const MAX_SET_COOKIE_FIELDS_PER_RESPONSE: usize = 16;
const MAX_SET_COOKIE_RESPONSE_BYTES: usize = 32 * 1024;
const MAX_CORS_REQUEST_BODY_BYTES: usize = 1024 * 1024;
const MAX_COOKIE_AGE_SECS: u64 = 400 * 24 * 60 * 60;
const IMAGE_ACCEPT: &str =
    "image/webp,image/png,image/jpeg,image/gif;q=0.95,image/*;q=0.5,*/*;q=0.1";

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

    // PHANTOM_2C13_FORM_QUERY_URL
    // PHANTOM_2C14_FRAGMENT_URL
    /// Returns the serialized URL fragment without the leading `#`.
    #[must_use]
    pub fn fragment(&self) -> Option<&str> {
        self.0.fragment()
    }

    /// Returns this URL with the supplied fragment.
    #[must_use]
    pub fn with_fragment(&self, fragment: Option<&str>) -> Self {
        let mut url = self.0.clone();
        url.set_fragment(fragment);
        Self(url)
    }

    /// Returns this URL without a fragment.
    #[must_use]
    pub fn without_fragment(&self) -> Self {
        self.with_fragment(None)
    }

    /// Returns whether two HTTP(S) URLs identify the same network document
    /// when only their fragments are ignored.
    #[must_use]
    pub fn same_document_except_fragment(&self, other: &Self) -> bool {
        self.without_fragment() == other.without_fragment()
    }
    /// Returns this HTTP(S) URL with its query replaced by form-style pairs.
    ///
    /// Encoding is delegated to `url::Url` rather than implemented manually.
    /// The URL fragment, if any, is preserved.
    #[must_use]
    pub fn with_query_pairs(&self, pairs: &[(String, String)]) -> Self {
        let mut url = self.0.clone();
        url.set_query(None);

        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(
                pairs
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str())),
            );
        }

        Self(url)
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

    // PHANTOM_2D2_TYPED_ORIGIN
    /// Returns the canonical tuple origin represented by this HTTP(S) URL.
    #[must_use]
    pub fn origin(&self) -> Origin {
        Origin::from_http_url(self)
    }

    /// Returns whether this URL and another HTTP(S) URL share one origin.
    #[must_use]
    pub fn same_origin(&self, other: &Self) -> bool {
        self.origin().same_origin(&other.origin())
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

// PHANTOM_2D2_ORIGIN_DOMAIN
/// Scheme component of one HTTP(S) tuple origin.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OriginScheme {
    /// Unencrypted HTTP transport.
    Http,

    /// TLS-protected HTTPS transport.
    Https,
}

impl OriginScheme {
    /// Returns the serialized scheme without `://`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }

    /// Returns the default/effective port for the scheme.
    #[must_use]
    pub const fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }

    /// Returns whether this scheme provides encrypted transport.
    #[must_use]
    pub const fn is_secure_transport(self) -> bool {
        matches!(self, Self::Https)
    }
}

/// Canonical HTTP(S) tuple origin used by Phantom security/network policy.
///
/// Equality is based on normalized scheme, host and effective port. The
/// serialized representation delegates canonicalization to `url::Url`, so
/// default ports are omitted and host normalization is not reimplemented by
/// Phantom.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Origin {
    scheme: OriginScheme,
    host: String,
    effective_port: u16,
    serialized: String,
}

impl Origin {
    fn from_http_url(url: &HttpUrl) -> Self {
        let scheme = if url.scheme() == "https" {
            OriginScheme::Https
        } else {
            OriginScheme::Http
        };
        let host = url.host_str().unwrap_or_default().to_owned();
        let effective_port = url
            .0
            .port_or_known_default()
            .unwrap_or_else(|| scheme.default_port());
        let serialized = url.0.origin().ascii_serialization();

        Self {
            scheme,
            host,
            effective_port,
            serialized,
        }
    }

    /// Returns the normalized HTTP(S) scheme.
    #[must_use]
    pub const fn scheme(&self) -> OriginScheme {
        self.scheme
    }

    /// Returns the canonical ASCII host.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the effective port, including the scheme default when omitted.
    #[must_use]
    pub const fn effective_port(&self) -> u16 {
        self.effective_port
    }

    /// Returns the canonical serialized origin.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// Returns whether two typed origins are identical under same-origin rules.
    #[must_use]
    pub fn same_origin(&self, other: &Self) -> bool {
        self == other
    }

    /// Returns whether the origin uses encrypted HTTPS transport.
    #[must_use]
    pub const fn is_secure_transport(&self) -> bool {
        self.scheme.is_secure_transport()
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Privacy boundary used to partition reusable network state.
///
/// Phantom currently keys the boundary by schemeful origins rather than by
/// registrable sites because the engine does not yet carry a Public Suffix List
/// dependency. This is intentionally conservative: it may reduce cache sharing
/// between same-site subdomains, but it does not merge unrelated origins.
///
/// The two dimensions reserve the browser architecture for nested browsing
/// contexts: `top_level_origin` identifies the user-visible document and
/// `frame_origin` identifies the document that initiated the subresource load.
/// For the current top-level image pipeline both values are identical.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NetworkIsolationKey {
    top_level_origin: Origin,
    frame_origin: Origin,
}

impl NetworkIsolationKey {
    /// Creates a double-keyed network isolation boundary from the top-level and
    /// requesting frame URLs.
    #[must_use]
    pub fn new(top_level_url: &HttpUrl, frame_url: &HttpUrl) -> Self {
        Self {
            top_level_origin: top_level_url.origin(),
            frame_origin: frame_url.origin(),
        }
    }

    /// Creates the isolation key used by a top-level document and its direct
    /// subresources.
    #[must_use]
    pub fn from_top_level(top_level_url: &HttpUrl) -> Self {
        Self::new(top_level_url, top_level_url)
    }

    /// Canonical serialized origin of the top-level document.
    ///
    /// This string accessor remains for compatibility. New security policy
    /// should prefer [`Self::top_level_origin_value`].
    #[must_use]
    pub fn top_level_origin(&self) -> &str {
        self.top_level_origin.as_str()
    }

    /// Typed origin of the top-level document.
    #[must_use]
    pub const fn top_level_origin_value(&self) -> &Origin {
        &self.top_level_origin
    }

    /// Canonical serialized origin of the requesting frame/document.
    ///
    /// This string accessor remains for compatibility. New security policy
    /// should prefer [`Self::frame_origin_value`].
    #[must_use]
    pub fn frame_origin(&self) -> &str {
        self.frame_origin.as_str()
    }

    /// Typed origin of the requesting frame/document.
    #[must_use]
    pub const fn frame_origin_value(&self) -> &Origin {
        &self.frame_origin
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

// PHANTOM_2C11_DOCUMENT_CACHE
/// Fetch behavior requested by the top-level document navigator.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DocumentRequestMode {
    /// Normal navigation may reuse a fresh cached document representation.
    #[default]
    Navigate,

    /// Reload bypasses freshness reuse and asks the origin to revalidate.
    Reload,
}

// PHANTOM_2N7_CORS_TRANSPORT_I
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextCrossOriginMode {
    Block,
    ClassicScript,
    CorsSimple,
}

// PHANTOM_2N15_NON_SIMPLE_METHOD_BODY_TRANSPORT_I
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextRequestMethod {
    Get,
    Post,
    Put,
    Patch,
}

impl TextRequestMethod {
    fn carries_body(self) -> bool {
        self != Self::Get
    }
}

/// Non-GET CORS method supported by Phantom's bounded body transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorsBodyMethod {
    /// HTTP POST.
    Post,
    /// HTTP PUT.
    Put,
    /// HTTP PATCH.
    Patch,
}

impl CorsBodyMethod {
    /// Returns the HTTP token used for preflight and transport.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
        }
    }

    fn text_request_method(self) -> TextRequestMethod {
        match self {
            Self::Post => TextRequestMethod::Post,
            Self::Put => TextRequestMethod::Put,
            Self::Patch => TextRequestMethod::Patch,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TextRequestPolicy<'a> {
    cross_origin_mode: TextCrossOriginMode,
    author_headers: &'a [(&'a str, &'a str)],
    cache_enabled: bool,
    credentials_mode: CredentialsMode,
    cookie_site_context: CookieSiteContext,
    request_method: TextRequestMethod,
    request_body: Option<&'a [u8]>,
}

// PHANTOM_2N10_SIMPLE_REQUEST_MATRIX_I
/// CORS request classification used by Phantom before transport.
///
/// `Simple` can proceed without a preflight under the current CORS request
/// safelist. `PreflightRequired` is structurally valid but needs the OPTIONS
/// path that remains deferred to 2N-11. `Forbidden` is rejected outright.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorsRequestClassification {
    /// Request is within the CORS method/header safelist.
    Simple,
    /// Request is valid but requires a CORS preflight before transport.
    PreflightRequired,
    /// Request uses a forbidden method or author-controlled header.
    Forbidden,
}

// PHANTOM_2N13_CREDENTIALS_COOKIE_FOUNDATION_I
/// Fetch credentials mode modeled at the network boundary.
///
/// Transport attachment of cookies remains disabled in 2N-13. This type
/// defines when a future request may include credentials and when a response
/// may mutate credential state.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CredentialsMode {
    /// Never send credentials and ignore credential-bearing response state.
    Omit,
    /// Use credentials only for same-origin requests/responses.
    SameOrigin,
    /// Permit credentials for both same-origin and cross-origin requests.
    Include,
}

impl CredentialsMode {
    /// Returns whether this mode permits request credentials for `request_url`.
    #[must_use]
    pub fn includes_request_credentials(
        self,
        document_url: &HttpUrl,
        request_url: &HttpUrl,
    ) -> bool {
        match self {
            Self::Omit => false,
            Self::SameOrigin => request_url.same_origin(document_url),
            Self::Include => true,
        }
    }

    /// Returns whether response credential state may be accepted.
    #[must_use]
    pub fn accepts_response_credentials(
        self,
        document_url: &HttpUrl,
        response_url: &HttpUrl,
    ) -> bool {
        self.includes_request_credentials(document_url, response_url)
    }
}

/// Same-site classification supplied by the Browser Host for cookie selection.
///
/// Full registrable-domain/site computation is intentionally not introduced
/// in 2N-13. The cookie store receives this explicit classification so that
/// SameSite enforcement is not guessed from tuple-origin equality.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CookieSiteContext {
    /// Request is same-site for cookie purposes.
    SameSite,
    /// Request is cross-site for cookie purposes.
    CrossSite,
}

/// SameSite value retained by Phantom's cookie store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CookieSameSite {
    /// Attribute omitted or unrecognized; treated conservatively like Lax for
    /// subresource selection in this foundation.
    Default,
    /// SameSite=Strict.
    Strict,
    /// SameSite=Lax.
    Lax,
    /// SameSite=None.
    None,
}

/// Result of processing one Set-Cookie field into Phantom's bounded store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CookieStoreOutcome {
    /// A new cookie was stored.
    Stored,
    /// An existing cookie with the same key was replaced.
    Replaced,
    /// Max-Age removed an existing cookie, or requested deletion found none.
    Deleted,
    /// Cookie was intentionally ignored by credentials/security policy.
    Ignored,
}

/// Classifies one author-controlled CORS request using a fixed, bounded matrix.
///
/// This function does not send network traffic and does not enable preflight.
/// It exists so Browser Host can distinguish requests that are currently safe
/// to send from those that must wait for the preflight implementation.
///
/// Header values are checked against Fetch's per-value 128-byte safelist limit
/// and the aggregate 1024-byte safelist limit.
#[must_use]
pub fn classify_cors_request(
    method: &str,
    author_headers: &[(&str, &str)],
) -> CorsRequestClassification {
    if cors_method_is_forbidden(method) {
        return CorsRequestClassification::Forbidden;
    }

    if !cors_method_is_safelisted(method) {
        return CorsRequestClassification::PreflightRequired;
    }

    let mut safelisted_total = 0_usize;
    for (name, value) in author_headers {
        if cors_author_header_is_forbidden(name) {
            return CorsRequestClassification::Forbidden;
        }
        if !cors_request_header_is_safelisted(name, value) {
            return CorsRequestClassification::PreflightRequired;
        }
        safelisted_total = safelisted_total.saturating_add(value.len());
    }

    if safelisted_total > MAX_CORS_SAFELISTED_REQUEST_HEADER_TOTAL_BYTES {
        CorsRequestClassification::PreflightRequired
    } else {
        CorsRequestClassification::Simple
    }
}

fn cors_method_is_safelisted(method: &str) -> bool {
    method.eq_ignore_ascii_case("GET")
        || method.eq_ignore_ascii_case("HEAD")
        || method.eq_ignore_ascii_case("POST")
}

fn cors_method_is_forbidden(method: &str) -> bool {
    method.eq_ignore_ascii_case("CONNECT")
        || method.eq_ignore_ascii_case("TRACE")
        || method.eq_ignore_ascii_case("TRACK")
}

fn cors_author_header_is_forbidden(name: &str) -> bool {
    const FORBIDDEN: [&str; 21] = [
        "accept-charset",
        "accept-encoding",
        "access-control-request-headers",
        "access-control-request-method",
        "connection",
        "content-length",
        "cookie",
        "cookie2",
        "date",
        "dnt",
        "expect",
        "host",
        "keep-alive",
        "origin",
        "referer",
        "set-cookie",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
        "via",
    ];

    FORBIDDEN
        .into_iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
        || name
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("proxy-"))
        || name
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("sec-"))
}

fn cors_request_header_is_safelisted(name: &str, value: &str) -> bool {
    if value.len() > MAX_CORS_SAFELISTED_REQUEST_HEADER_VALUE_BYTES {
        return false;
    }

    if name.eq_ignore_ascii_case("accept") {
        return !value.bytes().any(cors_unsafe_request_header_byte);
    }

    if name.eq_ignore_ascii_case("accept-language")
        || name.eq_ignore_ascii_case("content-language")
    {
        return value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b' ' | b'*' | b',' | b'-' | b'.' | b';' | b'=')
        });
    }

    if name.eq_ignore_ascii_case("content-type") {
        if value.bytes().any(cors_unsafe_request_header_byte) {
            return false;
        }
        let essence = value.split(';').next().unwrap_or_default().trim();
        return essence.eq_ignore_ascii_case("application/x-www-form-urlencoded")
            || essence.eq_ignore_ascii_case("multipart/form-data")
            || essence.eq_ignore_ascii_case("text/plain");
    }

    if name.eq_ignore_ascii_case("range") {
        return cors_range_header_is_safelisted(value);
    }

    false
}

fn cors_unsafe_request_header_byte(byte: u8) -> bool {
    (byte < 0x20 && byte != b'\t')
        || matches!(
            byte,
            b'"' | b'(' | b')' | b':' | b'<' | b'>' | b'?' | b'@' | b'[' | b'\\'
                | b']' | b'{' | b'}' | 0x7f
        )
}

fn cors_range_header_is_safelisted(value: &str) -> bool {
    let Some(prefix) = value.get(..5) else {
        return false;
    };
    if !prefix.eq_ignore_ascii_case("bytes") || value.as_bytes().get(5) != Some(&b'=') {
        return false;
    }

    let range = &value[6..];
    let Some((start, end)) = range.split_once('-') else {
        return false;
    };
    if start.is_empty() || start.bytes().any(|byte| !byte.is_ascii_digit()) {
        return false;
    }
    if end.bytes().any(|byte| !byte.is_ascii_digit()) {
        return false;
    }

    let Ok(start_value) = start.parse::<u64>() else {
        return false;
    };
    if end.is_empty() {
        return true;
    }
    let Ok(end_value) = end.parse::<u64>() else {
        return false;
    };
    start_value <= end_value
}

// PHANTOM_2N12_PREFLIGHT_METHODS_HEADERS_ADMISSION_II
fn cors_author_headers_transport_safe(author_headers: &[(&str, &str)]) -> bool {
    author_headers.iter().all(|(name, value)| {
        !name.is_empty()
            && name.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-'
                            | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
                    )
            })
            && !value.bytes().any(|byte| matches!(byte, b'\r' | b'\n'))
    })
}

// PHANTOM_2N11_CORS_PREFLIGHT_FOUNDATION_I
/// Result of Phantom's bounded CORS-preflight admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorsPreflightOutcome {
    /// Request is already inside the simple CORS request matrix.
    NotRequired,
    /// OPTIONS preflight was sent and admitted.
    Performed,
    /// A still-valid partitioned preflight cache entry admitted the request.
    Cached,
}

fn cors_unsafe_request_header_names(author_headers: &[(&str, &str)]) -> Vec<String> {
    let mut names = author_headers
        .iter()
        .filter(|(name, value)| !cors_request_header_is_safelisted(name, value))
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}


// PHANTOM_2N19_FINAL_ORIGIN_CORS_HARDENING
/// Whether the final response origin requires explicit CORS admission.
/// Checking the initial request alone is insufficient after redirects.
fn cors_final_origin_needs_admission(document: &HttpUrl, final_url: &HttpUrl) -> bool {
    !final_url.same_origin(document)
}

#[cfg(test)]
fn comma_list_contains_token(value: Option<&str>, token: &str) -> bool {
    comma_list_contains_token_for_credentials(value, token, CredentialsMode::Omit)
}

fn comma_list_contains_token_for_credentials(
    value: Option<&str>,
    token: &str,
    credentials_mode: CredentialsMode,
) -> bool {
    value.is_some_and(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .any(|item| {
                item.eq_ignore_ascii_case(token)
                    || (item == "*" && credentials_mode != CredentialsMode::Include)
            })
    })
}

#[cfg(test)]
fn cors_preflight_allows_headers(value: Option<&str>, required: &[String]) -> bool {
    cors_preflight_allows_headers_for_credentials(value, required, CredentialsMode::Omit)
}

fn cors_preflight_allows_headers_for_credentials(
    value: Option<&str>,
    required: &[String],
    credentials_mode: CredentialsMode,
) -> bool {
    if required.is_empty() {
        return true;
    }
    let Some(value) = value else {
        return false;
    };
    let allowed = value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<BTreeSet<_>>();

    required.iter().all(|header| {
        allowed.contains(header)
            || (credentials_mode != CredentialsMode::Include
                && allowed.contains("*")
                && !header.eq_ignore_ascii_case("authorization"))
    })
}


#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CookieStoreKey {
    isolation_key: NetworkIsolationKey,
    host: String,
    path: String,
    name: String,
}

#[derive(Clone, Debug)]
struct StoredCookie {
    value: String,
    secure: bool,
    _http_only: bool,
    same_site: CookieSameSite,
    expires_at: Option<Instant>,
    creation_order: u64,
    last_used: u64,
}

#[derive(Debug)]
struct CookieStore {
    entries: BTreeMap<CookieStoreKey, StoredCookie>,
    clock: u64,
    max_entries: usize,
}

impl CookieStore {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            clock: 0,
            max_entries: max_entries.max(1),
        }
    }

    fn retain_unexpired(&mut self, now: Instant) {
        self.entries
            .retain(|_, cookie| cookie.expires_at.is_none_or(|expires_at| expires_at > now));
    }

    fn insert(
        &mut self,
        key: CookieStoreKey,
        mut cookie: StoredCookie,
        now: Instant,
    ) -> CookieStoreOutcome {
        self.retain_unexpired(now);
        self.clock = self.clock.saturating_add(1);

        if let Some(existing) = self.entries.get(&key) {
            cookie.creation_order = existing.creation_order;
        } else if self.entries.len() >= self.max_entries
            && let Some(victim) = self
                .entries
                .iter()
                .min_by_key(|(_, stored)| stored.last_used)
                .map(|(key, _)| key.clone())
        {
            self.entries.remove(&victim);
        }

        cookie.last_used = self.clock;
        let replaced = self.entries.insert(key, cookie).is_some();
        if replaced {
            CookieStoreOutcome::Replaced
        } else {
            CookieStoreOutcome::Stored
        }
    }

    fn delete(&mut self, key: &CookieStoreKey) -> CookieStoreOutcome {
        self.entries.remove(key);
        CookieStoreOutcome::Deleted
    }

    fn cookie_header(
        &mut self,
        isolation_key: &NetworkIsolationKey,
        request_url: &HttpUrl,
        site_context: CookieSiteContext,
        now: Instant,
    ) -> Result<Option<String>, NetworkError> {
        self.retain_unexpired(now);
        let Some(host) = request_url.host_str() else {
            return Ok(None);
        };
        let request_path = request_url.0.path();
        let secure_request = request_url.scheme() == "https";

        let mut selected = self
            .entries
            .iter()
            .filter(|(key, cookie)| {
                key.isolation_key == *isolation_key
                    && key.host.eq_ignore_ascii_case(host)
                    && cookie_path_matches(&key.path, request_path)
                    && (!cookie.secure || secure_request)
                    && cookie_same_site_allows(cookie.same_site, site_context)
            })
            .map(|(key, cookie)| {
                (
                    key.clone(),
                    cookie.creation_order,
                    format!("{}={}", key.name, cookie.value),
                )
            })
            .collect::<Vec<_>>();

        selected.sort_by(|(left_key, left_order, _), (right_key, right_order, _)| {
            right_key
                .path
                .len()
                .cmp(&left_key.path.len())
                .then(left_order.cmp(right_order))
        });

        if selected.is_empty() {
            return Ok(None);
        }

        self.clock = self.clock.saturating_add(1);
        for (key, _, _) in &selected {
            if let Some(cookie) = self.entries.get_mut(key) {
                cookie.last_used = self.clock;
            }
        }

        let header = selected
            .into_iter()
            .map(|(_, _, pair)| pair)
            .collect::<Vec<_>>()
            .join("; ");
        if header.len() > MAX_COOKIE_HEADER_BYTES {
            return Err(NetworkError::Request(
                "Cookie header exceeds configured bounded size".to_owned(),
            ));
        }
        Ok(Some(header))
    }
}

#[derive(Debug)]
struct ParsedCookie {
    name: String,
    value: String,
    path: String,
    secure: bool,
    http_only: bool,
    same_site: CookieSameSite,
    max_age: Option<i64>,
}

fn cookie_name_is_valid(name: &str) -> bool {
    name.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-'
                    | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
            )
    })
}

fn cookie_value_is_valid(value: &str) -> bool {
    value.bytes().all(|byte| {
        matches!(byte, 0x21 | 0x23..=0x2b | 0x2d..=0x3a | 0x3c..=0x5b | 0x5d..=0x7e)
    })
}

fn cookie_default_path(url: &HttpUrl) -> String {
    let path = url.0.path();
    if !path.starts_with('/') || path == "/" {
        return "/".to_owned();
    }
    let Some(index) = path.rfind('/') else {
        return "/".to_owned();
    };
    if index == 0 {
        "/".to_owned()
    } else {
        path[..index].to_owned()
    }
}

fn cookie_path_matches(cookie_path: &str, request_path: &str) -> bool {
    request_path == cookie_path
        || (request_path.starts_with(cookie_path)
            && (cookie_path.ends_with('/')
                || request_path
                    .as_bytes()
                    .get(cookie_path.len())
                    .is_some_and(|byte| *byte == b'/')))
}

fn cookie_same_site_allows(
    same_site: CookieSameSite,
    context: CookieSiteContext,
) -> bool {
    match same_site {
        CookieSameSite::None => true,
        CookieSameSite::Default | CookieSameSite::Lax | CookieSameSite::Strict => {
            context == CookieSiteContext::SameSite
        }
    }
}

fn parse_set_cookie(
    set_cookie: &str,
    response_url: &HttpUrl,
) -> Option<ParsedCookie> {
    if set_cookie.is_empty() || set_cookie.len() > MAX_SET_COOKIE_BYTES {
        return None;
    }

    let mut parts = set_cookie.split(';');
    let first = parts.next()?.trim();
    let (name, raw_value) = first.split_once('=')?;
    let name = name.trim();
    let mut value = raw_value.trim();

    if name.is_empty() || !cookie_name_is_valid(name) {
        return None;
    }
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        value = &value[1..value.len() - 1];
    }
    if !cookie_value_is_valid(value)
        || name.len().saturating_add(value.len()) > MAX_COOKIE_PAIR_BYTES
    {
        return None;
    }

    let mut path = cookie_default_path(response_url);
    let mut secure = false;
    let mut http_only = false;
    let mut same_site = CookieSameSite::Default;
    let mut max_age = None;
    let mut domain_present = false;

    for part in parts {
        let attribute = part.trim();
        if attribute.is_empty() {
            continue;
        }
        let (attribute_name, attribute_value) = attribute
            .split_once('=')
            .map_or((attribute, ""), |(name, value)| (name.trim(), value.trim()));

        if attribute_name.eq_ignore_ascii_case("secure") {
            secure = true;
        } else if attribute_name.eq_ignore_ascii_case("httponly") {
            http_only = true;
        } else if attribute_name.eq_ignore_ascii_case("domain") {
            domain_present = true;
        } else if attribute_name.eq_ignore_ascii_case("path") {
            if attribute_value.starts_with('/') {
                path = attribute_value.to_owned();
            }
        } else if attribute_name.eq_ignore_ascii_case("samesite") {
            same_site = if attribute_value.eq_ignore_ascii_case("strict") {
                CookieSameSite::Strict
            } else if attribute_value.eq_ignore_ascii_case("lax") {
                CookieSameSite::Lax
            } else if attribute_value.eq_ignore_ascii_case("none") {
                CookieSameSite::None
            } else {
                CookieSameSite::Default
            };
        } else if attribute_name.eq_ignore_ascii_case("max-age")
            && let Ok(seconds) = attribute_value.parse::<i64>()
        {
            max_age = Some(seconds);
        }
    }

    if domain_present {
        return None;
    }
    if secure && response_url.scheme() != "https" {
        return None;
    }
    if same_site == CookieSameSite::None && !secure {
        return None;
    }
    if name.starts_with("__Secure-") && (!secure || response_url.scheme() != "https") {
        return None;
    }
    if name.starts_with("__Host-")
        && (!secure || response_url.scheme() != "https" || path != "/")
    {
        return None;
    }

    Some(ParsedCookie {
        name: name.to_owned(),
        value: value.to_owned(),
        path,
        secure,
        http_only,
        same_site,
        max_age,
    })
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CorsPreflightCacheKey {
    isolation_key: NetworkIsolationKey,
    request_origin: Origin,
    resource_url: String,
    method: String,
    unsafe_header_names: Vec<String>,
    credentials_mode: CredentialsMode,
}

#[derive(Clone, Debug)]
struct CorsPreflightCacheEntry {
    expires_at: Instant,
    last_used: u64,
}

#[derive(Debug)]
struct CorsPreflightCache {
    entries: BTreeMap<CorsPreflightCacheKey, CorsPreflightCacheEntry>,
    clock: u64,
    max_entries: usize,
}

impl CorsPreflightCache {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            clock: 0,
            max_entries: max_entries.max(1),
        }
    }

    fn admits(&mut self, key: &CorsPreflightCacheKey, now: Instant) -> bool {
        self.entries.retain(|_, entry| entry.expires_at > now);
        self.clock = self.clock.saturating_add(1);
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        entry.last_used = self.clock;
        true
    }

    fn insert(&mut self, key: CorsPreflightCacheKey, expires_at: Instant) {
        self.entries.retain(|_, entry| entry.expires_at > Instant::now());
        self.clock = self.clock.saturating_add(1);

        if !self.entries.contains_key(&key)
            && self.entries.len() >= self.max_entries
            && let Some(victim) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
        {
            self.entries.remove(&victim);
        }

        self.entries.insert(
            key,
            CorsPreflightCacheEntry {
                expires_at,
                last_used: self.clock,
            },
        );
    }

    fn clear_for(
        &mut self,
        isolation_key: &NetworkIsolationKey,
        request_origin: &Origin,
        resource_url: &str,
    ) {
        self.entries.retain(|key, _| {
            key.isolation_key != *isolation_key
                || key.request_origin != *request_origin
                || key.resource_url != resource_url
        });
    }
}

// PHANTOM_2D1_DOCUMENT_LOADER_HARDENING
/// Top-level document media type admitted by Phantom's first hardened loader.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentMediaType {
    /// HTML served as `text/html`.
    Html,

    /// XHTML served as `application/xhtml+xml`.
    Xhtml,
}

/// A top-level response that passed Phantom's bounded document-admission policy.
#[derive(Clone, Debug)]
pub struct DocumentResponse {
    response: TextResponse,
    requested_url: HttpUrl,
    final_url: HttpUrl,
    media_type: DocumentMediaType,
}

impl DocumentResponse {
    /// URL supplied to the document loader.
    #[must_use]
    pub fn requested_url(&self) -> &str {
        self.requested_url.as_str()
    }

    /// Typed URL supplied to the document loader.
    #[must_use]
    pub const fn requested_http_url(&self) -> &HttpUrl {
        &self.requested_url
    }

    /// Final URL after HTTP redirects.
    #[must_use]
    pub fn final_url(&self) -> &str {
        self.final_url.as_str()
    }

    /// Typed final document URL after HTTP redirects.
    #[must_use]
    pub const fn final_http_url(&self) -> &HttpUrl {
        &self.final_url
    }

    /// Canonical final document origin.
    #[must_use]
    pub fn origin(&self) -> Origin {
        self.final_url.origin()
    }

    /// Final HTTP status code.
    #[must_use]
    pub const fn status(&self) -> u16 {
        self.response.status()
    }

    /// Original response `Content-Type`, when supplied.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.response.content_type()
    }

    /// Admitted document media type.
    #[must_use]
    pub const fn media_type(&self) -> DocumentMediaType {
        self.media_type
    }

    /// Number of redirect hops followed before the final response.
    #[must_use]
    pub const fn redirect_count(&self) -> usize {
        self.response.redirect_count()
    }

    /// Strictly decoded UTF-8 document text.
    #[must_use]
    pub fn body(&self) -> &str {
        self.response.body()
    }

    /// UTF-8 bytes retained by the decoded document.
    #[must_use]
    pub fn body_bytes(&self) -> usize {
        self.response.body_bytes()
    }

    /// Reports network/cache origin of the admitted document.
    #[must_use]
    pub const fn cache_status(&self) -> CacheStatus {
        self.response.cache_status()
    }
}

/// Failures raised after or during top-level document loading.
#[derive(Debug, Error)]
pub enum DocumentLoadError {
    /// HTTP/network/URL/text-decoding failure.
    #[error(transparent)]
    Network(#[from] NetworkError),

    /// A successful response declared a media type Phantom does not render as a document.
    #[error("unsupported top-level document media type: {0}")]
    UnsupportedMediaType(String),

    /// No usable media type was declared and the bounded HTML sniff did not identify HTML.
    #[error("top-level document type could not be identified safely")]
    UnidentifiedMediaType,

    /// HTTP explicitly returned a no-content status.
    #[error("HTTP {status} contains no document body")]
    NoContent {
        /// HTTP status that explicitly carries no top-level document body.
        status: u16,
    },

    /// Partial content is not accepted as a top-level document representation.
    #[error("HTTP 206 partial content is not supported for top-level documents")]
    PartialContent,
}
// PHANTOM_2N9_CORS_SAFELISTED_RESPONSE_HEADERS_I
/// Bounded response-header view safe to expose across a successful CORS fetch.
///
/// Phantom intentionally admits only the Fetch CORS-safelisted response-header
/// names. Forbidden or arbitrary response headers never enter this view.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CorsSafelistedResponseHeaders {
    values: BTreeMap<String, String>,
}

impl CorsSafelistedResponseHeaders {
    /// Returns one admitted header value using an ASCII case-insensitive name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        let canonical = [
            "cache-control",
            "content-language",
            "content-length",
            "content-type",
            "expires",
            "last-modified",
            "pragma",
        ]
        .into_iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(name))?;
        self.values.get(canonical).map(String::as_str)
    }

    // PHANTOM_2N18_RESPONSE_HEADERS_EXPOSURE_I
    /// Returns the already-admitted safelist as owned name/value pairs.
    /// No raw HTTP header map crosses this boundary.
    #[must_use]
    pub fn into_pairs(&self) -> Vec<(String, String)> {
        self.values.iter().map(|(name, value)| (name.clone(), value.clone())).collect()
    }

    /// Number of admitted safelisted response headers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Reports whether no safelisted response header was admitted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// HTTP response decoded as text for the document pipeline.
#[derive(Clone, Debug)]
pub struct TextResponse {
    requested_url: String,
    final_url: String,
    status: u16,
    content_type: Option<String>,
    access_control_allow_origin: Option<String>,
    access_control_allow_credentials: Option<String>,
    set_cookie_fields: Vec<String>,
    cors_safelisted_response_headers: CorsSafelistedResponseHeaders,
    redirect_count: usize,
    body: Arc<str>,
    cache_status: CacheStatus,
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

    /// Final response `Access-Control-Allow-Origin`, when supplied.
    ///
    /// Multiple header field-lines are joined with a comma so the Browser Host
    /// can conservatively reject ambiguous CORS admission.
    #[must_use]
    pub fn access_control_allow_origin(&self) -> Option<&str> {
        self.access_control_allow_origin.as_deref()
    }

    /// Final response `Access-Control-Allow-Credentials`, when supplied.
    ///
    /// Credentialed CORS requires the exact case-sensitive token `true`.
    #[must_use]
    pub fn access_control_allow_credentials(&self) -> Option<&str> {
        self.access_control_allow_credentials.as_deref()
    }

    /// Bounded response headers admitted for CORS-safe exposure.
    #[must_use]
    pub const fn cors_safelisted_response_headers(&self) -> &CorsSafelistedResponseHeaders {
        &self.cors_safelisted_response_headers
    }

    /// Number of HTTP redirect hops followed before the final response.
    #[must_use]
    pub const fn redirect_count(&self) -> usize {
        self.redirect_count
    }

    /// Decoded response body.
    #[must_use]
    pub fn body(&self) -> &str {
        self.body.as_ref()
    }

    /// UTF-8 byte count held by the decoded body.
    #[must_use]
    pub fn body_bytes(&self) -> usize {
        self.body.len()
    }

    /// Reports whether the document came from network, fresh cache, or HTTP
    /// conditional revalidation.
    #[must_use]
    pub const fn cache_status(&self) -> CacheStatus {
        self.cache_status
    }
}

/// Origin of a response relative to Phantom's in-memory HTTP caches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheStatus {
    /// A network response was required and no reusable representation existed.
    Miss,

    /// A fresh cached representation satisfied the request without I/O.
    Fresh,

    /// A stale representation was conditionally validated with HTTP 304.
    Revalidated,

    /// A stale representation was used after a recoverable network/server error.
    StaleIfError,
}

/// Bounded binary HTTP response used for image and future subresources.
#[derive(Clone, Debug)]
pub struct BinaryResponse {
    requested_url: String,
    final_url: String,
    status: u16,
    content_type: Option<String>,
    body: Arc<[u8]>,
    cache_status: CacheStatus,
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
        self.body.as_ref()
    }

    /// Number of downloaded or cached response bytes retained in memory.
    #[must_use]
    pub fn body_bytes(&self) -> usize {
        self.body.len()
    }

    /// Reports whether the response came from network, fresh cache, 304
    /// revalidation, or stale-if-error recovery.
    #[must_use]
    pub const fn cache_status(&self) -> CacheStatus {
        self.cache_status
    }
}

#[derive(Clone, Debug, Default)]
struct CacheValidators {
    etag: Option<String>,
    last_modified: Option<String>,
}

impl CacheValidators {
    fn has_any(&self) -> bool {
        self.etag.is_some() || self.last_modified.is_some()
    }
}

#[derive(Clone, Debug)]
struct CachePolicy {
    store: bool,
    revalidate_always: bool,
    fresh_for: Duration,
    stale_if_error_for: Option<Duration>,
    must_revalidate: bool,
}

impl CachePolicy {
    fn fresh(&self, stored_at: Instant, now: Instant) -> bool {
        !self.revalidate_always && now.saturating_duration_since(stored_at) < self.fresh_for
    }

    fn permits_stale_if_error(&self, stored_at: Instant, now: Instant) -> bool {
        if self.must_revalidate {
            return false;
        }

        let Some(stale_window) = self.stale_if_error_for else {
            return false;
        };

        let age = now.saturating_duration_since(stored_at);
        let stale_age = age.saturating_sub(self.fresh_for);
        stale_age <= stale_window
    }
}

#[derive(Clone, Debug)]
struct CachedTextResponse {
    final_url: String,
    status: u16,
    content_type: Option<String>,
    access_control_allow_origin: Option<String>,
    cors_safelisted_response_headers: CorsSafelistedResponseHeaders,
    body: Arc<str>,
    validators: CacheValidators,
    policy: CachePolicy,
    stored_at: Instant,
    last_used: u64,
}

impl CachedTextResponse {
    fn to_response(
        &self,
        requested_url: &str,
        redirect_count: usize,
        cache_status: CacheStatus,
    ) -> TextResponse {
        TextResponse {
            requested_url: requested_url.to_owned(),
            final_url: self.final_url.clone(),
            status: self.status,
            content_type: self.content_type.clone(),
            access_control_allow_origin: self.access_control_allow_origin.clone(),
            access_control_allow_credentials: None,
            set_cookie_fields: Vec::new(),
            cors_safelisted_response_headers: self.cors_safelisted_response_headers.clone(),
            redirect_count,
            body: self.body.clone(),
            cache_status,
        }
    }
}

#[derive(Clone, Debug)]
struct CachedBinaryResponse {
    final_url: String,
    status: u16,
    content_type: Option<String>,
    body: Arc<[u8]>,
    validators: CacheValidators,
    policy: CachePolicy,
    stored_at: Instant,
    last_used: u64,
}

impl CachedBinaryResponse {
    fn to_response(&self, requested_url: &str, cache_status: CacheStatus) -> BinaryResponse {
        BinaryResponse {
            requested_url: requested_url.to_owned(),
            final_url: self.final_url.clone(),
            status: self.status,
            content_type: self.content_type.clone(),
            body: self.body.clone(),
            cache_status,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum CacheRequestVariant {
    Default,
    Cors(Origin),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PartitionedCacheKey {
    isolation_key: NetworkIsolationKey,
    resource_url: String,
    request_variant: CacheRequestVariant,
}

impl PartitionedCacheKey {
    fn new(isolation_key: &NetworkIsolationKey, resource_url: &str) -> Self {
        Self {
            isolation_key: isolation_key.clone(),
            resource_url: resource_url.to_owned(),
            request_variant: CacheRequestVariant::Default,
        }
    }

    fn new_cors(
        isolation_key: &NetworkIsolationKey,
        resource_url: &str,
        request_origin: &Origin,
    ) -> Self {
        Self {
            isolation_key: isolation_key.clone(),
            resource_url: resource_url.to_owned(),
            request_variant: CacheRequestVariant::Cors(request_origin.clone()),
        }
    }
}

#[derive(Debug)]
struct TextHttpCache {
    entries: BTreeMap<PartitionedCacheKey, CachedTextResponse>,
    body_bytes: u64,
    clock: u64,
    max_body_bytes: u64,
    max_entries: usize,
}

impl TextHttpCache {
    fn new(max_body_bytes: u64, max_entries: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            body_bytes: 0,
            clock: 0,
            max_body_bytes: max_body_bytes.max(1),
            max_entries: max_entries.max(1),
        }
    }

    fn get(&mut self, key: &PartitionedCacheKey) -> Option<CachedTextResponse> {
        self.clock = self.clock.saturating_add(1);
        let last_used = self.clock;
        let entry = self.entries.get_mut(key)?;
        entry.last_used = last_used;
        Some(entry.clone())
    }

    fn remove(&mut self, key: &PartitionedCacheKey) {
        if let Some(entry) = self.entries.remove(key) {
            self.body_bytes = self
                .body_bytes
                .saturating_sub(u64::try_from(entry.body.len()).unwrap_or(u64::MAX));
        }
    }

    fn insert(&mut self, key: PartitionedCacheKey, mut entry: CachedTextResponse) {
        let entry_bytes = u64::try_from(entry.body.len()).unwrap_or(u64::MAX);
        if entry_bytes > self.max_body_bytes {
            self.remove(&key);
            return;
        }

        self.remove(&key);
        self.clock = self.clock.saturating_add(1);
        entry.last_used = self.clock;

        while self.entries.len() >= self.max_entries
            || self.body_bytes.saturating_add(entry_bytes) > self.max_body_bytes
        {
            let victim = self
                .entries
                .iter()
                .min_by_key(|(_, candidate)| candidate.last_used)
                .map(|(candidate_key, _)| candidate_key.clone());

            let Some(victim_key) = victim else {
                break;
            };

            self.remove(&victim_key);
        }

        self.body_bytes = self.body_bytes.saturating_add(entry_bytes);
        self.entries.insert(key, entry);
    }
}

#[derive(Debug)]
struct BinaryHttpCache {
    entries: BTreeMap<PartitionedCacheKey, CachedBinaryResponse>,
    body_bytes: u64,
    clock: u64,
    max_body_bytes: u64,
    max_entries: usize,
}

impl BinaryHttpCache {
    fn new(max_body_bytes: u64, max_entries: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            body_bytes: 0,
            clock: 0,
            max_body_bytes: max_body_bytes.max(1),
            max_entries: max_entries.max(1),
        }
    }

    fn get(&mut self, key: &PartitionedCacheKey) -> Option<CachedBinaryResponse> {
        self.clock = self.clock.saturating_add(1);
        let last_used = self.clock;
        let entry = self.entries.get_mut(key)?;
        entry.last_used = last_used;
        Some(entry.clone())
    }

    fn remove(&mut self, key: &PartitionedCacheKey) {
        if let Some(entry) = self.entries.remove(key) {
            self.body_bytes = self
                .body_bytes
                .saturating_sub(u64::try_from(entry.body.len()).unwrap_or(u64::MAX));
        }
    }

    fn insert(&mut self, key: PartitionedCacheKey, mut entry: CachedBinaryResponse) {
        let entry_bytes = u64::try_from(entry.body.len()).unwrap_or(u64::MAX);
        if entry_bytes > self.max_body_bytes {
            self.remove(&key);
            return;
        }

        self.remove(&key);
        self.clock = self.clock.saturating_add(1);
        entry.last_used = self.clock;

        while self.entries.len() >= self.max_entries
            || self.body_bytes.saturating_add(entry_bytes) > self.max_body_bytes
        {
            let victim = self
                .entries
                .iter()
                .min_by_key(|(_, candidate)| candidate.last_used)
                .map(|(candidate_key, _)| candidate_key.clone());

            let Some(victim_key) = victim else {
                break;
            };

            self.remove(&victim_key);
        }

        self.body_bytes = self.body_bytes.saturating_add(entry_bytes);
        self.entries.insert(key, entry);
    }
}

// PHANTOM_2D5_PUBLIC_NETWORK_RESOLVER
/// Resolver used for public-network navigations and automatic subresources.
///
/// It delegates DNS to ureq's pinned default resolver and removes known private,
/// loopback, link-local, multicast, documentation and special-use addresses
/// before a connector sees them. This binds the private-network
/// policy to the address actually selected for the connection rather than only
/// to the URL hostname.
#[derive(Debug, Default)]
struct PublicNetworkResolver {
    inner: DefaultResolver,
}

impl Resolver for PublicNetworkResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let resolved = self.inner.resolve(uri, config, timeout)?;
        let mut allowed = self.empty();

        for address in resolved.iter().copied() {
            if is_public_network_ip(address.ip()) {
                allowed.push(address);
            }
        }

        if allowed.is_empty() {
            Err(ureq::Error::HostNotFound)
        } else {
            Ok(allowed)
        }
    }
}

fn is_public_network_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => is_public_ipv4(ipv4),
        IpAddr::V6(ipv6) => is_public_ipv6(ipv6),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();

    if a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || a >= 224
    {
        return false;
    }

    if (a, b, c) == (192, 0, 2)
        || (a, b, c) == (198, 51, 100)
        || (a, b, c) == (203, 0, 113)
        || (a, b, c) == (192, 0, 0)
    {
        return false;
    }

    true
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let octets = ip.octets();

    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() {
        return false;
    }

    // fc00::/7 unique-local.
    if octets[0] & 0xfe == 0xfc {
        return false;
    }

    // fe80::/10 link-local.
    if octets[0] == 0xfe && octets[1] & 0xc0 == 0x80 {
        return false;
    }

    // fec0::/10 deprecated site-local.
    if octets[0] == 0xfe && octets[1] & 0xc0 == 0xc0 {
        return false;
    }

    // 2001:db8::/32 documentation.
    if octets[0..4] == [0x20, 0x01, 0x0d, 0xb8] {
        return false;
    }

    // ::/96 includes unspecified/loopback-compatible legacy forms. The
    // IPv4-mapped form is handled separately below.
    let low_96 = octets[0..12].iter().all(|byte| *byte == 0);
    if low_96 {
        return false;
    }

    // IPv4-mapped IPv6: ::ffff:a.b.c.d.
    if octets[0..10] == [0_u8; 10] && octets[10] == 0xff && octets[11] == 0xff {
        return is_public_ipv4(Ipv4Addr::new(
            octets[12], octets[13], octets[14], octets[15],
        ));
    }

    true
}

fn host_is_private_namespace(host: &str) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();

    if normalized == "localhost"
        || normalized.ends_with(".localhost")
        || normalized.ends_with(".local")
        || normalized == "home.arpa"
        || normalized.ends_with(".home.arpa")
        || !normalized.contains('.')
    {
        return true;
    }

    normalized
        .parse::<IpAddr>()
        .is_ok_and(|address| !is_public_network_ip(address))
}

fn top_level_allows_private_subresources(key: &NetworkIsolationKey) -> bool {
    host_is_private_namespace(key.top_level_origin_value().host())
}

fn navigation_allows_private_network(requested_url: &HttpUrl) -> bool {
    requested_url
        .host_str()
        .is_some_and(host_is_private_namespace)
}

fn enforce_navigation_redirect_policy(
    private_network_allowed: bool,
    next_url: &HttpUrl,
) -> Result<(), NetworkError> {
    if !private_network_allowed && next_url.host_str().is_some_and(host_is_private_namespace) {
        return Err(NetworkError::PrivateNetworkBlocked {
            resource_url: next_url.as_str().to_owned(),
        });
    }

    Ok(())
}

fn enforce_subresource_policy(
    isolation_key: &NetworkIsolationKey,
    resource_url: &HttpUrl,
) -> Result<(), NetworkError> {
    if isolation_key
        .top_level_origin_value()
        .scheme()
        .is_secure_transport()
        && resource_url.scheme() == "http"
    {
        return Err(NetworkError::MixedContentBlocked {
            resource_url: resource_url.as_str().to_owned(),
        });
    }

    if !top_level_allows_private_subresources(isolation_key)
        && resource_url
            .host_str()
            .is_some_and(host_is_private_namespace)
    {
        return Err(NetworkError::PrivateNetworkBlocked {
            resource_url: resource_url.as_str().to_owned(),
        });
    }

    Ok(())
}

// PHANTOM_2N8_CORS_REDIRECT_ADMISSION_I
fn joined_response_header(
    headers: &ureq::http::HeaderMap,
    name: &str,
) -> Option<String> {
    let mut values = headers.get_all(name).iter().filter_map(|value| value.to_str().ok());
    values.next().map(|first| {
        let mut joined = first.to_owned();
        for value in values {
            joined.push_str(", ");
            joined.push_str(value);
        }
        joined
    })
}

fn cors_safelisted_response_headers(
    headers: &ureq::http::HeaderMap,
) -> CorsSafelistedResponseHeaders {
    const NAMES: [&str; 7] = [
        "cache-control",
        "content-language",
        "content-length",
        "content-type",
        "expires",
        "last-modified",
        "pragma",
    ];

    let mut values = BTreeMap::new();
    for name in NAMES {
        if let Some(value) = joined_response_header(headers, name)
            && value.len() <= MAX_CORS_SAFELISTED_RESPONSE_HEADER_VALUE_BYTES
        {
            values.insert(name.to_owned(), value);
        }
    }
    CorsSafelistedResponseHeaders { values }
}

// PHANTOM_2N14_CREDENTIALED_CORS_TRANSPORT_I
fn cors_credentialed_response_admits(
    request_origin: &Origin,
    allow_origin: Option<&str>,
    allow_credentials: Option<&str>,
) -> bool {
    let exact_origin = allow_origin
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|value| {
            value != "*"
                && value != "null"
                && !value.contains(',')
                && value.split_ascii_whitespace().count() == 1
                && value == request_origin.as_str()
        });
    let credentials = allow_credentials
        .map(str::trim)
        .is_some_and(|value| value == "true");
    exact_origin && credentials
}

fn bounded_set_cookie_fields(
    headers: &ureq::http::HeaderMap,
) -> Result<Vec<String>, NetworkError> {
    let mut fields = Vec::new();
    let mut total_bytes = 0_usize;

    for value in headers.get_all("set-cookie").iter() {
        if fields.len() >= MAX_SET_COOKIE_FIELDS_PER_RESPONSE {
            return Err(NetworkError::Request(
                "Set-Cookie response field count exceeds bounded limit".to_owned(),
            ));
        }
        let Ok(value) = value.to_str() else {
            continue;
        };
        if value.len() > MAX_SET_COOKIE_BYTES {
            return Err(NetworkError::Request(
                "Set-Cookie response field exceeds bounded limit".to_owned(),
            ));
        }
        total_bytes = total_bytes.saturating_add(value.len());
        if total_bytes > MAX_SET_COOKIE_RESPONSE_BYTES {
            return Err(NetworkError::Request(
                "Set-Cookie response bytes exceed bounded limit".to_owned(),
            ));
        }
        fields.push(value.to_owned());
    }

    Ok(fields)
}

fn cors_allow_origin_admits(request_origin: &Origin, allow_origin: Option<&str>) -> bool {
    allow_origin
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|value| {
            value == "*"
                || (value != "null"
                    && !value.contains(',')
                    && value.split_ascii_whitespace().count() == 1
                    && value == request_origin.as_str())
        })
}

fn read_decoded_body_bounded<R: Read>(
    reader: R,
    max_decoded_bytes: u64,
) -> Result<Vec<u8>, NetworkError> {
    let probe_limit = max_decoded_bytes.saturating_add(1);
    let mut reader = reader.take(probe_limit);
    let mut bytes = Vec::new();

    reader
        .read_to_end(&mut bytes)
        .map_err(|error| NetworkError::Body(error.to_string()))?;

    let Ok(decoded_length) = u64::try_from(bytes.len()) else {
        return Err(NetworkError::DecodedBodyLimitExceeded {
            limit: max_decoded_bytes,
        });
    };

    if decoded_length > max_decoded_bytes {
        return Err(NetworkError::DecodedBodyLimitExceeded {
            limit: max_decoded_bytes,
        });
    }

    Ok(bytes)
}

/// Reusable network client used by browser resource coordinators.
///
/// Clones share bounded process-memory document and binary caches. Every
/// reusable entry is partitioned by [`NetworkIsolationKey`] before its URL is
/// considered. Neither cache is persisted to disk.
#[derive(Clone)]
pub struct NetworkClient {
    agent: ureq::Agent,
    public_network_agent: ureq::Agent,
    max_text_body_bytes: u64,
    max_binary_body_bytes: u64,
    text_cache: Arc<Mutex<TextHttpCache>>,
    binary_cache: Arc<Mutex<BinaryHttpCache>>,
    cors_preflight_cache: Arc<Mutex<CorsPreflightCache>>,
    cookie_store: Arc<Mutex<CookieStore>>,
}

impl Default for NetworkClient {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_response_header_size(64 * 1024)
            .timeout_global(Some(Duration::from_secs(30)))
            .proxy(None)
            .user_agent("Phantom/0.0.1 (+https://github.com/eusourmr/phantom)")
            .build();

        let public_network_agent = ureq::Agent::with_parts(
            config.clone(),
            DefaultConnector::default(),
            PublicNetworkResolver::default(),
        );

        Self {
            agent: ureq::Agent::new_with_config(config),
            public_network_agent,
            max_text_body_bytes: DEFAULT_MAX_TEXT_BODY_BYTES,
            max_binary_body_bytes: DEFAULT_MAX_BINARY_BODY_BYTES,
            text_cache: Arc::new(Mutex::new(TextHttpCache::new(
                DEFAULT_MAX_TEXT_CACHE_BYTES,
                DEFAULT_MAX_TEXT_CACHE_ENTRIES,
            ))),
            binary_cache: Arc::new(Mutex::new(BinaryHttpCache::new(
                DEFAULT_MAX_BINARY_CACHE_BYTES,
                DEFAULT_MAX_BINARY_CACHE_ENTRIES,
            ))),
            cors_preflight_cache: Arc::new(Mutex::new(CorsPreflightCache::new(
                DEFAULT_MAX_CORS_PREFLIGHT_CACHE_ENTRIES,
            ))),
            cookie_store: Arc::new(Mutex::new(CookieStore::new(
                DEFAULT_MAX_COOKIE_STORE_ENTRIES,
            ))),
        }
    }
}

impl NetworkClient {
    /// Creates a client using Phantom's default bounded response and cache
    /// policy.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a client with one custom text-response size limit.
    ///
    /// The binary-resource and HTTP-cache limits remain at safe defaults.
    #[must_use]
    pub fn with_max_body_bytes(max_body_bytes: u64) -> Self {
        Self {
            max_text_body_bytes: max_body_bytes.max(1),
            ..Self::default()
        }
    }

    /// Builds a client with explicit document and binary-resource body limits.
    #[must_use]
    pub fn with_body_limits(max_text_body_bytes: u64, max_binary_body_bytes: u64) -> Self {
        Self {
            max_text_body_bytes: max_text_body_bytes.max(1),
            max_binary_body_bytes: max_binary_body_bytes.max(1),
            ..Self::default()
        }
    }

    /// Builds a client with explicit response and binary HTTP-cache limits.
    ///
    /// This constructor exists primarily for deterministic validation of cache
    /// eviction and bounded-memory behavior.
    #[must_use]
    pub fn with_cache_limits(
        max_text_body_bytes: u64,
        max_binary_body_bytes: u64,
        max_binary_cache_bytes: u64,
        max_binary_cache_entries: usize,
    ) -> Self {
        let mut client = Self::with_body_limits(max_text_body_bytes, max_binary_body_bytes);
        client.binary_cache = Arc::new(Mutex::new(BinaryHttpCache::new(
            max_binary_cache_bytes,
            max_binary_cache_entries,
        )));
        client
    }

    /// Builds a client with explicit response, document-cache and binary-cache
    /// limits. This constructor is intended for deterministic cache tests.
    #[must_use]
    pub fn with_all_cache_limits(
        max_text_body_bytes: u64,
        max_binary_body_bytes: u64,
        max_text_cache_bytes: u64,
        max_text_cache_entries: usize,
        max_binary_cache_bytes: u64,
        max_binary_cache_entries: usize,
    ) -> Self {
        let mut client = Self::with_cache_limits(
            max_text_body_bytes,
            max_binary_body_bytes,
            max_binary_cache_bytes,
            max_binary_cache_entries,
        );
        client.text_cache = Arc::new(Mutex::new(TextHttpCache::new(
            max_text_cache_bytes,
            max_text_cache_entries,
        )));
        client
    }

    /// Builds a client with an explicit in-memory cookie-store entry limit.
    ///
    /// This is primarily intended for deterministic bounded-store validation.
    #[must_use]
    pub fn with_cookie_store_limit(max_cookie_entries: usize) -> Self {
        Self {
            cookie_store: Arc::new(Mutex::new(CookieStore::new(max_cookie_entries))),
            ..Self::default()
        }
    }

    // PHANTOM_2N13_CREDENTIALS_COOKIE_FOUNDATION_I
    /// Processes one Set-Cookie field into Phantom's bounded host-only store.
    ///
    /// 2N-13 deliberately accepts only host-only cookies. `Domain` cookies are
    /// ignored until registrable-domain/public-suffix handling is available.
    /// No Cookie or Set-Cookie field is attached to live transport in this
    /// milestone.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] only when the bounded cookie store lock is
    /// poisoned.
    pub fn store_set_cookie(
        &self,
        document_url: &HttpUrl,
        response_url: &HttpUrl,
        credentials_mode: CredentialsMode,
        set_cookie: &str,
    ) -> Result<CookieStoreOutcome, NetworkError> {
        if !credentials_mode.accepts_response_credentials(document_url, response_url) {
            return Ok(CookieStoreOutcome::Ignored);
        }

        let Some(parsed) = parse_set_cookie(set_cookie, response_url) else {
            return Ok(CookieStoreOutcome::Ignored);
        };
        let Some(host) = response_url.host_str() else {
            return Ok(CookieStoreOutcome::Ignored);
        };

        let isolation_key = NetworkIsolationKey::from_top_level(document_url);
        let key = CookieStoreKey {
            isolation_key,
            host: host.to_ascii_lowercase(),
            path: parsed.path.clone(),
            name: parsed.name.clone(),
        };

        let mut store = self
            .cookie_store
            .lock()
            .map_err(|_| NetworkError::Request("cookie store lock poisoned".to_owned()))?;

        if parsed.max_age.is_some_and(|seconds| seconds <= 0) {
            return Ok(store.delete(&key));
        }

        store.clock = store.clock.saturating_add(1);
        let now = Instant::now();
        let expires_at = parsed.max_age.and_then(|seconds| {
            let seconds = u64::try_from(seconds).ok()?;
            Some(now + Duration::from_secs(seconds.min(MAX_COOKIE_AGE_SECS)))
        });
        let cookie = StoredCookie {
            value: parsed.value,
            secure: parsed.secure,
            _http_only: parsed.http_only,
            same_site: parsed.same_site,
            expires_at,
            creation_order: store.clock,
            last_used: store.clock,
        };
        Ok(store.insert(key, cookie, now))
    }

    /// Returns the Cookie header value that would be eligible for a request.
    ///
    /// This is an architecture/query surface only in 2N-13; callers do not
    /// attach the returned value to live network traffic yet.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] if the bounded store lock is poisoned or if the
    /// serialized Cookie field would exceed Phantom's configured limit.
    pub fn cookie_header_for_request(
        &self,
        document_url: &HttpUrl,
        request_url: &HttpUrl,
        credentials_mode: CredentialsMode,
        site_context: CookieSiteContext,
    ) -> Result<Option<String>, NetworkError> {
        if !credentials_mode.includes_request_credentials(document_url, request_url) {
            return Ok(None);
        }

        let isolation_key = NetworkIsolationKey::from_top_level(document_url);
        self.cookie_store
            .lock()
            .map_err(|_| NetworkError::Request("cookie store lock poisoned".to_owned()))?
            .cookie_header(
                &isolation_key,
                request_url,
                site_context,
                Instant::now(),
            )
    }

    /// Fetches an HTTP(S) document using normal navigation cache semantics.
    ///
    /// Fresh cached documents may satisfy a navigation without network I/O.
    /// Stale entries with validators are conditionally revalidated. Redirects
    /// remain explicit and bounded by Phantom.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for rejected URLs, transport failures, invalid
    /// redirect targets, redirect loops, excessive redirect chains, standalone
    /// `304 Not Modified`, or responses exceeding the configured text budget.
    pub fn fetch_text(&self, input: &str) -> Result<TextResponse, NetworkError> {
        self.fetch_text_with_mode(input, DocumentRequestMode::Navigate)
    }

    // PHANTOM_2L4_SAME_ORIGIN_FETCH_I
    /// Fetches bounded text for a page script without following cross-origin redirects.
    /// The caller supplies the active document URL as the origin authority.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for cross-origin URLs, redirects or network failures.
    pub fn fetch_text_same_origin(
        &self,
        input: &str,
        document_url: &HttpUrl,
    ) -> Result<TextResponse, NetworkError> {
        self.fetch_text_with_policy(
            input,
            DocumentRequestMode::Navigate,
            Some(document_url),
            TextRequestPolicy {
                cross_origin_mode: TextCrossOriginMode::Block,
                author_headers: &[],
                cache_enabled: true,
                credentials_mode: CredentialsMode::Omit,
                cookie_site_context: CookieSiteContext::CrossSite,
                request_method: TextRequestMethod::Get,
                request_body: None,
            },
        )
    }

    // PHANTOM_2L6_CLASSIC_SCRIPT_NETWORK_POLICY_I
    /// Fetches a classic script with explicit subresource checks on every hop.
    /// Cross-origin public scripts are allowed; cookies and credentials are absent.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for mixed content, private-network or transport failures.
    pub fn fetch_text_classic_script(
        &self,
        input: &str,
        document_url: &HttpUrl,
    ) -> Result<TextResponse, NetworkError> {
        self.fetch_text_with_policy(
            input,
            DocumentRequestMode::Navigate,
            Some(document_url),
            TextRequestPolicy {
                cross_origin_mode: TextCrossOriginMode::ClassicScript,
                author_headers: &[],
                cache_enabled: true,
                credentials_mode: CredentialsMode::Omit,
                cookie_site_context: CookieSiteContext::CrossSite,
                request_method: TextRequestMethod::Get,
                request_body: None,
            },
        )
    }

    // PHANTOM_2N7_CORS_TRANSPORT_I
    /// Fetches a simple page-script GET under Phantom's CORS transport policy.
    ///
    /// Same-origin requests retain their existing behavior. Cross-origin hops
    /// carry the canonical document `Origin` and expose only bounded CORS
    /// admission metadata to the Browser Host. Cookies, credentials,
    /// `Authorization`, request bodies and preflight are not enabled here.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for mixed-content, private-network, redirect,
    /// transport, decoding or bounded-cache failures.
    pub fn fetch_text_cors_simple(
        &self,
        input: &str,
        document_url: &HttpUrl,
    ) -> Result<TextResponse, NetworkError> {
        self.fetch_text_with_policy(
            input,
            DocumentRequestMode::Navigate,
            Some(document_url),
            TextRequestPolicy {
                cross_origin_mode: TextCrossOriginMode::CorsSimple,
                author_headers: &[],
                cache_enabled: true,
                credentials_mode: CredentialsMode::Omit,
                cookie_site_context: CookieSiteContext::CrossSite,
                request_method: TextRequestMethod::Get,
                request_body: None,
            },
        )
    }

    // PHANTOM_2N12_PREFLIGHT_METHODS_HEADERS_ADMISSION_II
    /// Fetches one credential-free GET with explicit author headers.
    ///
    /// If a cross-origin request requires preflight, Phantom completes or
    /// reuses OPTIONS admission before the main GET. Custom-header responses
    /// bypass the ordinary text cache in this milestone, preventing reuse under
    /// a different author-header set.
    ///
    /// Final cross-origin ACAO is validated before the response is returned.
    /// A main-request transport/CORS failure clears matching preflight cache
    /// entries fail-closed.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for forbidden/invalid headers, preflight denial,
    /// transport failure or final CORS denial.
    pub fn fetch_text_cors_get_with_headers(
        &self,
        input: &str,
        document_url: &HttpUrl,
        author_headers: &[(&str, &str)],
    ) -> Result<TextResponse, NetworkError> {
        if !cors_author_headers_transport_safe(author_headers) {
            return Err(NetworkError::Request(
                "CORS author header name/value is not transport-safe".to_owned(),
            ));
        }

        match classify_cors_request("GET", author_headers) {
            CorsRequestClassification::Forbidden => {
                return Err(NetworkError::Request(
                    "CORS request rejected forbidden author header".to_owned(),
                ));
            }
            CorsRequestClassification::Simple
            | CorsRequestClassification::PreflightRequired => {}
        }

        let resource_url = HttpUrl::parse(input)?;
        let cross_origin = !resource_url.same_origin(document_url);
        if cross_origin {
            self.ensure_cors_preflight(
                resource_url.as_str(),
                document_url,
                "GET",
                author_headers,
            )?;
        }

        let result = self.fetch_text_with_policy(
            resource_url.as_str(),
            DocumentRequestMode::Navigate,
            Some(document_url),
            TextRequestPolicy {
                cross_origin_mode: TextCrossOriginMode::CorsSimple,
                author_headers,
                cache_enabled: false,
                credentials_mode: CredentialsMode::Omit,
                cookie_site_context: CookieSiteContext::CrossSite,
                request_method: TextRequestMethod::Get,
                request_body: None,
            },
        );

        let response = match result {
            Ok(response) => response,
            Err(error) => {
                if cross_origin {
                    self.clear_cors_preflight_for(document_url, &resource_url)?;
                }
                return Err(error);
            }
        };

        // PHANTOM_2N19_FINAL_ORIGIN_CORS_HARDENING
        // Redirects can turn an initially same-origin request into a cross-origin
        // response. Admit the final origin independently of the initial origin.
        let final_url = HttpUrl::parse(response.final_url())?;
        if cors_final_origin_needs_admission(document_url, &final_url) {
            let request_origin = document_url.origin();
            if !cors_allow_origin_admits(
                &request_origin,
                response.access_control_allow_origin(),
            ) {
                self.clear_cors_preflight_for(document_url, &resource_url)?;
                return Err(NetworkError::Request(
                    "CORS final response denied request origin".to_owned(),
                ));
            }
        }

        Ok(response)
    }

    // PHANTOM_2N15_NON_SIMPLE_METHOD_BODY_TRANSPORT_I
    /// Sends a bounded credential-free POST, PUT or PATCH request under CORS.
    ///
    /// The request body is capped at 1 MiB. Author headers remain subject to
    /// Phantom's forbidden-header and transport-safety policy. Cross-origin
    /// requests perform or reuse preflight whenever the method/header matrix
    /// requires it. The final response must pass CORS admission before its body
    /// is returned.
    ///
    /// This milestone intentionally keeps body requests credential-free,
    /// uncached and redirect-free. JavaScript `fetch(url, options)` is not
    /// wired here yet.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for oversized bodies, forbidden headers,
    /// preflight denial, redirects, transport failure or final CORS denial.
    pub fn fetch_text_cors_with_body(
        &self,
        input: &str,
        document_url: &HttpUrl,
        method: CorsBodyMethod,
        author_headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<TextResponse, NetworkError> {
        if body.len() > MAX_CORS_REQUEST_BODY_BYTES {
            return Err(NetworkError::Request(format!(
                "CORS request body exceeds {} byte limit",
                MAX_CORS_REQUEST_BODY_BYTES
            )));
        }
        if !cors_author_headers_transport_safe(author_headers) {
            return Err(NetworkError::Request(
                "CORS body request author header is not transport-safe".to_owned(),
            ));
        }

        let method_name = method.as_str();
        if classify_cors_request(method_name, author_headers)
            == CorsRequestClassification::Forbidden
        {
            return Err(NetworkError::Request(
                "CORS body request rejected forbidden method/header".to_owned(),
            ));
        }

        let resource_url = HttpUrl::parse(input)?;
        let cross_origin = !resource_url.same_origin(document_url);
        if cross_origin {
            self.ensure_cors_preflight(
                resource_url.as_str(),
                document_url,
                method_name,
                author_headers,
            )?;
        }

        let result = self.fetch_text_with_policy(
            resource_url.as_str(),
            DocumentRequestMode::Navigate,
            Some(document_url),
            TextRequestPolicy {
                cross_origin_mode: TextCrossOriginMode::CorsSimple,
                author_headers,
                cache_enabled: false,
                credentials_mode: CredentialsMode::Omit,
                cookie_site_context: CookieSiteContext::CrossSite,
                request_method: method.text_request_method(),
                request_body: Some(body),
            },
        );

        let response = match result {
            Ok(response) => response,
            Err(error) => {
                if cross_origin {
                    self.clear_cors_preflight_for(document_url, &resource_url)?;
                }
                return Err(error);
            }
        };

        // PHANTOM_2N19_FINAL_ORIGIN_CORS_HARDENING
        // Redirects can turn an initially same-origin request into a cross-origin
        // response. Admit the final origin independently of the initial origin.
        let final_url = HttpUrl::parse(response.final_url())?;
        if cors_final_origin_needs_admission(document_url, &final_url) {
            let request_origin = document_url.origin();
            if !cors_allow_origin_admits(
                &request_origin,
                response.access_control_allow_origin(),
            ) {
                self.clear_cors_preflight_for(document_url, &resource_url)?;
                return Err(NetworkError::Request(
                    "CORS body response denied request origin".to_owned(),
                ));
            }
        }

        Ok(response)
    }

    // PHANTOM_2N14_CREDENTIALED_CORS_TRANSPORT_I
    /// Fetches a GET using explicit credentialed CORS transport.
    ///
    /// This is intentionally not wired to JavaScript `fetch(..., options)` yet.
    /// The caller supplies SameSite context explicitly. Cross-origin responses
    /// require exact ACAO plus case-sensitive `Access-Control-Allow-Credentials:
    /// true`; wildcard ACAO is rejected. Preflight requests never receive a
    /// Cookie header. Set-Cookie state is committed only after final CORS
    /// admission succeeds.
    ///
    /// Credentialed redirects remain fail-closed in 2N-14.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for invalid author headers, preflight denial,
    /// credentialed CORS denial, bounded cookie failures, redirects or transport
    /// errors.
    pub fn fetch_text_cors_credentialed_get_with_headers(
        &self,
        input: &str,
        document_url: &HttpUrl,
        author_headers: &[(&str, &str)],
        site_context: CookieSiteContext,
    ) -> Result<TextResponse, NetworkError> {
        if !cors_author_headers_transport_safe(author_headers) {
            return Err(NetworkError::Request(
                "credentialed CORS author header is not transport-safe".to_owned(),
            ));
        }
        if classify_cors_request("GET", author_headers) == CorsRequestClassification::Forbidden {
            return Err(NetworkError::Request(
                "credentialed CORS request rejected forbidden author header".to_owned(),
            ));
        }

        let resource_url = HttpUrl::parse(input)?;
        let cross_origin = !resource_url.same_origin(document_url);
        if cross_origin {
            self.ensure_cors_preflight_with_credentials(
                resource_url.as_str(),
                document_url,
                "GET",
                author_headers,
                CredentialsMode::Include,
            )?;
        }

        let result = self.fetch_text_with_policy(
            resource_url.as_str(),
            DocumentRequestMode::Navigate,
            Some(document_url),
            TextRequestPolicy {
                cross_origin_mode: TextCrossOriginMode::CorsSimple,
                author_headers,
                cache_enabled: false,
                credentials_mode: CredentialsMode::Include,
                cookie_site_context: site_context,
                request_method: TextRequestMethod::Get,
                request_body: None,
            },
        );

        let response = match result {
            Ok(response) => response,
            Err(error) => {
                if cross_origin {
                    self.clear_cors_preflight_for(document_url, &resource_url)?;
                }
                return Err(error);
            }
        };

        // PHANTOM_2N19_FINAL_ORIGIN_CORS_HARDENING
        let final_url = HttpUrl::parse(response.final_url())?;
        if cors_final_origin_needs_admission(document_url, &final_url) {
            let request_origin = document_url.origin();
            if !cors_credentialed_response_admits(
                &request_origin,
                response.access_control_allow_origin(),
                response.access_control_allow_credentials(),
            ) {
                self.clear_cors_preflight_for(document_url, &resource_url)?;
                return Err(NetworkError::Request(
                    "credentialed CORS final response denied origin/credentials".to_owned(),
                ));
            }
        }

        let final_url = HttpUrl::parse(response.final_url())?;
        for set_cookie in &response.set_cookie_fields {
            self.store_set_cookie(
                document_url,
                &final_url,
                CredentialsMode::Include,
                set_cookie,
            )?;
        }

        Ok(response)
    }

    // PHANTOM_2N11_CORS_PREFLIGHT_FOUNDATION_I
    /// Performs or reuses a bounded CORS preflight for one future request.
    ///
    /// The preflight itself is credential-free and never follows redirects.
    /// `Access-Control-Allow-Origin`, method and unsafe author-header admission
    /// are all checked before a cache entry is installed.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] when the request is forbidden, the OPTIONS
    /// transport fails, the response is non-2xx, CORS admission fails or the
    /// response omits required method/header permission.
    pub fn ensure_cors_preflight(
        &self,
        input: &str,
        document_url: &HttpUrl,
        method: &str,
        author_headers: &[(&str, &str)],
    ) -> Result<CorsPreflightOutcome, NetworkError> {
        self.ensure_cors_preflight_with_credentials(
            input,
            document_url,
            method,
            author_headers,
            CredentialsMode::Omit,
        )
    }

    fn ensure_cors_preflight_with_credentials(
        &self,
        input: &str,
        document_url: &HttpUrl,
        method: &str,
        author_headers: &[(&str, &str)],
        credentials_mode: CredentialsMode,
    ) -> Result<CorsPreflightOutcome, NetworkError> {
        match classify_cors_request(method, author_headers) {
            CorsRequestClassification::Simple => {
                return Ok(CorsPreflightOutcome::NotRequired);
            }
            CorsRequestClassification::Forbidden => {
                return Err(NetworkError::Request(
                    "CORS preflight rejected forbidden method/header".to_owned(),
                ));
            }
            CorsRequestClassification::PreflightRequired => {}
        }

        let resource_url = HttpUrl::parse(input)?;
        let isolation_key = NetworkIsolationKey::from_top_level(document_url);
        enforce_subresource_policy(&isolation_key, &resource_url)?;

        if resource_url.same_origin(document_url) {
            return Ok(CorsPreflightOutcome::NotRequired);
        }

        let request_origin = document_url.origin();
        let unsafe_header_names = cors_unsafe_request_header_names(author_headers);
        let cache_key = CorsPreflightCacheKey {
            isolation_key: isolation_key.clone(),
            request_origin: request_origin.clone(),
            resource_url: resource_url.as_str().to_owned(),
            method: method.to_ascii_uppercase(),
            unsafe_header_names: unsafe_header_names.clone(),
            credentials_mode,
        };

        if self
            .cors_preflight_cache
            .lock()
            .map_err(|_| NetworkError::Request("CORS preflight cache lock poisoned".to_owned()))?
            .admits(&cache_key, Instant::now())
        {
            return Ok(CorsPreflightOutcome::Cached);
        }

        let private_network_allowed = navigation_allows_private_network(document_url);
        enforce_navigation_redirect_policy(private_network_allowed, &resource_url)?;
        let agent = if private_network_allowed {
            &self.agent
        } else {
            &self.public_network_agent
        };

        let mut request = agent
            .options(resource_url.as_str())
            .header("Accept", "*/*")
            .header("Origin", request_origin.as_str())
            .header("Access-Control-Request-Method", method);

        if !unsafe_header_names.is_empty() {
            request = request.header(
                "Access-Control-Request-Headers",
                unsafe_header_names.join(","),
            );
        }

        let response = request
            .call()
            .map_err(|error| NetworkError::Request(error.to_string()))?;
        let status = response.status().as_u16();
        if !(200..=299).contains(&status) {
            return Err(NetworkError::Request(format!(
                "CORS preflight returned HTTP {status}"
            )));
        }

        let header_bytes = response
            .headers()
            .iter()
            .map(|(name, value)| name.as_str().len().saturating_add(value.as_bytes().len()))
            .sum::<usize>();
        if header_bytes > MAX_CORS_PREFLIGHT_RESPONSE_HEADER_BYTES {
            return Err(NetworkError::Request(
                "CORS preflight response headers exceed bounded limit".to_owned(),
            ));
        }

        let allow_origin =
            joined_response_header(response.headers(), "access-control-allow-origin");
        let allow_credentials =
            joined_response_header(response.headers(), "access-control-allow-credentials");
        let origin_admitted = if credentials_mode == CredentialsMode::Include {
            cors_credentialed_response_admits(
                &request_origin,
                allow_origin.as_deref(),
                allow_credentials.as_deref(),
            )
        } else {
            cors_allow_origin_admits(&request_origin, allow_origin.as_deref())
        };
        if !origin_admitted {
            return Err(NetworkError::Request(
                "CORS preflight response denied request origin/credentials".to_owned(),
            ));
        }

        if !cors_method_is_safelisted(method) {
            let allow_methods =
                joined_response_header(response.headers(), "access-control-allow-methods");
            if !comma_list_contains_token_for_credentials(
                allow_methods.as_deref(),
                method,
                credentials_mode,
            ) {
                return Err(NetworkError::Request(
                    "CORS preflight response denied request method".to_owned(),
                ));
            }
        }

        let allow_headers =
            joined_response_header(response.headers(), "access-control-allow-headers");
        if !cors_preflight_allows_headers_for_credentials(
            allow_headers.as_deref(),
            &unsafe_header_names,
            credentials_mode,
        ) {
            return Err(NetworkError::Request(
                "CORS preflight response denied request headers".to_owned(),
            ));
        }

        let max_age = joined_response_header(response.headers(), "access-control-max-age")
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_CORS_PREFLIGHT_MAX_AGE_SECS)
            .min(MAX_CORS_PREFLIGHT_MAX_AGE_SECS);

        if max_age > 0 {
            let expires_at = Instant::now() + Duration::from_secs(max_age);
            self.cors_preflight_cache
                .lock()
                .map_err(|_| {
                    NetworkError::Request("CORS preflight cache lock poisoned".to_owned())
                })?
                .insert(cache_key, expires_at);
        }

        Ok(CorsPreflightOutcome::Performed)
    }

    fn clear_cors_preflight_for(
        &self,
        document_url: &HttpUrl,
        resource_url: &HttpUrl,
    ) -> Result<(), NetworkError> {
        let isolation_key = NetworkIsolationKey::from_top_level(document_url);
        let request_origin = document_url.origin();
        self.cors_preflight_cache
            .lock()
            .map_err(|_| NetworkError::Request("CORS preflight cache lock poisoned".to_owned()))?
            .clear_for(&isolation_key, &request_origin, resource_url.as_str());
        Ok(())
    }

    /// Reloads an HTTP(S) document, bypassing fresh-cache reuse and requesting
    /// origin revalidation with `Cache-Control: max-age=0`.
    ///
    /// Cached `ETag` and `Last-Modified` validators are sent when available. A
    /// valid `304 Not Modified` response refreshes metadata and reuses the
    /// cached body.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] under the same conditions as [`Self::fetch_text`].
    pub fn reload_text(&self, input: &str) -> Result<TextResponse, NetworkError> {
        self.fetch_text_with_mode(input, DocumentRequestMode::Reload)
    }

    /// Fetches and admits a top-level HTML/XHTML document.
    ///
    /// This preserves the 2C document-cache semantics, but adds explicit
    /// top-level media/status admission after the bounded text response has
    /// been decoded.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentLoadError`] for network failures, unsupported media
    /// types, no-content/partial responses, or text-decoding policy failures.
    pub fn fetch_document(&self, input: &str) -> Result<DocumentResponse, DocumentLoadError> {
        let response = self.fetch_text(input)?;
        admit_document(response)
    }

    /// Reloads and admits a top-level HTML/XHTML document.
    ///
    /// Reload preserves the 2C cache-revalidation contract (`max-age=0`,
    /// validators and HTTP 304 body reuse).
    ///
    /// # Errors
    ///
    /// Returns [`DocumentLoadError`] under the same document-admission policy
    /// as [`Self::fetch_document`].
    pub fn reload_document(&self, input: &str) -> Result<DocumentResponse, DocumentLoadError> {
        let response = self.reload_text(input)?;
        admit_document(response)
    }
    /// Fetches a top-level document using an explicit navigation cache mode.
    ///
    /// Redirect responses themselves are not cached in this milestone. When a
    /// redirect reaches a final URL that already has a fresh representation,
    /// normal navigation may reuse that final cached representation.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for URL, redirect, transport, body or invalid
    /// revalidation failures.
    pub fn fetch_text_with_mode(
        &self,
        input: &str,
        mode: DocumentRequestMode,
    ) -> Result<TextResponse, NetworkError> {
        self.fetch_text_with_policy(
            input,
            mode,
            None,
            TextRequestPolicy {
                cross_origin_mode: TextCrossOriginMode::Block,
                author_headers: &[],
                cache_enabled: true,
                credentials_mode: CredentialsMode::Omit,
                cookie_site_context: CookieSiteContext::CrossSite,
                request_method: TextRequestMethod::Get,
                request_body: None,
            },
        )
    }

    fn fetch_text_with_policy(
        &self,
        input: &str,
        mode: DocumentRequestMode,
        document_url: Option<&HttpUrl>,
        policy: TextRequestPolicy<'_>,
    ) -> Result<TextResponse, NetworkError> {
        let TextRequestPolicy {
            cross_origin_mode,
            author_headers,
            cache_enabled,
            credentials_mode,
            cookie_site_context,
            request_method,
            request_body,
        } = policy;
        if request_method == TextRequestMethod::Get && request_body.is_some() {
            return Err(NetworkError::Request(
                "GET request cannot carry a body in text transport".to_owned(),
            ));
        }
        if request_method.carries_body() && request_body.is_none() {
            return Err(NetworkError::Request(
                "body-capable text request is missing request body".to_owned(),
            ));
        }
        if request_body.is_some_and(|body| body.len() > MAX_CORS_REQUEST_BODY_BYTES) {
            return Err(NetworkError::Request(format!(
                "CORS request body exceeds {} byte limit",
                MAX_CORS_REQUEST_BODY_BYTES
            )));
        }
        if request_method.carries_body() && cache_enabled {
            return Err(NetworkError::Request(
                "body text transport cannot use the GET response cache".to_owned(),
            ));
        }

        let mut current_url = HttpUrl::parse(input)?;
        if let Some(document) = document_url {
            enforce_subresource_policy(&NetworkIsolationKey::from_top_level(document), &current_url)?;
        }
        if cross_origin_mode == TextCrossOriginMode::Block
            && document_url.is_some_and(|document| !current_url.same_origin(document))
        {
            return Err(NetworkError::CrossOriginBlocked {
                resource_url: current_url.as_str().to_owned(),
            });
        }
        let private_network_allowed = document_url.map_or_else(
            || navigation_allows_private_network(&current_url),
            navigation_allows_private_network,
        );
        let requested_url = current_url.as_str().to_owned();
        let mut visited = BTreeSet::new();
        visited.insert(redirect_visit_key(&current_url));
        let mut redirect_count = 0_usize;

        loop {
            let isolation_key = document_url.map_or_else(
                || NetworkIsolationKey::from_top_level(&current_url),
                NetworkIsolationKey::from_top_level,
            );
            let cors_request_origin = document_url
                .filter(|document| {
                    cross_origin_mode == TextCrossOriginMode::CorsSimple
                        && !current_url.same_origin(document)
                })
                .map(HttpUrl::origin);
            let cache_key = cors_request_origin.as_ref().map_or_else(
                || PartitionedCacheKey::new(&isolation_key, current_url.as_str()),
                |origin| {
                    PartitionedCacheKey::new_cors(
                        &isolation_key,
                        current_url.as_str(),
                        origin,
                    )
                },
            );
            let cached = if cache_enabled {
                self.cached_text(&cache_key)
            } else {
                None
            };
            let now = Instant::now();

            if mode == DocumentRequestMode::Navigate
                && let Some(entry) = cached.as_ref()
                && entry.policy.fresh(entry.stored_at, now)
            {
                return Ok(entry.to_response(&requested_url, redirect_count, CacheStatus::Fresh));
            }

            let agent = if private_network_allowed {
                &self.agent
            } else {
                &self.public_network_agent
            };

            let author_supplies_accept = author_headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("accept"));

            let mut response = if request_method == TextRequestMethod::Get {
                let mut request = agent.get(current_url.as_str());
                if !author_supplies_accept {
                    request = request.header(
                        "Accept",
                        "text/html,application/xhtml+xml,text/css,text/plain;q=0.9,*/*;q=0.5",
                    );
                }
                for (name, value) in author_headers {
                    request = request.header(*name, *value);
                }

                if let Some(origin) = cors_request_origin.as_ref() {
                    request = request.header("Origin", origin.as_str());
                }

                if let Some(document) = document_url
                    && let Some(cookie_header) = self.cookie_header_for_request(
                        document,
                        &current_url,
                        credentials_mode,
                        cookie_site_context,
                    )?
                {
                    request = request.header("Cookie", cookie_header);
                }

                if mode == DocumentRequestMode::Reload {
                    request = request.header("Cache-Control", "max-age=0");
                }

                if let Some(entry) = cached.as_ref() {
                    if let Some(etag) = entry.validators.etag.as_deref() {
                        request = request.header("If-None-Match", etag);
                    }

                    if let Some(last_modified) = entry.validators.last_modified.as_deref() {
                        request = request.header("If-Modified-Since", last_modified);
                    }
                }

                request
                    .call()
                    .map_err(|error| NetworkError::Request(error.to_string()))?
            } else {
                let mut request = match request_method {
                    TextRequestMethod::Post => agent.post(current_url.as_str()),
                    TextRequestMethod::Put => agent.put(current_url.as_str()),
                    TextRequestMethod::Patch => agent.patch(current_url.as_str()),
                    TextRequestMethod::Get => {
                        return Err(NetworkError::Request(
                            "invalid GET body transport state".to_owned(),
                        ));
                    }
                };

                if !author_supplies_accept {
                    request = request.header(
                        "Accept",
                        "text/html,application/xhtml+xml,text/css,text/plain;q=0.9,*/*;q=0.5",
                    );
                }
                for (name, value) in author_headers {
                    request = request.header(*name, *value);
                }
                if let Some(origin) = cors_request_origin.as_ref() {
                    request = request.header("Origin", origin.as_str());
                }

                request
                    .send(request_body.unwrap_or_default())
                    .map_err(|error| NetworkError::Request(error.to_string()))?
            };
            let status = response.status().as_u16();

            if is_document_redirect_status(status) {
                if cache_enabled {
                    self.remove_cached_text(&cache_key);
                }

                if request_method.carries_body() {
                    return Err(NetworkError::Request(
                        "CORS body request redirect transport is deferred".to_owned(),
                    ));
                }

                if credentials_mode == CredentialsMode::Include {
                    return Err(NetworkError::Request(
                        "credentialed CORS redirect transport is deferred".to_owned(),
                    ));
                }

                if !author_headers.is_empty() {
                    return Err(NetworkError::Request(
                        "CORS author-header request redirect requires new admission".to_owned(),
                    ));
                }

                if redirect_count >= MAX_DOCUMENT_REDIRECTS {
                    return Err(NetworkError::RedirectLimitExceeded {
                        limit: MAX_DOCUMENT_REDIRECTS,
                    });
                }

                // PHANTOM_2N8_CORS_REDIRECT_ADMISSION_I
                if cross_origin_mode == TextCrossOriginMode::CorsSimple
                    && let Some(document) = document_url
                    && !current_url.same_origin(document)
                {
                    let request_origin = document.origin();
                    let allow_origin =
                        joined_response_header(response.headers(), "access-control-allow-origin");
                    if !cors_allow_origin_admits(&request_origin, allow_origin.as_deref()) {
                        return Err(NetworkError::CorsRedirectBlocked {
                            resource_url: current_url.as_str().to_owned(),
                        });
                    }
                }

                let location = response
                    .headers()
                    .get("location")
                    .and_then(|value| value.to_str().ok())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or(NetworkError::RedirectMissingLocation { status })?;

                let next_url = current_url.resolve(location)?;
                if let Some(document) = document_url {
                    enforce_subresource_policy(&NetworkIsolationKey::from_top_level(document), &next_url)?;
                }
                if cross_origin_mode == TextCrossOriginMode::Block
                    && document_url.is_some_and(|document| !next_url.same_origin(document))
                {
                    return Err(NetworkError::CrossOriginBlocked {
                        resource_url: next_url.as_str().to_owned(),
                    });
                }
                enforce_navigation_redirect_policy(private_network_allowed, &next_url)?;
                let next_key = redirect_visit_key(&next_url);

                if !visited.insert(next_key.clone()) {
                    return Err(NetworkError::RedirectLoop(next_key));
                }

                current_url = next_url;
                redirect_count = redirect_count.saturating_add(1);
                continue;
            }

            let final_url = HttpUrl::parse(&response.get_uri().to_string())?;
            if let Some(document) = document_url {
                enforce_subresource_policy(&NetworkIsolationKey::from_top_level(document), &final_url)?;
            }
            if cross_origin_mode == TextCrossOriginMode::Block
                && document_url.is_some_and(|document| !final_url.same_origin(document))
            {
                return Err(NetworkError::CrossOriginBlocked {
                    resource_url: final_url.as_str().to_owned(),
                });
            }
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let access_control_allow_origin =
                joined_response_header(response.headers(), "access-control-allow-origin");
            let access_control_allow_credentials =
                joined_response_header(response.headers(), "access-control-allow-credentials");
            let set_cookie_fields = bounded_set_cookie_fields(response.headers())?;
            let cors_safelisted_response_headers =
                cors_safelisted_response_headers(response.headers());
            let etag = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let last_modified = response
                .headers()
                .get("last-modified")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let cache_control = response
                .headers()
                .get("cache-control")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let age = response
                .headers()
                .get("age")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let vary = response
                .headers()
                .get("vary")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);

            if status == 304 {
                let Some(existing) = cached.as_ref() else {
                    return Err(NetworkError::UnexpectedNotModified);
                };

                let validators = CacheValidators {
                    etag: etag.or_else(|| existing.validators.etag.clone()),
                    last_modified: last_modified
                        .or_else(|| existing.validators.last_modified.clone()),
                };

                let policy = if cache_control.is_some() || vary.is_some() {
                    cache_policy(
                        cache_control.as_deref(),
                        age.as_deref(),
                        vary.as_deref(),
                        validators.has_any(),
                    )
                } else {
                    existing.policy.clone()
                };

                let mut refreshed_cors_safelisted_response_headers =
                    existing.cors_safelisted_response_headers.clone();
                for (name, value) in cors_safelisted_response_headers.values {
                    refreshed_cors_safelisted_response_headers.values.insert(name, value);
                }

                let refreshed = CachedTextResponse {
                    final_url: final_url.as_str().to_owned(),
                    status: existing.status,
                    content_type: content_type.or_else(|| existing.content_type.clone()),
                    access_control_allow_origin: access_control_allow_origin
                        .or_else(|| existing.access_control_allow_origin.clone()),
                    cors_safelisted_response_headers: refreshed_cors_safelisted_response_headers,
                    body: existing.body.clone(),
                    validators,
                    policy,
                    stored_at: Instant::now(),
                    last_used: 0,
                };

                if cache_enabled {
                    if refreshed.policy.store {
                        self.store_cached_text(cache_key.clone(), refreshed.clone());
                    } else {
                        self.remove_cached_text(&cache_key);
                    }
                }

                return Ok(refreshed.to_response(
                    &requested_url,
                    redirect_count,
                    CacheStatus::Revalidated,
                ));
            }

            let reader = response
                .body_mut()
                .with_config()
                .limit(self.max_text_body_bytes)
                .reader();
            let body_bytes = read_decoded_body_bounded(reader, self.max_text_body_bytes)?;

            let body = decode_text_body(&body_bytes, content_type.as_deref())?;

            let text_response = TextResponse {
                requested_url: requested_url.clone(),
                final_url: final_url.as_str().to_owned(),
                status,
                content_type: content_type.clone(),
                access_control_allow_origin: access_control_allow_origin.clone(),
                access_control_allow_credentials: access_control_allow_credentials.clone(),
                set_cookie_fields,
                cors_safelisted_response_headers: cors_safelisted_response_headers.clone(),
                redirect_count,
                body: body.clone(),
                cache_status: CacheStatus::Miss,
            };

            if cache_enabled {
                if status == 200 {
                    let validators = CacheValidators {
                        etag,
                        last_modified,
                    };
                    let policy = cache_policy(
                        cache_control.as_deref(),
                        age.as_deref(),
                        vary.as_deref(),
                        validators.has_any(),
                    );

                    if policy.store {
                        self.store_cached_text(
                            cache_key,
                            CachedTextResponse {
                                final_url: text_response.final_url.clone(),
                                status,
                                content_type,
                                access_control_allow_origin,
                                cors_safelisted_response_headers,
                                body,
                                validators,
                                policy,
                                stored_at: Instant::now(),
                                last_used: 0,
                            },
                        );
                    } else {
                        self.remove_cached_text(&cache_key);
                    }
                } else {
                    self.remove_cached_text(&cache_key);
                }
            }

            return Ok(text_response);
        }
    }

    /// Fetches a bounded binary HTTP(S) subresource inside an explicit network
    /// isolation partition, with conditional HTTP cache revalidation and one
    /// bounded transient-error retry.
    ///
    /// The request advertises the image formats enabled by the current Phantom
    /// decoder. Fresh cache entries are served without network I/O. Stale
    /// entries use `If-None-Match` and/or `If-Modified-Since` when validators are
    /// available. A `304 Not Modified` response reuses the existing bytes.
    ///
    /// Cache lookup, insertion, revalidation and stale recovery are all scoped
    /// by `isolation_key`. The same third-party resource URL therefore cannot
    /// reuse cached state across two different top-level contexts.
    ///
    /// Recovery remains conservative: Phantom retries a transient transport or
    /// selected server failure at most once. A stale cached representation is
    /// used only when the origin explicitly supplied `stale-if-error` and did
    /// not require `must-revalidate`.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for transport failures that cannot be recovered,
    /// invalid redirect targets, invalid standalone `304` responses, or a body
    /// exceeding the configured binary byte budget.
    pub fn fetch_bytes_partitioned(
        &self,
        isolation_key: &NetworkIsolationKey,
        url: &HttpUrl,
    ) -> Result<BinaryResponse, NetworkError> {
        self.fetch_bytes_partitioned_with_limit(isolation_key, url, self.max_binary_body_bytes)
    }

    /// Fetches one partitioned binary subresource with an additional
    /// caller-supplied body ceiling.
    ///
    /// The effective ceiling is the smaller of this value and the client-wide
    /// binary limit. This lets a document coordinator enforce an aggregate
    /// budget without weakening the process-wide transport policy.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkError`] for policy, transport, cache or body-limit
    /// failures.
    pub fn fetch_bytes_partitioned_with_limit(
        &self,
        isolation_key: &NetworkIsolationKey,
        url: &HttpUrl,
        max_body_bytes: u64,
    ) -> Result<BinaryResponse, NetworkError> {
        enforce_subresource_policy(isolation_key, url)?;

        let effective_body_limit = self.max_binary_body_bytes.min(max_body_bytes.max(1));
        let requested_url = url.as_str().to_owned();
        let cache_key = PartitionedCacheKey::new(isolation_key, &requested_url);
        let cached = self.cached_binary(&cache_key);
        let now = Instant::now();

        if let Some(entry) = cached.as_ref()
            && entry.policy.fresh(entry.stored_at, now)
        {
            let response = entry.to_response(&requested_url, CacheStatus::Fresh);
            ensure_binary_response_limit(&response, effective_body_limit)?;
            return Ok(response);
        }

        let agent = if top_level_allows_private_subresources(isolation_key) {
            &self.agent
        } else {
            &self.public_network_agent
        };

        for attempt in 0..MAX_BINARY_FETCH_ATTEMPTS {
            let mut request = agent.get(url.as_str()).header("Accept", IMAGE_ACCEPT);

            if let Some(entry) = cached.as_ref() {
                if let Some(etag) = entry.validators.etag.as_deref() {
                    request = request.header("If-None-Match", etag);
                }

                if let Some(last_modified) = entry.validators.last_modified.as_deref() {
                    request = request.header("If-Modified-Since", last_modified);
                }
            }

            let response_result = request.call();

            let mut response = match response_result {
                Ok(response) => response,
                Err(error) => {
                    if attempt + 1 < MAX_BINARY_FETCH_ATTEMPTS {
                        continue;
                    }

                    if let Some(recovered) = stale_recovery(cached.as_ref(), &requested_url) {
                        ensure_binary_response_limit(&recovered, effective_body_limit)?;
                        return Ok(recovered);
                    }

                    return Err(NetworkError::Request(error.to_string()));
                }
            };

            let status = response.status().as_u16();

            if is_transient_status(status) && attempt + 1 < MAX_BINARY_FETCH_ATTEMPTS {
                continue;
            }

            if is_recoverable_status(status)
                && let Some(recovered) = stale_recovery(cached.as_ref(), &requested_url)
            {
                ensure_binary_response_limit(&recovered, effective_body_limit)?;
                return Ok(recovered);
            }

            let final_url = HttpUrl::parse(&response.get_uri().to_string())?;
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let etag = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let last_modified = response
                .headers()
                .get("last-modified")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let cache_control = response
                .headers()
                .get("cache-control")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let age = response
                .headers()
                .get("age")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let vary = response
                .headers()
                .get("vary")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);

            if status == 304 {
                let Some(existing) = cached.as_ref() else {
                    return Err(NetworkError::UnexpectedNotModified);
                };

                let validators = CacheValidators {
                    etag: etag.or_else(|| existing.validators.etag.clone()),
                    last_modified: last_modified
                        .or_else(|| existing.validators.last_modified.clone()),
                };

                let policy = if cache_control.is_some() || vary.is_some() {
                    cache_policy(
                        cache_control.as_deref(),
                        age.as_deref(),
                        vary.as_deref(),
                        validators.has_any(),
                    )
                } else {
                    existing.policy.clone()
                };

                let refreshed = CachedBinaryResponse {
                    final_url: final_url.as_str().to_owned(),
                    status: existing.status,
                    content_type: content_type.or_else(|| existing.content_type.clone()),
                    body: existing.body.clone(),
                    validators,
                    policy,
                    stored_at: Instant::now(),
                    last_used: 0,
                };

                if refreshed.policy.store {
                    self.store_cached_binary(cache_key.clone(), refreshed.clone());
                } else {
                    self.remove_cached_binary(&cache_key);
                }

                let refreshed = refreshed.to_response(&requested_url, CacheStatus::Revalidated);
                ensure_binary_response_limit(&refreshed, effective_body_limit)?;
                return Ok(refreshed);
            }

            let reader = response
                .body_mut()
                .with_config()
                .limit(effective_body_limit)
                .reader();
            let body: Arc<[u8]> = read_decoded_body_bounded(reader, effective_body_limit)?.into();

            let binary_response = BinaryResponse {
                requested_url: requested_url.clone(),
                final_url: final_url.as_str().to_owned(),
                status,
                content_type: content_type.clone(),
                body: body.clone(),
                cache_status: CacheStatus::Miss,
            };

            if status == 200 {
                let validators = CacheValidators {
                    etag,
                    last_modified,
                };
                let policy = cache_policy(
                    cache_control.as_deref(),
                    age.as_deref(),
                    vary.as_deref(),
                    validators.has_any(),
                );

                if policy.store {
                    self.store_cached_binary(
                        cache_key.clone(),
                        CachedBinaryResponse {
                            final_url: binary_response.final_url.clone(),
                            status,
                            content_type,
                            body,
                            validators,
                            policy,
                            stored_at: Instant::now(),
                            last_used: 0,
                        },
                    );
                } else {
                    self.remove_cached_binary(&cache_key);
                }
            }

            return Ok(binary_response);
        }

        Err(NetworkError::Request(
            "binary request exhausted retry budget".to_owned(),
        ))
    }

    fn cached_text(&self, key: &PartitionedCacheKey) -> Option<CachedTextResponse> {
        let Ok(mut cache) = self.text_cache.lock() else {
            return None;
        };

        cache.get(key)
    }

    fn store_cached_text(&self, key: PartitionedCacheKey, entry: CachedTextResponse) {
        let Ok(mut cache) = self.text_cache.lock() else {
            return;
        };

        cache.insert(key, entry);
    }

    fn remove_cached_text(&self, key: &PartitionedCacheKey) {
        let Ok(mut cache) = self.text_cache.lock() else {
            return;
        };

        cache.remove(key);
    }

    fn cached_binary(&self, key: &PartitionedCacheKey) -> Option<CachedBinaryResponse> {
        let Ok(mut cache) = self.binary_cache.lock() else {
            return None;
        };

        cache.get(key)
    }

    fn store_cached_binary(&self, key: PartitionedCacheKey, entry: CachedBinaryResponse) {
        let Ok(mut cache) = self.binary_cache.lock() else {
            return;
        };

        cache.insert(key, entry);
    }

    fn remove_cached_binary(&self, key: &PartitionedCacheKey) {
        let Ok(mut cache) = self.binary_cache.lock() else {
            return;
        };

        cache.remove(key);
    }
}

fn ensure_binary_response_limit(
    response: &BinaryResponse,
    max_body_bytes: u64,
) -> Result<(), NetworkError> {
    let Ok(body_length) = u64::try_from(response.body().len()) else {
        return Err(NetworkError::DecodedBodyLimitExceeded {
            limit: max_body_bytes,
        });
    };

    if body_length > max_body_bytes {
        return Err(NetworkError::DecodedBodyLimitExceeded {
            limit: max_body_bytes,
        });
    }

    Ok(())
}

fn stale_recovery(
    cached: Option<&CachedBinaryResponse>,
    requested_url: &str,
) -> Option<BinaryResponse> {
    let entry = cached?;
    if entry
        .policy
        .permits_stale_if_error(entry.stored_at, Instant::now())
    {
        Some(entry.to_response(requested_url, CacheStatus::StaleIfError))
    } else {
        None
    }
}

fn is_transient_status(status: u16) -> bool {
    matches!(status, 408 | 500 | 502 | 503 | 504)
}

fn is_recoverable_status(status: u16) -> bool {
    matches!(status, 500 | 502 | 503 | 504)
}

fn cache_policy(
    cache_control: Option<&str>,
    age: Option<&str>,
    vary: Option<&str>,
    has_validator: bool,
) -> CachePolicy {
    let mut no_store = false;
    let mut no_cache = false;
    let mut must_revalidate = false;
    let mut max_age_seconds = None;
    let mut stale_if_error_seconds = None;

    if let Some(value) = cache_control {
        for raw_directive in value.split(',') {
            let directive = raw_directive.trim();
            let mut parts = directive.splitn(2, '=');
            let name = parts.next().unwrap_or_default().trim().to_ascii_lowercase();
            let parameter = parts.next().map(str::trim).map(trim_http_quotes);

            match name.as_str() {
                "no-store" => no_store = true,
                "no-cache" => no_cache = true,
                "must-revalidate" => must_revalidate = true,
                "max-age" => {
                    max_age_seconds = parameter.and_then(|value| value.parse::<u64>().ok());
                }
                "stale-if-error" => {
                    stale_if_error_seconds = parameter.and_then(|value| value.parse::<u64>().ok());
                }
                _ => {}
            }
        }
    }

    let age_seconds = age
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let fresh_seconds = max_age_seconds.unwrap_or(0).saturating_sub(age_seconds);
    let vary_supported = vary_allows_fixed_accept_request(vary);
    let has_explicit_cache_semantics =
        max_age_seconds.is_some() || stale_if_error_seconds.is_some() || no_cache || has_validator;

    CachePolicy {
        store: !no_store && vary_supported && has_explicit_cache_semantics,
        revalidate_always: no_cache,
        fresh_for: Duration::from_secs(fresh_seconds),
        stale_if_error_for: stale_if_error_seconds.map(Duration::from_secs),
        must_revalidate,
    }
}

fn trim_http_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

fn vary_allows_fixed_accept_request(vary: Option<&str>) -> bool {
    let Some(value) = vary else {
        return true;
    };

    value.split(',').all(|raw_name| {
        let name = raw_name.trim();
        !name.is_empty()
            && name != "*"
            && (name.eq_ignore_ascii_case("accept") || name.eq_ignore_ascii_case("origin"))
    })
}

fn admit_document(response: TextResponse) -> Result<DocumentResponse, DocumentLoadError> {
    let requested_url = HttpUrl::parse(response.requested_url()).map_err(NetworkError::from)?;
    let final_url = HttpUrl::parse(response.final_url()).map_err(NetworkError::from)?;

    match response.status() {
        204 | 205 => {
            return Err(DocumentLoadError::NoContent {
                status: response.status(),
            });
        }

        206 => return Err(DocumentLoadError::PartialContent),

        _ => {}
    }

    let media_type = match response.content_type() {
        Some(value) => {
            let normalized = normalized_media_type(value);

            match normalized.as_str() {
                "text/html" => DocumentMediaType::Html,
                "application/xhtml+xml" => DocumentMediaType::Xhtml,
                "" => {
                    if looks_like_html_document(response.body()) {
                        DocumentMediaType::Html
                    } else {
                        return Err(DocumentLoadError::UnidentifiedMediaType);
                    }
                }
                other => {
                    return Err(DocumentLoadError::UnsupportedMediaType(other.to_owned()));
                }
            }
        }

        None => {
            if looks_like_html_document(response.body()) {
                DocumentMediaType::Html
            } else {
                return Err(DocumentLoadError::UnidentifiedMediaType);
            }
        }
    };

    Ok(DocumentResponse {
        response,
        requested_url,
        final_url,
        media_type,
    })
}

fn normalized_media_type(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn declared_charset(content_type: Option<&str>) -> Option<String> {
    let value = content_type?;

    value
        .split(';')
        .skip(1)
        .filter_map(|parameter| parameter.split_once('='))
        .find_map(|(name, raw_value)| {
            name.trim().eq_ignore_ascii_case("charset").then(|| {
                raw_value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_ascii_lowercase()
            })
        })
        .filter(|value| !value.is_empty())
}

// PHANTOM_2H1_DOCUMENT_ENCODING_I
fn is_windows_1252_label(label: &str) -> bool {
    matches!(
        label,
        "windows-1252" | "cp1252" | "x-cp1252" | "iso-8859-1" | "latin1" | "latin-1"
    )
}

fn decode_windows_1252(payload: &[u8]) -> Arc<str> {
    let mut decoded = String::with_capacity(payload.len());

    for &byte in payload {
        let character = match byte {
            0x80 => '\u{20AC}',
            0x82 => '\u{201A}',
            0x83 => '\u{0192}',
            0x84 => '\u{201E}',
            0x85 => '\u{2026}',
            0x86 => '\u{2020}',
            0x87 => '\u{2021}',
            0x88 => '\u{02C6}',
            0x89 => '\u{2030}',
            0x8A => '\u{0160}',
            0x8B => '\u{2039}',
            0x8C => '\u{0152}',
            0x8E => '\u{017D}',
            0x91 => '\u{2018}',
            0x92 => '\u{2019}',
            0x93 => '\u{201C}',
            0x94 => '\u{201D}',
            0x95 => '\u{2022}',
            0x96 => '\u{2013}',
            0x97 => '\u{2014}',
            0x98 => '\u{02DC}',
            0x99 => '\u{2122}',
            0x9A => '\u{0161}',
            0x9B => '\u{203A}',
            0x9C => '\u{0153}',
            0x9E => '\u{017E}',
            0x9F => '\u{0178}',
            _ => char::from(byte),
        };
        decoded.push(character);
    }

    Arc::<str>::from(decoded)
}

fn decode_text_body(bytes: &[u8], content_type: Option<&str>) -> Result<Arc<str>, NetworkError> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return Err(NetworkError::UnsupportedCharset("utf-16le".to_owned()));
    }

    if bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(NetworkError::UnsupportedCharset("utf-16be".to_owned()));
    }

    let has_utf8_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let payload = if has_utf8_bom { &bytes[3..] } else { bytes };

    let charset = if has_utf8_bom {
        None
    } else {
        declared_charset(content_type)
    };

    match charset.as_deref() {
        None | Some("utf-8") | Some("utf8") => {}

        Some("us-ascii") | Some("ascii") => {
            if payload.iter().any(|byte| !byte.is_ascii()) {
                return Err(NetworkError::InvalidTextEncoding);
            }
        }

        Some(label) if is_windows_1252_label(label) => {
            return Ok(decode_windows_1252(payload));
        }

        Some(other) => {
            return Err(NetworkError::UnsupportedCharset(other.to_owned()));
        }
    }

    let decoded = std::str::from_utf8(payload).map_err(|_| NetworkError::InvalidTextEncoding)?;

    Ok(Arc::<str>::from(decoded))
}

fn looks_like_html_document(body: &str) -> bool {
    let sample = body
        .trim_start_matches(char::is_whitespace)
        .chars()
        .take(1024)
        .collect::<String>()
        .to_ascii_lowercase();

    sample.starts_with("<!doctype html")
        || sample.starts_with("<html")
        || sample.starts_with("<head")
        || sample.starts_with("<body")
        || sample.contains("<html")
}
fn redirect_visit_key(url: &HttpUrl) -> String {
    let mut normalized = url.0.clone();
    normalized.set_fragment(None);
    normalized.as_str().to_owned()
}

fn is_document_redirect_status(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
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

    /// The decoded/decompressed body exceeded Phantom's explicit output budget.
    #[error("decoded response body exceeds configured limit ({limit} bytes)")]
    DecodedBodyLimitExceeded {
        /// Maximum decoded bytes accepted for this response.
        limit: u64,
    },

    /// A secure top-level document attempted to load an insecure HTTP resource.
    #[error("mixed content blocked: {resource_url}")]
    MixedContentBlocked {
        /// Insecure resource URL that was rejected.
        resource_url: String,
    },

    /// A public navigation context attempted a request to a local/private target.
    #[error("private-network request blocked: {resource_url}")]
    PrivateNetworkBlocked {
        /// Local/private resource URL that was rejected before transport.
        resource_url: String,
    },

    /// A page-script fetch attempted to leave its document origin.
    #[error("cross-origin script fetch blocked: {resource_url}")]
    CrossOriginBlocked {
        /// URL rejected before a cross-origin network request.
        resource_url: String,
    },

    /// A cross-origin redirect response failed CORS admission before Phantom
    /// followed its `Location`.
    #[error("CORS redirect blocked before follow: {resource_url}")]
    CorsRedirectBlocked {
        /// Redirecting cross-origin URL whose response did not authorize the
        /// document origin.
        resource_url: String,
    },

    /// A redirect response omitted a usable `Location` header.
    #[error("HTTP {status} redirect is missing a usable Location header")]
    RedirectMissingLocation {
        /// Redirect status code that omitted the target.
        status: u16,
    },

    /// A redirect chain revisited a URL already seen in the same navigation.
    #[error("redirect loop detected at {0}")]
    RedirectLoop(String),

    /// A redirect chain exceeded Phantom's bounded navigation policy.
    #[error("redirect limit exceeded ({limit} hops)")]
    RedirectLimitExceeded {
        /// Maximum number of redirect hops allowed for one document fetch.
        limit: usize,
    },

    /// A 304 response arrived without a cached representation to validate.
    #[error("received HTTP 304 without a cached representation")]
    UnexpectedNotModified,

    /// The response declares a text charset not supported by the current
    /// bounded document decoder.
    #[error("unsupported document text charset: {0}")]
    UnsupportedCharset(String),

    /// The response bytes are not valid under the admitted UTF-8/ASCII policy.
    #[error("document text is not valid UTF-8/ASCII")]
    InvalidTextEncoding,

    /// The supplied or redirected URL violates Phantom HTTP(S) policy.
    #[error(transparent)]
    Url(#[from] UrlError),
}

#[cfg(test)]
mod tests {
    use super::{
        CachePolicy, CacheStatus, CorsSafelistedResponseHeaders, HttpUrl, NetworkClient,
        NetworkIsolationKey, OriginScheme, TextResponse, UrlError, admit_document, cache_policy,
        enforce_navigation_redirect_policy,
        host_is_private_namespace, is_public_network_ip, navigation_allows_private_network,
        trim_http_quotes, vary_allows_fixed_accept_request,
        CookieSiteContext,
        CookieStoreOutcome,
        CredentialsMode,
        CorsBodyMethod,
        CorsRequestClassification,
        MAX_CORS_REQUEST_BODY_BYTES,
        classify_cors_request,
        comma_list_contains_token_for_credentials,
        cors_credentialed_response_admits,
        cors_preflight_allows_headers_for_credentials,
    };
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    // PHANTOM_2N20_FINAL_SECURITY_INTEGRATION_GATE
    #[test]
    fn integrated_final_origin_cors_matrix_2n20()
    -> Result<(), Box<dyn std::error::Error>> {
        let page = HttpUrl::parse("https://app.example.test/index.html")?;
        let request_origin = page.origin();
        for target in [
            "https://api.example.test/item",
            "https://app.example.test:8443/item",
            "http://app.example.test/item",
        ] {
            let final_url = HttpUrl::parse(target)?;
            assert!(super::cors_final_origin_needs_admission(&page, &final_url));
            assert!(!super::cors_allow_origin_admits(&request_origin, None));
            assert!(!super::cors_credentialed_response_admits(
                &request_origin, Some("*"), Some("true"),
            ));
            assert!(!super::cors_credentialed_response_admits(
                &request_origin, Some("https://app.example.test"), None,
            ));
        }
        let same = HttpUrl::parse("https://app.example.test/other")?;
        assert!(!super::cors_final_origin_needs_admission(&page, &same));
        assert!(super::cors_credentialed_response_admits(
            &request_origin, Some("https://app.example.test"), Some("true"),
        ));
        Ok(())
    }

    #[test]
    fn integrated_response_origin_isolation_2n20()
    -> Result<(), Box<dyn std::error::Error>> {
        let page = HttpUrl::parse("https://app.example.test/")?;
        let original = page.origin();
        let other = HttpUrl::parse("https://other.example.test/")?.origin();
        assert!(!super::cors_allow_origin_admits(&original, Some("https://other.example.test")));
        assert!(!super::cors_credentialed_response_admits(
            &original, Some("https://other.example.test"), Some("true"),
        ));
        assert!(!super::cors_credentialed_response_admits(
            &other, Some("https://app.example.test"), Some("true"),
        ));
        assert!(super::cors_allow_origin_admits(&original, Some("https://app.example.test")));
        Ok(())
    }

    // PHANTOM_2N19_FINAL_ORIGIN_CORS_HARDENING
    #[test]
    fn final_origin_requires_admission_even_after_same_origin_start()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = HttpUrl::parse("https://app.example.test/page")?;
        let same = HttpUrl::parse("https://app.example.test/redirect")?;
        let different = HttpUrl::parse("https://api.example.test/data")?;
        let downgraded = HttpUrl::parse("http://app.example.test/data")?;
        assert!(!super::cors_final_origin_needs_admission(&document, &same));
        assert!(super::cors_final_origin_needs_admission(&document, &different));
        assert!(super::cors_final_origin_needs_admission(&document, &downgraded));
        let request_origin = document.origin();
        assert!(!super::cors_allow_origin_admits(&request_origin, None));
        assert!(super::cors_allow_origin_admits(
            &request_origin,
            Some("https://app.example.test")
        ));
        assert!(!super::cors_credentialed_response_admits(
            &request_origin,
            Some("*"),
            Some("true")
        ));
        Ok(())
    }

    #[test]
    fn classic_script_policy_allows_public_cdn_but_blocks_private_and_mixed()
    -> Result<(), Box<dyn std::error::Error>> {
        let client = NetworkClient::with_max_body_bytes(1024 * 1024);
        let secure_document = HttpUrl::parse("https://example.test/page")?;
        let key = NetworkIsolationKey::from_top_level(&secure_document);
        let cdn = HttpUrl::parse("https://cdn.example.test/app.js")?;
        assert!(super::enforce_subresource_policy(&key, &cdn).is_ok());
        let mixed = client.fetch_text_classic_script("http://cdn.example.test/app.js", &secure_document);
        assert!(matches!(mixed, Err(super::NetworkError::MixedContentBlocked { .. })));
        let public_document = HttpUrl::parse("http://example.test/page")?;
        let private = client.fetch_text_classic_script("http://127.0.0.1/app.js", &public_document);
        assert!(matches!(private, Err(super::NetworkError::PrivateNetworkBlocked { .. })));
        Ok(())
    }

    // PHANTOM_2N15_NON_SIMPLE_METHOD_BODY_TRANSPORT_I
    #[test]
    fn non_simple_body_transport_methods_are_explicit_bounded_and_preflight_classified()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(CorsBodyMethod::Post.as_str(), "POST");
        assert_eq!(CorsBodyMethod::Put.as_str(), "PUT");
        assert_eq!(CorsBodyMethod::Patch.as_str(), "PATCH");

        assert_eq!(
            classify_cors_request(
                CorsBodyMethod::Post.as_str(),
                &[("Content-Type", "text/plain")],
            ),
            CorsRequestClassification::Simple
        );
        assert_eq!(
            classify_cors_request(
                CorsBodyMethod::Post.as_str(),
                &[("Content-Type", "application/json")],
            ),
            CorsRequestClassification::PreflightRequired
        );
        assert_eq!(
            classify_cors_request(CorsBodyMethod::Put.as_str(), &[]),
            CorsRequestClassification::PreflightRequired
        );
        assert_eq!(
            classify_cors_request(CorsBodyMethod::Patch.as_str(), &[("X-Phantom", "1")]),
            CorsRequestClassification::PreflightRequired
        );

        let document = HttpUrl::parse("https://app.example.test/page")?;
        let client = NetworkClient::new();
        let oversized = vec![b'x'; MAX_CORS_REQUEST_BODY_BYTES + 1];
        assert!(client
            .fetch_text_cors_with_body(
                "https://api.example.test/items",
                &document,
                CorsBodyMethod::Post,
                &[("Content-Type", "application/json")],
                &oversized,
            )
            .is_err());
        Ok(())
    }

    // PHANTOM_2N14_CREDENTIALED_CORS_TRANSPORT_I
    #[test]
    fn credentialed_cors_requires_exact_origin_true_and_no_wildcards()
    -> Result<(), Box<dyn std::error::Error>> {
        let origin = HttpUrl::parse("https://app.example.test/page")?.origin();

        assert!(cors_credentialed_response_admits(
            &origin,
            Some("https://app.example.test"),
            Some("true"),
        ));
        assert!(!cors_credentialed_response_admits(&origin, Some("*"), Some("true")));
        assert!(!cors_credentialed_response_admits(
            &origin,
            Some("https://app.example.test"),
            Some("True"),
        ));
        assert!(!cors_credentialed_response_admits(
            &origin,
            Some("https://app.example.test"),
            None,
        ));

        assert!(!comma_list_contains_token_for_credentials(
            Some("*"),
            "PUT",
            CredentialsMode::Include,
        ));
        assert!(comma_list_contains_token_for_credentials(
            Some("PUT"),
            "PUT",
            CredentialsMode::Include,
        ));

        let required = vec!["x-phantom".to_owned()];
        assert!(!cors_preflight_allows_headers_for_credentials(
            Some("*"),
            &required,
            CredentialsMode::Include,
        ));
        assert!(cors_preflight_allows_headers_for_credentials(
            Some("x-phantom"),
            &required,
            CredentialsMode::Include,
        ));
        Ok(())
    }

    // PHANTOM_2N13_CREDENTIALS_COOKIE_FOUNDATION_I
    #[test]
    fn credentials_modes_gate_request_and_response_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = HttpUrl::parse("https://app.example.test/page")?;
        let same_origin = HttpUrl::parse("https://app.example.test/api")?;
        let cross_origin = HttpUrl::parse("https://api.example.test/data")?;

        assert!(!CredentialsMode::Omit.includes_request_credentials(&document, &same_origin));
        assert!(CredentialsMode::SameOrigin.includes_request_credentials(&document, &same_origin));
        assert!(!CredentialsMode::SameOrigin.includes_request_credentials(&document, &cross_origin));
        assert!(CredentialsMode::Include.includes_request_credentials(&document, &cross_origin));

        assert!(!CredentialsMode::Omit.accepts_response_credentials(&document, &same_origin));
        assert!(CredentialsMode::SameOrigin.accepts_response_credentials(&document, &same_origin));
        assert!(!CredentialsMode::SameOrigin.accepts_response_credentials(&document, &cross_origin));
        assert!(CredentialsMode::Include.accepts_response_credentials(&document, &cross_origin));
        Ok(())
    }

    #[test]
    fn cookie_store_is_bounded_host_only_secure_partitioned_and_path_scoped()
    -> Result<(), Box<dyn std::error::Error>> {
        let client = NetworkClient::with_cookie_store_limit(4);
        let document_a = HttpUrl::parse("https://app.example.test/page")?;
        let document_b = HttpUrl::parse("https://other.example.test/page")?;
        let response = HttpUrl::parse("https://api.example.test/api/login")?;
        let request = HttpUrl::parse("https://api.example.test/api/data")?;

        assert_eq!(
            client.store_set_cookie(
                &document_a,
                &response,
                CredentialsMode::Include,
                "sid=abc; Path=/api; Secure; HttpOnly; SameSite=None; Max-Age=60",
            )?,
            CookieStoreOutcome::Stored
        );
        assert_eq!(
            client
                .cookie_header_for_request(
                    &document_a,
                    &request,
                    CredentialsMode::Include,
                    CookieSiteContext::CrossSite,
                )?
                .as_deref(),
            Some("sid=abc")
        );
        assert_eq!(
            client.cookie_header_for_request(
                &document_b,
                &request,
                CredentialsMode::Include,
                CookieSiteContext::CrossSite,
            )?,
            None
        );
        assert_eq!(
            client.cookie_header_for_request(
                &document_a,
                &request,
                CredentialsMode::SameOrigin,
                CookieSiteContext::CrossSite,
            )?,
            None
        );

        assert_eq!(
            client.store_set_cookie(
                &document_a,
                &response,
                CredentialsMode::Include,
                "domaincookie=x; Domain=example.test; Secure",
            )?,
            CookieStoreOutcome::Ignored
        );

        let insecure_document = HttpUrl::parse("http://app.example.test/page")?;
        let insecure_response = HttpUrl::parse("http://api.example.test/api/login")?;
        assert_eq!(
            client.store_set_cookie(
                &insecure_document,
                &insecure_response,
                CredentialsMode::Include,
                "securecookie=x; Secure",
            )?,
            CookieStoreOutcome::Ignored
        );

        assert_eq!(
            client.store_set_cookie(
                &document_a,
                &response,
                CredentialsMode::Include,
                "lax=1; Path=/api; Secure; SameSite=Lax",
            )?,
            CookieStoreOutcome::Stored
        );
        assert_eq!(
            client
                .cookie_header_for_request(
                    &document_a,
                    &request,
                    CredentialsMode::Include,
                    CookieSiteContext::CrossSite,
                )?
                .as_deref(),
            Some("sid=abc")
        );

        assert_eq!(
            client.store_set_cookie(
                &document_a,
                &response,
                CredentialsMode::Include,
                "third=3; Path=/; Secure; SameSite=None",
            )?,
            CookieStoreOutcome::Stored
        );

        let header = client
            .cookie_header_for_request(
                &document_a,
                &request,
                CredentialsMode::Include,
                CookieSiteContext::SameSite,
            )?
            .ok_or(std::io::Error::other("expected cookie header"))?;
        assert!(header.contains("sid=abc"));
        assert!(header.contains("lax=1"));
        assert!(header.contains("third=3"));

        assert_eq!(
            client.store_set_cookie(
                &document_a,
                &response,
                CredentialsMode::Include,
                "lax=gone; Path=/api; Secure; SameSite=Lax; Max-Age=0",
            )?,
            CookieStoreOutcome::Deleted
        );
        let header = client
            .cookie_header_for_request(
                &document_a,
                &request,
                CredentialsMode::Include,
                CookieSiteContext::SameSite,
            )?
            .ok_or(std::io::Error::other("expected cookie header after deletion"))?;
        assert!(!header.contains("lax="));

        let bounded = NetworkClient::with_cookie_store_limit(2);
        assert_eq!(
            bounded.store_set_cookie(
                &document_a,
                &response,
                CredentialsMode::Include,
                "one=1; Path=/; Secure; SameSite=None",
            )?,
            CookieStoreOutcome::Stored
        );
        assert_eq!(
            bounded.store_set_cookie(
                &document_a,
                &response,
                CredentialsMode::Include,
                "two=2; Path=/; Secure; SameSite=None",
            )?,
            CookieStoreOutcome::Stored
        );
        assert_eq!(
            bounded.store_set_cookie(
                &document_a,
                &response,
                CredentialsMode::Include,
                "three=3; Path=/; Secure; SameSite=None",
            )?,
            CookieStoreOutcome::Stored
        );
        let bounded_header = bounded
            .cookie_header_for_request(
                &document_a,
                &request,
                CredentialsMode::Include,
                CookieSiteContext::CrossSite,
            )?
            .ok_or(std::io::Error::other("expected bounded cookie header"))?;
        assert!(!bounded_header.contains("one=1"));
        assert!(bounded_header.contains("two=2"));
        assert!(bounded_header.contains("three=3"));
        Ok(())
    }

    // PHANTOM_2N12_PREFLIGHT_METHODS_HEADERS_ADMISSION_II
    #[test]
    fn cors_preflight_header_wildcard_excludes_authorization_and_transport_is_bounded() {
        let authorization = ["authorization".to_owned()];
        assert!(!super::cors_preflight_allows_headers(
            Some("*"),
            &authorization,
        ));
        assert!(super::cors_preflight_allows_headers(
            Some("*, authorization"),
            &authorization,
        ));

        assert!(super::cors_author_headers_transport_safe(&[
            ("X-Phantom", "ok"),
            ("Accept", "text/plain"),
        ]));
        assert!(!super::cors_author_headers_transport_safe(&[
            ("Bad Header", "x"),
        ]));
        assert!(!super::cors_author_headers_transport_safe(&[
            ("X-Phantom", "ok\r\nInjected: yes"),
        ]));
    }

    // PHANTOM_2N11_CORS_PREFLIGHT_FOUNDATION_I
    #[test]
    fn cors_preflight_helpers_require_explicit_method_and_header_admission() {
        let unsafe_headers = super::cors_unsafe_request_header_names(&[
            ("Content-Type", "application/json"),
            ("X-Phantom", "1"),
            ("x-phantom", "2"),
        ]);
        assert_eq!(
            unsafe_headers,
            vec!["content-type".to_owned(), "x-phantom".to_owned()]
        );

        assert!(super::comma_list_contains_token(Some("GET, PUT"), "PUT"));
        assert!(super::comma_list_contains_token(Some("*"), "PATCH"));
        assert!(!super::comma_list_contains_token(Some("GET, POST"), "DELETE"));

        assert!(super::cors_preflight_allows_headers(
            Some("Content-Type, X-Phantom"),
            &unsafe_headers,
        ));
        assert!(super::cors_preflight_allows_headers(
            Some("*"),
            &unsafe_headers,
        ));
        assert!(!super::cors_preflight_allows_headers(
            Some("Content-Type"),
            &unsafe_headers,
        ));
    }

    // PHANTOM_2N10_SIMPLE_REQUEST_MATRIX_I
    #[test]
    fn cors_simple_request_matrix_matches_method_header_and_budget_boundaries() {
        use super::{CorsRequestClassification, classify_cors_request};

        assert_eq!(
            classify_cors_request("GET", &[]),
            CorsRequestClassification::Simple
        );
        assert_eq!(
            classify_cors_request("HEAD", &[("Accept", "text/plain")]),
            CorsRequestClassification::Simple
        );
        assert_eq!(
            classify_cors_request(
                "POST",
                &[
                    ("Content-Type", "text/plain; charset=utf-8"),
                    ("Accept-Language", "pt-BR,en;q=0.8"),
                ],
            ),
            CorsRequestClassification::Simple
        );
        assert_eq!(
            classify_cors_request("GET", &[("Range", "bytes=0-499")]),
            CorsRequestClassification::Simple
        );
        assert_eq!(
            classify_cors_request("GET", &[("Range", "bytes=0-")]),
            CorsRequestClassification::Simple
        );

        assert_eq!(
            classify_cors_request("PUT", &[]),
            CorsRequestClassification::PreflightRequired
        );
        assert_eq!(
            classify_cors_request("POST", &[("Content-Type", "application/json")]),
            CorsRequestClassification::PreflightRequired
        );
        assert_eq!(
            classify_cors_request("GET", &[("Authorization", "Bearer token")]),
            CorsRequestClassification::PreflightRequired
        );
        assert_eq!(
            classify_cors_request("GET", &[("X-Phantom", "1")]),
            CorsRequestClassification::PreflightRequired
        );
        assert_eq!(
            classify_cors_request("GET", &[("Accept", "text/plain:unsafe")]),
            CorsRequestClassification::PreflightRequired
        );
        assert_eq!(
            classify_cors_request("GET", &[("Accept-Language", "pt_BR")]),
            CorsRequestClassification::PreflightRequired
        );
        assert_eq!(
            classify_cors_request("GET", &[("Range", "bytes=-500")]),
            CorsRequestClassification::PreflightRequired
        );
        assert_eq!(
            classify_cors_request("GET", &[("Range", "bytes=500-10")]),
            CorsRequestClassification::PreflightRequired
        );

        let oversized = "x".repeat(129);
        assert_eq!(
            classify_cors_request("GET", &[("Accept", oversized.as_str())]),
            CorsRequestClassification::PreflightRequired
        );

        let aggregate = "a".repeat(128);
        let repeated = [
            ("Accept", aggregate.as_str()),
            ("Accept", aggregate.as_str()),
            ("Accept", aggregate.as_str()),
            ("Accept", aggregate.as_str()),
            ("Accept", aggregate.as_str()),
            ("Accept", aggregate.as_str()),
            ("Accept", aggregate.as_str()),
            ("Accept", aggregate.as_str()),
            ("Accept", aggregate.as_str()),
        ];
        assert_eq!(
            classify_cors_request("GET", &repeated),
            CorsRequestClassification::PreflightRequired
        );

        assert_eq!(
            classify_cors_request("TRACE", &[]),
            CorsRequestClassification::Forbidden
        );
        assert_eq!(
            classify_cors_request("GET", &[("Cookie", "session=secret")]),
            CorsRequestClassification::Forbidden
        );
        assert_eq!(
            classify_cors_request("GET", &[("Origin", "https://evil.test")]),
            CorsRequestClassification::Forbidden
        );
        assert_eq!(
            classify_cors_request("GET", &[("Sec-Fetch-Site", "cross-site")]),
            CorsRequestClassification::Forbidden
        );
    }

    #[test]
    fn cors_allow_origin_admission_is_single_value_and_fail_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = HttpUrl::parse("https://example.test/page")?;
        let origin = document.origin();

        assert!(super::cors_allow_origin_admits(&origin, Some("*")));
        assert!(super::cors_allow_origin_admits(
            &origin,
            Some("https://example.test")
        ));
        assert!(!super::cors_allow_origin_admits(&origin, None));
        assert!(!super::cors_allow_origin_admits(&origin, Some("")));
        assert!(!super::cors_allow_origin_admits(&origin, Some("null")));
        assert!(!super::cors_allow_origin_admits(
            &origin,
            Some("https://other.test")
        ));
        assert!(!super::cors_allow_origin_admits(
            &origin,
            Some("https://example.test, https://other.test")
        ));
        Ok(())
    }

    #[test]
    fn cors_safelisted_response_headers_are_fixed_bounded_and_case_insensitive()
    -> Result<(), Box<dyn std::error::Error>> {
        use ureq::http::HeaderValue;

        let mut headers = ureq::http::HeaderMap::new();
        headers.insert("cache-control", HeaderValue::from_static("no-store"));
        headers.insert("content-language", HeaderValue::from_static("pt-BR"));
        headers.insert("content-length", HeaderValue::from_static("12"));
        headers.insert("content-type", HeaderValue::from_static("text/plain"));
        headers.insert(
            "expires",
            HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT"),
        );
        headers.insert(
            "last-modified",
            HeaderValue::from_static("Tue, 20 Oct 2026 07:28:00 GMT"),
        );
        headers.insert("pragma", HeaderValue::from_static("no-cache"));
        headers.insert("set-cookie", HeaderValue::from_static("secret=1"));
        headers.insert("x-phantom-secret", HeaderValue::from_static("hidden"));

        let admitted = super::cors_safelisted_response_headers(&headers);
        assert_eq!(admitted.len(), 7);
        assert_eq!(admitted.get("Content-Language"), Some("pt-BR"));
        assert_eq!(admitted.get("CONTENT-TYPE"), Some("text/plain"));
        assert_eq!(admitted.get("set-cookie"), None);
        assert_eq!(admitted.get("x-phantom-secret"), None);

        let oversized = "x".repeat(
            super::MAX_CORS_SAFELISTED_RESPONSE_HEADER_VALUE_BYTES.saturating_add(1),
        );
        headers.insert("pragma", oversized.parse::<HeaderValue>()?);
        let bounded = super::cors_safelisted_response_headers(&headers);
        assert_eq!(bounded.get("pragma"), None);
        Ok(())
    }

    #[test]
    fn same_origin_fetch_rejects_external_url_before_network()
    -> Result<(), Box<dyn std::error::Error>> {
        let client = NetworkClient::with_max_body_bytes(1024 * 1024);
        let document = HttpUrl::parse("https://example.test/page")?;
        let result = client.fetch_text_same_origin("https://other.test/data", &document);
        assert!(matches!(result, Err(super::NetworkError::CrossOriginBlocked { .. })));
        Ok(())
    }

    #[test]
    fn public_network_classifier_rejects_local_ranges() {
        for address in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "fec0::1",
            "2001:db8::1",
            "::127.0.0.1",
            "::ffff:127.0.0.1",
        ] {
            let parsed = address.parse().ok();
            assert!(parsed.is_some_and(|ip| !is_public_network_ip(ip)));
        }

        for address in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            let parsed = address.parse().ok();
            assert!(parsed.is_some_and(is_public_network_ip));
        }
    }

    #[test]
    fn private_host_namespaces_are_explicit() {
        assert!(host_is_private_namespace("localhost"));
        assert!(host_is_private_namespace("dev.localhost"));
        assert!(host_is_private_namespace("router.local"));
        assert!(host_is_private_namespace("home.arpa"));
        assert!(host_is_private_namespace("printer.home.arpa"));
        assert!(host_is_private_namespace("router"));
        assert!(host_is_private_namespace("192.168.1.1"));
        assert!(!host_is_private_namespace("example.com"));
    }

    #[test]
    fn public_navigation_cannot_redirect_to_explicit_private_target() -> Result<(), UrlError> {
        let public = HttpUrl::parse("https://example.com/start")?;
        let private = HttpUrl::parse("http://127.0.0.1/admin")?;

        assert!(!navigation_allows_private_network(&public));
        assert!(enforce_navigation_redirect_policy(false, &private).is_err());
        assert!(enforce_navigation_redirect_policy(true, &private).is_ok());

        Ok(())
    }

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
    fn network_isolation_key_is_origin_scoped_and_double_keyed() -> Result<(), UrlError> {
        let top = HttpUrl::parse("https://www.example.com:443/page")?;
        let frame = HttpUrl::parse("https://embed.example.net:8443/frame")?;
        let key = NetworkIsolationKey::new(&top, &frame);

        assert_eq!(key.top_level_origin(), "https://www.example.com");
        assert_eq!(key.frame_origin(), "https://embed.example.net:8443");

        Ok(())
    }

    #[test]
    fn top_level_isolation_key_uses_same_origin_for_both_dimensions() -> Result<(), UrlError> {
        let top = HttpUrl::parse("https://example.com/page")?;
        let key = NetworkIsolationKey::from_top_level(&top);

        assert_eq!(key.top_level_origin(), "https://example.com");
        assert_eq!(key.frame_origin(), "https://example.com");

        Ok(())
    }

    #[test]
    fn typed_origin_normalizes_default_port_and_host_case() -> Result<(), UrlError> {
        let explicit = HttpUrl::parse("https://EXAMPLE.com:443/path")?;
        let implicit = HttpUrl::parse("https://example.com/other")?;
        let origin = explicit.origin();

        assert!(explicit.same_origin(&implicit));
        assert_eq!(origin.scheme(), OriginScheme::Https);
        assert_eq!(origin.host(), "example.com");
        assert_eq!(origin.effective_port(), 443);
        assert_eq!(origin.as_str(), "https://example.com");

        Ok(())
    }

    #[test]
    fn typed_origin_distinguishes_scheme_and_non_default_port() -> Result<(), UrlError> {
        let secure = HttpUrl::parse("https://example.com/")?;
        let insecure = HttpUrl::parse("http://example.com/")?;
        let alternate_port = HttpUrl::parse("https://example.com:444/")?;

        assert!(!secure.same_origin(&insecure));
        assert!(!secure.same_origin(&alternate_port));
        assert_eq!(alternate_port.origin().effective_port(), 444);

        Ok(())
    }

    #[test]
    fn document_response_keeps_typed_requested_and_final_urls()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = TextResponse {
            requested_url: "https://example.com/request".to_owned(),
            final_url: "https://www.example.com/final".to_owned(),
            status: 200,
            content_type: Some("text/html; charset=utf-8".to_owned()),
            access_control_allow_origin: None,
            access_control_allow_credentials: None,
            set_cookie_fields: Vec::new(),
            cors_safelisted_response_headers: CorsSafelistedResponseHeaders::default(),
            redirect_count: 1,
            body: Arc::<str>::from("<!doctype html><title>typed</title>"),
            cache_status: CacheStatus::Miss,
        };

        let document = admit_document(response)?;

        assert_eq!(
            document.requested_http_url().as_str(),
            "https://example.com/request"
        );
        assert_eq!(
            document.final_http_url().as_str(),
            "https://www.example.com/final"
        );
        assert_eq!(document.origin().as_str(), "https://www.example.com");

        Ok(())
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

    #[test]
    fn cache_control_parses_revalidation_and_recovery_windows() {
        let policy = cache_policy(
            Some("max-age=120, stale-if-error=45, no-cache"),
            Some("20"),
            Some("Accept"),
            true,
        );

        assert!(policy.store);
        assert!(policy.revalidate_always);
        assert_eq!(policy.fresh_for, Duration::from_secs(100));
        assert_eq!(policy.stale_if_error_for, Some(Duration::from_secs(45)));
    }

    #[test]
    fn must_revalidate_blocks_stale_recovery() {
        let policy = CachePolicy {
            store: true,
            revalidate_always: false,
            fresh_for: Duration::ZERO,
            stale_if_error_for: Some(Duration::from_secs(60)),
            must_revalidate: true,
        };
        let stored_at = Instant::now();

        assert!(!policy.permits_stale_if_error(stored_at, stored_at));
    }

    #[test]
    fn unsupported_vary_is_not_cached() {
        assert!(vary_allows_fixed_accept_request(None));
        assert!(vary_allows_fixed_accept_request(Some("Accept")));
        assert!(vary_allows_fixed_accept_request(Some("Origin")));
        assert!(vary_allows_fixed_accept_request(Some("Accept, Origin")));
        assert!(!vary_allows_fixed_accept_request(Some("*")));
        assert!(!vary_allows_fixed_accept_request(Some("Accept-Encoding")));
        assert!(!vary_allows_fixed_accept_request(Some(
            "Accept, Accept-Encoding"
        )));
    }

    #[test]
    fn quoted_cache_seconds_are_accepted() {
        assert_eq!(trim_http_quotes("\"60\""), "60");
        assert_eq!(trim_http_quotes("60"), "60");
    }
}
