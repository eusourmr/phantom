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
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use thiserror::Error;
use ureq::ResponseExt;
use url::Url;

const DEFAULT_MAX_TEXT_BODY_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_MAX_BINARY_BODY_BYTES: u64 = 16 * 1024 * 1024;
const DEFAULT_MAX_TEXT_CACHE_BYTES: u64 = 16 * 1024 * 1024;
const DEFAULT_MAX_TEXT_CACHE_ENTRIES: usize = 32;
const DEFAULT_MAX_BINARY_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_BINARY_CACHE_ENTRIES: usize = 128;
const MAX_BINARY_FETCH_ATTEMPTS: usize = 2;
const MAX_DOCUMENT_REDIRECTS: usize = 10;
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
/// HTTP response decoded as text for the document pipeline.
#[derive(Clone, Debug)]
pub struct TextResponse {
    requested_url: String,
    final_url: String,
    status: u16,
    content_type: Option<String>,
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
struct PartitionedCacheKey {
    isolation_key: NetworkIsolationKey,
    resource_url: String,
}

impl PartitionedCacheKey {
    fn new(isolation_key: &NetworkIsolationKey, resource_url: &str) -> Self {
        Self {
            isolation_key: isolation_key.clone(),
            resource_url: resource_url.to_owned(),
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

/// Reusable network client used by browser resource coordinators.
///
/// Clones share bounded process-memory document and binary caches. Every
/// reusable entry is partitioned by [`NetworkIsolationKey`] before its URL is
/// considered. Neither cache is persisted to disk.
#[derive(Clone)]
pub struct NetworkClient {
    agent: ureq::Agent,
    max_text_body_bytes: u64,
    max_binary_body_bytes: u64,
    text_cache: Arc<Mutex<TextHttpCache>>,
    binary_cache: Arc<Mutex<BinaryHttpCache>>,
}

impl Default for NetworkClient {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_response_header_size(64 * 1024)
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent("Phantom/0.0.1 (+https://github.com/eusourmr/phantom)")
            .build();

        Self {
            agent: ureq::Agent::new_with_config(config),
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
        let mut current_url = HttpUrl::parse(input)?;
        let requested_url = current_url.as_str().to_owned();
        let mut visited = BTreeSet::new();
        visited.insert(redirect_visit_key(&current_url));
        let mut redirect_count = 0_usize;

        loop {
            let isolation_key = NetworkIsolationKey::from_top_level(&current_url);
            let cache_key = PartitionedCacheKey::new(&isolation_key, current_url.as_str());
            let cached = self.cached_text(&cache_key);
            let now = Instant::now();

            if mode == DocumentRequestMode::Navigate
                && let Some(entry) = cached.as_ref()
                && entry.policy.fresh(entry.stored_at, now)
            {
                return Ok(entry.to_response(&requested_url, redirect_count, CacheStatus::Fresh));
            }

            let mut request = self.agent.get(current_url.as_str()).header(
                "Accept",
                "text/html,application/xhtml+xml,text/css,text/plain;q=0.9,*/*;q=0.5",
            );

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

            let mut response = request
                .call()
                .map_err(|error| NetworkError::Request(error.to_string()))?;
            let status = response.status().as_u16();

            if is_document_redirect_status(status) {
                self.remove_cached_text(&cache_key);

                if redirect_count >= MAX_DOCUMENT_REDIRECTS {
                    return Err(NetworkError::RedirectLimitExceeded {
                        limit: MAX_DOCUMENT_REDIRECTS,
                    });
                }

                let location = response
                    .headers()
                    .get("location")
                    .and_then(|value| value.to_str().ok())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or(NetworkError::RedirectMissingLocation { status })?;

                let next_url = current_url.resolve(location)?;
                let next_key = redirect_visit_key(&next_url);

                if !visited.insert(next_key.clone()) {
                    return Err(NetworkError::RedirectLoop(next_key));
                }

                current_url = next_url;
                redirect_count = redirect_count.saturating_add(1);
                continue;
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

                let refreshed = CachedTextResponse {
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
                    self.store_cached_text(cache_key.clone(), refreshed.clone());
                } else {
                    self.remove_cached_text(&cache_key);
                }

                return Ok(refreshed.to_response(
                    &requested_url,
                    redirect_count,
                    CacheStatus::Revalidated,
                ));
            }

            let body_bytes = response
                .body_mut()
                .with_config()
                .limit(self.max_text_body_bytes)
                .read_to_vec()
                .map_err(|error| NetworkError::Body(error.to_string()))?;

            let body = decode_text_body(&body_bytes, content_type.as_deref())?;

            let text_response = TextResponse {
                requested_url: requested_url.clone(),
                final_url: final_url.as_str().to_owned(),
                status,
                content_type: content_type.clone(),
                redirect_count,
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
                    self.store_cached_text(
                        cache_key,
                        CachedTextResponse {
                            final_url: text_response.final_url.clone(),
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
                    self.remove_cached_text(&cache_key);
                }
            } else {
                self.remove_cached_text(&cache_key);
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
        let requested_url = url.as_str().to_owned();
        let cache_key = PartitionedCacheKey::new(isolation_key, &requested_url);
        let cached = self.cached_binary(&cache_key);
        let now = Instant::now();

        if let Some(entry) = cached.as_ref()
            && entry.policy.fresh(entry.stored_at, now)
        {
            return Ok(entry.to_response(&requested_url, CacheStatus::Fresh));
        }

        for attempt in 0..MAX_BINARY_FETCH_ATTEMPTS {
            let mut request = self.agent.get(url.as_str()).header("Accept", IMAGE_ACCEPT);

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

                return Ok(refreshed.to_response(&requested_url, CacheStatus::Revalidated));
            }

            let body: Arc<[u8]> = response
                .body_mut()
                .with_config()
                .limit(self.max_binary_body_bytes)
                .read_to_vec()
                .map_err(|error| NetworkError::Body(error.to_string()))?
                .into();

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
        !name.is_empty() && name != "*" && name.eq_ignore_ascii_case("accept")
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
        CachePolicy, CacheStatus, HttpUrl, NetworkClient, NetworkIsolationKey, OriginScheme,
        TextResponse, UrlError, admit_document, cache_policy, trim_http_quotes,
        vary_allows_fixed_accept_request,
    };
    use std::sync::Arc;
    use std::time::{Duration, Instant};

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
