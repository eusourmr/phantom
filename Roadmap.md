# Phantom Roadmap

The roadmap prioritizes a correct, testable engine before product breadth.

## Phase 0 — Foundation

- [x] Rust workspace and crate boundaries
- [x] engineering constitution
- [x] `unsafe` forbidden by default
- [x] capability primitive
- [x] minimal invariant-preserving DOM model
- [x] CI baseline
- [x] public manifesto and project page source
- [ ] branch protection and required reviews
- [ ] private security advisory channel
- [ ] dependency/SBOM policy automation

## Cross-cutting security track — Phantom Guardian

Phantom Guardian is a planned local-first security-intelligence layer that complements, but never replaces, deterministic browser security. The Guardian Security Event Contract is defined early so navigation, origin, network, permission, download, certificate and policy subsystems can emit typed, minimal and auditable signals without depending on Guardian itself; actual risk scoring or compact local-model inference is deferred until process isolation, capability enforcement, observability and security-event provenance are mature enough to support it safely.

## Phase 1 — Document pipeline

- [ ] byte/text input abstraction
- [ ] URL/origin model
- [ ] HTML tokenizer
- [ ] HTML tree builder
- [ ] DOM conformance tests
- [ ] basic document loader

**Exit criterion:** load a local HTML document into Phantom's own DOM with no third-party browser engine.

## Phase 2 — CSS and layout

- [ ] CSS tokenizer/parser
- [ ] selector matching
- [ ] cascade
- [ ] computed style
- [ ] block/inline layout foundations
- [ ] deterministic layout tests

**Exit criterion:** render a defined subset of HTML/CSS test pages deterministically.

## Phase 3 — Rendering

- [ ] scene graph
- [ ] GPU abstraction
- [ ] WebGPU/wgpu renderer evaluation
- [ ] text and font pipeline
- [ ] image decode sandbox boundary
- [ ] compositor

## Phase 4 — Networking and isolation

- [ ] HTTP stack boundary
- [ ] TLS policy
- [ ] site-instance model
- [ ] typed IPC protocol
- [ ] renderer sandbox
- [ ] network sandbox
- [ ] Guardian security-event emission boundary

## Phase 5 — JavaScript runtime

- [ ] ECMAScript parser strategy
- [ ] bytecode interpreter
- [ ] garbage collection design
- [ ] event loop
- [ ] DOM bindings
- [ ] WebAssembly boundary

JIT optimization is deliberately deferred until correctness and observability are mature.

## Phase 6 — Semantic runtime

- [ ] semantic entity model
- [ ] source attribution
- [ ] page-to-semantic graph pipeline
- [ ] local semantic index
- [ ] temporal change model
- [ ] Guardian behavioral-signal adapters

## Phase 7 — Memory and agents

- [ ] user-authorized memory
- [ ] capability broker integration
- [ ] policy engine
- [ ] agent proposal model
- [ ] human approval gates
- [ ] auditable action records
- [ ] Guardian local risk assessment v1

## Phase 8 — Browser product

- [ ] native shell
- [ ] spaces/contexts
- [ ] memory UI
- [ ] agent UI
- [ ] Guardian security UI
- [ ] downloads
- [ ] permissions
- [ ] accessibility
- [ ] extension/WASM model
