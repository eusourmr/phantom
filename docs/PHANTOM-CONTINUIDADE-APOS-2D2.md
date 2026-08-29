# Phantom — Continuidade após 2D-2

## Homologated boundary after 2D-2

- `Origin` is a typed network/security domain value;
- same-origin checks no longer require callers to compare serialized strings;
- effective ports are canonicalized;
- `NetworkIsolationKey` stores typed origins;
- admitted documents expose typed requested/final URLs;
- browser chrome has a minimal transport-identity affordance.

## Next: 2D-3 — Navigation State Machine

Replace implicit combinations of `pending`, `page_loaded`, `document_error` and
status strings with an explicit navigation lifecycle state.

Planned states remain small and auditable: Idle, Fetching, Committing, Complete
and Failed, with History/Reload actions carried separately. The visual advance
will use these states only for transient page/loading/error presentation, not a
new status bar.
