# Phantom 2D-1 — Document Loader Hardening

## New boundary

Top-level browser navigation no longer consumes a generic `TextResponse`
directly.

`phantom-net` exposes:

- `DocumentResponse`
- `DocumentMediaType`
- `DocumentLoadError`
- `NetworkClient::fetch_document()`
- `NetworkClient::reload_document()`

The existing 2C document cache remains underneath this boundary.

## Media admission

Explicitly admitted:
- `text/html`
- `application/xhtml+xml`

Explicit other media types are rejected as top-level web documents.

When `Content-Type` is missing, Phantom performs a deliberately bounded HTML
sniff over at most the first 1024 decoded characters. This is compatibility
fallback, not a claim of WHATWG MIME-sniffing conformance.

HTTP 204/205 are explicit no-document results.
HTTP 206 is rejected as the main document representation.

## Text decoding

The old lossy UTF-8 path is removed.

Supported in 2D-1:
- UTF-8 without BOM;
- UTF-8 BOM, stripped before HTML reaches the engine;
- US-ASCII when bytes are actually ASCII.

Rejected explicitly:
- malformed UTF-8;
- UTF-16 BOM;
- declared legacy charsets such as ISO-8859-1 / Windows-1252.

Full HTML encoding sniffing and legacy charset decoding are intentionally
deferred rather than silently corrupting source text.
