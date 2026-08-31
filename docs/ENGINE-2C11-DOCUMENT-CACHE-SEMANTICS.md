# Phantom 2C-11 — Document Cache Semantics

## Objective

Extend the HTTP cache boundary from binary subresources to top-level text documents without hiding navigation semantics inside the transport library.

## Implemented slice

- bounded in-memory document cache;
- separate document-cache and binary-cache budgets;
- `Cache-Control: max-age` freshness;
- `no-store` and `no-cache` behavior;
- `ETag` / `If-None-Match` revalidation;
- `Last-Modified` / `If-Modified-Since` revalidation;
- HTTP `304 Not Modified` body reuse;
- existing conservative `Vary` policy (`Accept` only);
- explicit `DocumentRequestMode::{Navigate, Reload}`;
- normal navigation may reuse a fresh document;
- Reload never accepts freshness alone and sends `Cache-Control: max-age=0`;
- redirect responses remain uncached; a redirect may land on a cached final representation;
- document cache is memory-only and bounded.

## Deliberate boundaries

2C-11 does not implement disk cache, heuristic freshness, `Expires`, redirect-response caching, browser history cache/BFCache, cookies, service workers or stale-if-error fallback for top-level documents.

The document cache reuses the same cache-policy primitives already exercised by binary cache revalidation, but owns a separate bounded store.

## Browser integration

`NavigationAction::Reload` is wired to `NetworkClient::reload_text()`. New/history navigation continues through `fetch_text()`.

This distinction prevents Reload from accidentally being satisfied by a still-fresh in-memory representation.
