# Phantom — Continuidade após 2D-1

## Completed 2D-1 boundary

Network/document loading now has:
- strict bounded text decoding;
- typed charset failures;
- HTML/XHTML media admission;
- bounded missing-MIME fallback;
- no-content and partial-response policy;
- preserved document cache/reload semantics;
- centered browser error presentation;
- no technical status row in the navigation bar.

## Next

### 2D-2 — URL/Origin Domain Consolidation

Goals:
- make Origin an explicit public domain type;
- canonical effective-port behavior;
- same-origin comparison through typed values;
- base/document URL contract consolidation;
- remove remaining informal string-origin comparisons.

Small browser increment:
- a compact identity/security affordance associated with the address field,
  without bringing diagnostics back into the navigation bar.

Then:
- 2D-3 Navigation State Machine
- 2D-4 Navigation Compatibility Suite
