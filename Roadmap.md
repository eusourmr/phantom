# Phantom Roadmap

Phantom prioritizes a correct, testable and security-bounded engine before product breadth. The roadmap is intentionally incremental: a phase may begin only when the lower layers have contracts strong enough to support it.

> **Current status:** pre-beta independent browser engine. HTML, CSS, layout, paint, images, networking and native browser foundations exist today. The current emphasis is compatibility and security hardening before the next parser/runtime line.

## Established foundations

The repository already contains working foundations for:

- [x] Rust workspace with explicit crate boundaries
- [x] native browser shell
- [x] capability-based security primitives
- [x] owned DOM with stable identifiers
- [x] independent HTML parsing foundation
- [x] CSS parsing, selector matching, cascade and computed style
- [x] block and inline layout foundations
- [x] Flexbox foundations and executable compatibility slices
- [x] renderer-neutral paint pipeline
- [x] text shaping boundary
- [x] PNG, JPEG, GIF, WebP and ICO image foundations
- [x] responsive/replaced image work
- [x] HTTP(S), URL/origin and cache foundations
- [x] network isolation and cache partitioning foundations
- [x] browser navigation/history lifecycle foundations
- [x] strict Rust/Clippy engineering invariants

These items describe implemented foundations, not full web-platform conformance.

## Current line — compatibility and security hardening

Before expanding the engine, Phantom is tightening the contracts already present:

- [x] navigation compatibility coverage
- [x] network/resource security hardening foundations
- [ ] parser/DOM/CSS/layout adversarial budgets fully gated in the canonical branch
- [ ] supply-chain security gate fully gated in the canonical branch
- [ ] hardened CI/release policy fully gated
- [ ] protected `main` and protected release tags
- [ ] private vulnerability reporting enabled before Beta

**Exit criterion:** the current security gate must pass before the next engine line is treated as active.

## Next — HTML tokenizer and tree-builder maturation

Planned sequence:

1. deterministic tokenizer foundation with source positions and bounded attributes,
2. tree-builder foundation,
3. error recovery and character references,
4. focused HTML compatibility/WPT subset.

Security budgets introduced during hardening remain part of the parser contract rather than optional diagnostics.

## CSS maturation

- broader tokenizer/parser correctness,
- selector and cascade compatibility,
- stronger computed-value handling,
- bounded complexity and numeric robustness,
- standards-derived executable tests.

## Layout maturation

- stronger block/inline compatibility,
- Flexbox expansion and correctness,
- intrinsic sizing maturation,
- additional formatting contexts only when testable,
- performance budgets and adversarial layout coverage.

## Scene graph, GPU and compositor

- renderer-independent scene representation,
- `wgpu`/GPU backend evaluation behind a narrow boundary,
- compositor architecture,
- image/text integration without giving rendering components ambient authority.

## Security, storage and process isolation

- site-instance model,
- typed IPC boundaries,
- renderer/network/GPU isolation,
- storage and permission boundaries,
- capability broker maturation,
- deterministic policy kernel before intelligent assistance.

## Events, forms and event loop

- event dispatch model,
- form semantics maturation,
- task/event loop foundations,
- DOM/event bindings prepared for a future JavaScript runtime.

## JavaScript runtime

Phantom intends to add JavaScript only after lower-layer contracts and isolation are mature.

Planned principles:

- interpreter-first architecture,
- no JIT requirement for the first generation,
- explicit host bindings,
- observable execution budgets,
- event-loop integration,
- garbage-collection design with security and debuggability as first-class constraints.

## Guardian / local intelligence

Phantom's future intelligence layer does **not** replace browser security policy.

The intended hierarchy is:

```text
Security Kernel
      ↓
Isolation
      ↓
Privacy
      ↓
Guardian Intelligence
      ↓
Automation
```

Guardian is expected to begin local-first, using explicit typed security events and bounded local inference. It does not receive implicit filesystem, network, permission or action authority.

## Beta gate

Phantom reaches Beta when the supported subset is stable, documented and usable — not when the entire web platform is implemented.

The Beta gate includes, at minimum:

- deterministic supported HTML/CSS behavior,
- stable navigation and resource lifecycle,
- security/isolation gates passing,
- fuzzing and adversarial corpora continuously exercised,
- accessibility foundations,
- performance budgets,
- reproducible/auditable release path,
- protected repository governance,
- explicit documentation of unsupported web-platform behavior.

JavaScript breadth, full WPT parity and complete browser compatibility are **not** prerequisites for the first Beta if the supported subset is honest, stable and secure.
