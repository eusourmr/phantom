# Engineering Review Resolution — 2026-08-28

This document records which external-review recommendations apply to Phantom's current code and which do not.

## Already implemented / review premise was stale

### URL parsing
`phantom-net` already wraps the `url` crate's `Url`. It validates HTTP/HTTPS, hosts, credentials, relative resolution and schemeful origins. Phantom does not use a regex URL parser.

**Decision:** keep the current boundary. Do not move URL parsing into `phantom-core` merely to make every crate depend on it.

### Core layering
`phantom-core` currently contains low-level shared types such as request/build identifiers. DOM types live in `phantom-dom`; CSS lives in `phantom-css`.

**Decision:** no rewrite.

### Executable entrypoint
`phantom-browser/src/main.rs` is the native executable and already opens/owns the browser window and chrome.

**Decision:** do not add a duplicate `phantom-window` executable.

### `unwrap` / `expect`
Workspace Clippy policy already denies `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` and `dbg_macro`.

**Decision:** keep the strict gates.

### CI
The project CI is intended to run format, check, Clippy, tests and rustdoc against the real workspace, with a Windows native-browser gate in the current governance baseline.

## Valid concerns accepted

### DOM handles before JavaScript mutation
The current DOM avoids `Rc<RefCell>` and uses stable numeric `NodeId`s. It also does not yet support arbitrary node deletion/reuse.

Before a scripting runtime can retain live DOM handles across mutation/removal, Phantom must define stale-handle behavior. A generational arena is a strong candidate.

**Decision:** make generational/revocable handles a pre-scripting architecture gate, not a 2C-11 rewrite. See ADR-0003.

### Web Platform Tests
A browser cannot measure compatibility only against self-authored fixtures.

**Decision:** add a curated, commit-pinned WPT adoption path in the parser-compatibility phase. Do not clone the full WPT corpus during every ordinary CI run. See `WPT-ADOPTION.md`.

### Observability
Structured spans become increasingly valuable as navigation, network, resource scheduling and scripting become asynchronous.

**Decision:** define an observability contract before adding `tracing`. There are no production `println!` debug calls that require immediate replacement. Dependencies are added when real span/event points are introduced. See `OBSERVABILITY-STRATEGY.md`.

## Recommendations deliberately rejected or deferred

### Make DOM `Send + Sync` and run JS/layout concurrently immediately
A mutable web DOM does not become safer merely by making it concurrently accessible. Phantom already has a useful separation: DOM/style state feeds immutable layout/paint snapshots.

**Decision:** preserve ownership boundaries; parallelize measured pure/snapshot work later. Do not introduce locks/atomics around every node preemptively.

### HTTP/3, QUIC and TLS 1.3 0-RTT now
Transport performance is not the current compatibility bottleneck. 0-RTT also has replay semantics that require explicit policy.

**Decision:** defer HTTP/3/QUIC until HTTP semantics, caching, redirects, cookies/security policy and connection reuse have measured baselines.

### Replace renderer with wgpu immediately
Phantom already has a renderer-neutral `PaintList` boundary. That is the important architectural prerequisite for a future GPU compositor.

**Decision:** keep the backend replaceable. Introduce a dedicated GPU compositor only from measured performance/feature requirements.

### Set unmeasured performance SLAs
Targets such as “30% faster than Chrome”, “<30 MB per tab” or “cold start <150 ms” are not engineering contracts without reproducible hardware, corpus and benchmark methodology.

**Decision:** first create benchmarks and baselines; then set targets.

### Add `anyhow`, `proptest`, `rayon`, `quinn`, QuickJS/Hermes, `wgpu`, etc. immediately
Dependencies are not architecture. Each dependency adds maintenance, compile time and supply-chain surface.

**Decision:** add one only when an implemented milestone requires it and an ADR/short rationale demonstrates why existing boundaries are insufficient.

## Root patch scripts

`APPLY-2C*.ps1` files are delivery artifacts for incremental local updates, not engine architecture. They should not be treated as permanent source modules.

**Decision:** future repository cleanup should archive/remove obsolete tracked patch scripts after the corresponding code is committed. Active incremental package scripts may remain in the local working directory until homologation.
