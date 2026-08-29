# ADR — 2D-1 Document Admission Boundary

## Decision

Keep transport/cache ownership in `phantom-net` and place a narrow admitted
top-level document response directly above the existing `TextResponse`.

## Rationale

Creating another cache or another HTTP client for documents would duplicate the
validated 2C-11 semantics and create divergence between Navigate and Reload.

`DocumentResponse` therefore wraps an already bounded/cached text response after
media/status admission.

Strict byte-to-text decoding is performed before cached text is created so a
lossily decoded representation can never enter the document cache.

## Non-goals

2D-1 does not implement:
- full WHATWG MIME sniffing;
- `<meta charset>` encoding restart;
- legacy encodings;
- download handling;
- PDF/image top-level viewers;
- navigation state-machine refactor.

Those require later milestones and explicit contracts.
