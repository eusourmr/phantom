# Phantom — Continuidade após 2C-11

## Development rule

Each normal engine milestone advances two bounded surfaces:

1. one web-engine/runtime capability;
2. one browser/chrome capability.

Neither side may expand enough to obscure the engine milestone's testable exit criterion.

## 2C-11

### Engine — Document Cache Semantics

- bounded document cache;
- Cache-Control freshness;
- ETag and Last-Modified validators;
- 304 revalidation;
- explicit Navigate vs Reload cache behavior;
- redirect guardrails from 2C-10 preserved.

### Browser — Site Identity I

- `<link rel="icon">` discovery;
- bounded favicon fetch/decode;
- favicon displayed on normal tabs;
- favicon becomes primary identity of pinned tabs;
- Pin fallback preserved when identity is unavailable.

## Preserved baselines

- 2C-7 image recovery/cache revalidation;
- 2C-8 Network Isolation Key partitioning;
- 2C-9 priority/preload + system theme/pinned tabs/Lucide chrome;
- 2C-9 FIX 5 validated window controls;
- 2C-10 redirect guardrails + recently closed tabs.

## Recommended next milestone

**2C-12 — Link Navigation Semantics + Site Identity II**

Engine candidate:

- native link hit-testing/navigation contract;
- same-document fragment navigation boundary;
- explicit target/navigation intent without JavaScript.

Browser candidate:

- Site Identity II: ICO favicon support and favicon candidate fallback order.

Do not begin 2C-12 until 2C-11 passes all quality gates and native validation.
