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

## Phase 7 — Memory and agents

- [ ] user-authorized memory
- [ ] capability broker integration
- [ ] policy engine
- [ ] agent proposal model
- [ ] human approval gates
- [ ] auditable action records

## Phase 8 — Browser product

- [ ] native shell
- [ ] spaces/contexts
- [ ] memory UI
- [ ] agent UI
- [ ] downloads
- [ ] permissions
- [ ] accessibility
- [ ] extension/WASM model
