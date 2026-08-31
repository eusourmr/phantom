# Phantom Roadmap

This roadmap is an **execution plan**, not a list of everything a modern browser could eventually contain.

## Scope doctrine

1. The near-term target is **Engine Beta**, not feature parity with Chrome/Firefox/Safari.
2. Every milestone must produce executable, testable behavior.
3. Browser-engine work always has priority over semantic memory, agents and other product intelligence.
4. JavaScript integration is designed early enough to avoid architectural refactors, but a home-grown JIT is not a Beta requirement.
5. Standards compliance grows from measured subsets and regression tests; unsupported features are documented rather than implied.

## Completed foundation

- [x] Rust workspace and crate boundaries
- [x] native browser shell
- [x] invariant-preserving DOM
- [x] bounded independent HTML parsing/tree construction
- [x] CSS parsing, selector matching, cascade and computed style subset
- [x] block/inline layout foundations
- [x] Flexbox core
- [x] renderer-neutral paint pipeline
- [x] text boundary
- [x] raster images and animated image lifecycle
- [x] HTTP/HTTPS transport boundary
- [x] bounded binary cache, revalidation and stale recovery
- [x] network-isolation-key cache partitioning
- [x] CI running format, Clippy, tests and documentation

## Current line — 2C: Real-resource and navigation robustness

Goal: make the existing engine reliable on real network documents before broadening standards surface.

- [x] image fetch/decode/raster pipeline
- [x] responsive image selection subset
- [x] lazy image lifecycle and cancellation
- [x] HTTP cache revalidation and image recovery
- [x] cache partitioning by network isolation key
- [x] resource priority/preload scheduling slice
- [ ] redirect/navigation guardrails
- [ ] document cache semantics and reload revalidation
- [ ] site identity/favicon pipeline
- [ ] navigation/link semantics hardening
- [ ] bounded forms/input navigation subset

### 2C exit criterion

A native Phantom build can repeatedly navigate a defined corpus of HTTP/HTTPS pages, follow bounded redirects, render supported HTML/CSS/text/images, reload/revalidate safely, manage tabs, and survive malformed/unsupported content without crashes or unbounded resource use.

## 2D — Parser correctness and web-compatibility slice

Goal: improve compatibility deliberately instead of adding random CSS/HTML features.

- [ ] split HTML tokenizer/tree-builder responsibilities where needed
- [ ] expand malformed-markup recovery
- [ ] define supported insertion-mode subset
- [ ] improve character-reference coverage
- [ ] strengthen CSS tokenization/error recovery
- [ ] expand selectors only from measured page/WPT failures
- [ ] add parser fuzz targets
- [ ] adopt a curated Web Platform Tests subset as a compatibility gate

See `docs/PARSER-STRATEGY.md`.

## 2E — Script-ready browser architecture

Goal: make scripting an explicit subsystem before dynamic-web work expands.

- [ ] create `phantom-script` boundary only when implementation begins
- [ ] define task/microtask/event-loop contracts
- [ ] define safe DOM mutation commands/bindings
- [ ] define fetch/timer/event interfaces
- [ ] evaluate an embeddable ECMAScript runtime behind the boundary
- [ ] execute a minimal dynamic-page test corpus

**Not in this milestone:** building a production JIT from scratch.

See `docs/SCRIPTING-STRATEGY.md`.

## Engine Beta gate

Engine Beta is reached only when all of the following are true:

- [ ] documented HTML/CSS compatibility subset
- [ ] deterministic regression corpus
- [ ] parser/network malformed-input tests
- [ ] navigation/cache lifecycle stable
- [ ] bounded memory/resource policies for implemented paths
- [ ] Linux CI green
- [ ] Windows CI green and native shell build verified
- [ ] no Clippy warnings
- [ ] no undocumented production `unsafe`
- [ ] unsupported web-platform features clearly documented

Engine Beta **does not imply** general-purpose modern-web compatibility.

## Browser Technology Preview

After Engine Beta:

- [ ] minimal JavaScript runtime integration
- [ ] DOM bindings and events
- [ ] timers/task queues
- [ ] limited dynamic fetch integration
- [ ] basic form controls and focus/keyboard behavior
- [ ] accessibility foundation
- [ ] download lifecycle

This is the first milestone intended to evaluate substantially dynamic websites.

## Deferred until after browser fundamentals

These are long-term possibilities, not near-term commitments:

- custom JavaScript JIT;
- WebAssembly runtime ownership;
- full GPU compositor/process architecture;
- extension ecosystem;
- semantic entity runtime;
- user memory;
- autonomous/agent execution;
- broad AI-assisted browser actions.

They may be researched earlier, but they **must not block or destabilize the browser engine roadmap**.
