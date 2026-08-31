# Phantom 2D-4 — Navigation Compatibility Suite

2D-4 is intentionally validation-first. It adds no new browser architecture,
network cache or dependency.

Browser coverage:
- same-document fragment navigation remains Ready;
- fragment Back/Forward restores saved scroll without fetching;
- Reload never uses the same-document shortcut;
- requested fragments survive a redirect commit;
- response fragments take precedence;
- forward-history branches are truncated correctly;
- stale generations are discarded;
- disconnected workers become Failed;
- pending workers remain Fetching;
- Failed can recover into a new Fetching state;
- cross-document targets never take the fragment shortcut.

Network/document coverage:
- relative 302/307 redirect chains;
- final URL and redirect hop count;
- redirect-loop rejection;
- missing Location rejection;
- fresh cache reuse without a second request;
- Reload validator request plus HTTP 304 body reuse;
- HTML 404 remains a renderable document;
- URL fragments never enter the HTTP request.

All HTTP tests use a local std::net::TcpListener. No internet, sleeps, mocks or
test-only dependencies are used.
