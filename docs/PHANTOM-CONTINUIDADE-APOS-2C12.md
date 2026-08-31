# Phantom — Continuidade após 2C-12

## Homologation target

2C-12 closes the first real hyperlink interaction slice:

`DOM anchor -> cold layout region -> hit test -> browser URL resolution -> navigation`

The browser now has a meaningful interaction path beyond typing URLs manually.

## Roadmap state

Completed/current 2C line:

- image fetch/decode/raster;
- responsive image selection;
- lazy loading/cancellation;
- HTTP cache revalidation/recovery;
- network isolation/cache partitioning;
- resource priority/preload;
- redirect guardrails;
- document cache/reload semantics;
- Site Identity I;
- Link Navigation Semantics I.

## Valid feedback incorporated

- URL remains on `url::Url`; no custom parser.
- DOM remains separated from core.
- No `Rc<RefCell>` DOM rewrite is required.
- generational/revocable handles remain a pre-scripting gate.
- WPT adoption begins through compatibility-test discipline before a full harness.
- no speculative tracing/QUIC/GPU dependency is added without an implementation need.

## Recommended next milestone

### 2C-13 — Form Navigation I + Site Identity II

Engine:
- bounded `<form method="get">` semantics;
- successful-control subset;
- explicit action resolution contract;
- no password/upload/post body yet.

Browser/UX:
- basic text input/button interaction required to exercise GET forms;
- Site Identity II may add ICO/fallback only if it remains a bounded secondary
  change; otherwise favicon expansion moves to the following milestone.

The priority remains browser correctness over breadth.
