# Phantom 2D-5 — Network & Resource Security Hardening

## Security Gate findings addressed

2D-5 targets SG-001 through SG-004 from the 2D Security Gate.

### SG-001 — decompression expansion

Every document and binary body now has two bounds:

1. ureq input/wire-side reader limit;
2. Phantom-owned reader limit over the decoded/decompressed output.

The outer reader probes at most `decoded_limit + 1` and rejects expansion beyond
the configured ceiling.

### SG-002 — automatic private-network requests

Public-network navigations and automatic binary subresources now pass a
resolver that filters the DNS addresses actually handed to the connector.

In addition:

- URL/namespace policy rejects explicit loopback/private subresources from a
  public top-level context;
- a public navigation cannot redirect into an explicit private namespace;
- a public-looking hostname that resolves only to private/local addresses is
  rejected by the resolver;
- explicit user navigation to a private IP, `localhost`, `.local`, `home.arpa`
  or a single-label local host remains possible, and that private top-level may
  load its own private subresources.

Environment proxy discovery is disabled for the current security boundary so a
proxy cannot bypass the resolver policy.

### SG-003 — mixed content

HTTPS top-level origins cannot automatically load HTTP binary subresources.
This centrally covers images, preloads and site icons.

### SG-004 — document automatic-resource budgets

Browser limits:

- 64 image elements;
- 16 image preloads;
- 64 distinct image/preload requests after URL dedupe;
- 8 site icon candidates including `/favicon.ico`;
- 72 automatic binary fetches per document;
- 96 MiB total reserved binary body budget per document;
- 1 MiB response ceiling for each favicon candidate;
- 16 MiB response ceiling for each image/preload.

Reservations are shared by image and favicon workers. A failed request consumes
its conservative byte reservation; successful short responses refund unused
bytes.

Site icon workers now receive a cancellation token tied to document generation.

## Intentionally not in 2D-5

- HTML/DOM/CSS/layout structural budgets;
- parser raw-text quadratic-work fix;
- numeric CSS hardening;
- fuzzing;
- GitHub/CI/release hardening.

Those remain 2D-6.
