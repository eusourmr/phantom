# Phantom — Continuidade após 2D-3

## Completed

2D-3 consolidates top-level browser navigation around one explicit state
machine.

Removed as independent state sources:

- `page_loaded`
- `document_error`
- top-level `pending`

Preserved:

- 2D-2 typed Origin identity;
- 2D-1 hardened Document Loader and centered error surface;
- 2C-15 favicon fallback;
- 2C-14 fragment/history scroll lifecycle;
- 2C-13 forms;
- 2C-12 link navigation;
- 2C-10 recently closed tabs;
- approved full-width tab strip and `Maximize2`.

## Next

### 2D-4 — Navigation Compatibility Suite

Primary goals:

- deterministic cross-document navigation cases;
- redirect + fragment final-URL cases;
- reload/cache/state transition cases;
- history transition cases;
- failed navigation recovery;
- same-document navigation compatibility;
- navigation cancellation/generation cases;
- browser smoke matrix tied to explicit state phases.

The next milestone should primarily expand validation/compatibility rather than
add another architectural abstraction.
