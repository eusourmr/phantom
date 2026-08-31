# Phantom 2D-4 — Compatibility Matrix

| Area | Case | Expected |
|---|---|---|
| Fragment | page -> page#section | Ready -> Ready |
| Fragment history | Back/Forward | saved scroll, no fetch |
| Reload | fragmented URL | network path |
| Redirect + fragment | /start#x -> /final | /final#x |
| History branch | A -> B -> C, back B, then D | A -> B -> D |
| Generation | stale fetch | discarded |
| Worker | disconnected | Failed |
| Worker | no result | Fetching |
| Recovery | Failed -> new fetch | Fetching |
| Redirect | relative 302/307 | correct final URL/hops |
| Redirect loop | A -> B -> A | RedirectLoop |
| Redirect malformed | missing Location | RedirectMissingLocation |
| Cache | repeated fresh navigation | Fresh, no second I/O |
| Reload cache | ETag + Reload + 304 | Revalidated, body reused |
| HTTP error page | 404 text/html | admitted document |
| Fragment transport | URL #section | fragment absent from request |
