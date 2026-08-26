# Phantom

**Phantom** is an independent, open-source browser and web engine project designed from the ground up around safety, auditability, modularity, semantic understanding and human-controlled agentic execution.

Phantom is **not a Chromium, WebKit or Gecko fork**. The project intends to build its own engine incrementally while remaining standards-oriented and interoperable with the open web.

> Correctness before convenience. Security before compatibility. Composition before inheritance. Explicit before implicit. Auditable before clever.

## Why Phantom

Today's browsers are optimized around pages, tabs and URLs. Phantom is exploring a different model: the browser as an execution environment that can understand context, represent semantic entities, remember user-authorized knowledge and let agents act only through explicit security capabilities.

Read the [Manifesto](MANIFESTO.md).

## Status

Phantom is at **foundation stage (`0.0.1`)**. It is not yet a usable browser. The first milestone is deliberately narrow: establish a clean Rust workspace, strong invariants, a capability security model and a minimal DOM/engine boundary before implementing HTML, CSS, layout and rendering.

## Initial architecture

```text
Phantom Browser
      |
      v
Phantom Engine
      |
      +-- DOM
      +-- HTML            (planned)
      +-- CSS / Style     (planned)
      +-- Layout          (planned)
      +-- Render / GPU    (planned)
      +-- JavaScript      (planned)
      +-- Network         (planned)
      +-- Storage         (planned)
      +-- Security
      |
      +-- Semantic Runtime (planned)
      +-- Memory           (planned)
      `-- Agent Runtime    (planned)
```

Initial crates:

- `phantom-core` — shared low-level types and invariants.
- `phantom-security` — capability-based authorization primitives.
- `phantom-dom` — invariant-preserving DOM foundation.
- `phantom-engine` — top-level engine orchestration boundary.
- `phantom-browser` — native browser bootstrap executable.

See [ARCHITECTURE.md](ARCHITECTURE.md), [CODING_STANDARD.md](CODING_STANDARD.md) and [ROADMAP.md](ROADMAP.md).

## Engineering constitution

- Rust is the primary implementation language.
- `unsafe` is forbidden by default.
- Composition and traits are preferred over inheritance-style hierarchies.
- Domain types are preferred over primitive strings and boolean switches.
- Expected failures use typed errors and `Result`.
- Production code avoids `unwrap()`, `expect()`, `panic!()`, `todo!()` and `unimplemented!()`.
- Privileged side effects require explicit capabilities.
- Cross-process and cross-component input is untrusted by default.
- Critical behavior must be testable, observable and auditable.

## Build

A current stable Rust toolchain is required.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

## Project page

The repository contains a static project page under `site/`, prepared for GitHub Pages. It presents the manifesto, architecture and roadmap without requiring a frontend framework.

## Security

Do not report vulnerabilities in public issues. See [SECURITY.md](SECURITY.md).

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before proposing changes. Structural changes require an ADR under `docs/adr/`.

## License

Phantom core is licensed under the **Mozilla Public License 2.0 (MPL-2.0)**.
