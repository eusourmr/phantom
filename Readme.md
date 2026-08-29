<p align="center">
  <img src="crates/phantom-browser/assets/branding/phantom-logo.png" alt="Phantom" width="180">
</p>

<h1 align="center">Phantom</h1>

<p align="center">
  <strong>An independent web browser and engine built from first principles in Rust.</strong><br>
  Security-first. Auditable. Local-first. Human-controlled.
</p>

<p align="center">
  <a href="https://github.com/eusourmr/phantom/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/eusourmr/phantom/ci.yml?branch=main&style=flat-square&label=CI"></a>
  <img alt="Rust 2024" src="https://img.shields.io/badge/Rust-2024-000000?style=flat-square&logo=rust&logoColor=white">
  <a href="LICENSE"><img alt="MPL-2.0" src="https://img.shields.io/badge/license-MPL--2.0-7B68EE?style=flat-square"></a>
  <img alt="Status pre-beta" src="https://img.shields.io/badge/status-pre--beta-1f6feb?style=flat-square">
</p>

<p align="center">
  <a href="Manifesto.md">Manifesto</a> ·
  <a href="Architecture.md">Architecture</a> ·
  <a href="Roadmap.md">Roadmap</a> ·
  <a href="Security.md">Security</a> ·
  <a href="docs/README.md">Engineering docs</a>
</p>

---

## The idea

The web became one of humanity's main interfaces with knowledge, work, commerce and communication. Yet the dominant browser model still treats people primarily as operators of pages, tabs and forms.

Phantom starts from a different premise:

> **The browser should understand the user's objective, preserve context, explain what it knows, and act only within explicit human-controlled boundaries.**

Phantom is **not a Chromium, WebKit or Gecko fork**. The engine is being built incrementally from its own components and contracts, with standards compatibility pursued without surrendering architectural independence.

## What exists today

Phantom is still **pre-beta**, but the repository is well beyond an empty browser shell. The current workspace contains independent subsystems for the document pipeline, style, layout, paint, images, networking, text and security.

| Layer | Crate | Responsibility |
| --- | --- | --- |
| Product | [`phantom-browser`](crates/phantom-browser) | Native browser shell and chrome |
| Orchestration | [`phantom-engine`](crates/phantom-engine) | DOM → style → layout → paint pipeline |
| Core | [`phantom-core`](crates/phantom-core) | Shared primitives and invariants |
| DOM | [`phantom-dom`](crates/phantom-dom) | Owned DOM with stable node identifiers |
| HTML | [`phantom-html`](crates/phantom-html) | Independent HTML parsing foundation |
| CSS | [`phantom-css`](crates/phantom-css) | Parsing, selectors, cascade and computed style |
| Layout | [`phantom-layout`](crates/phantom-layout) | Cold block/inline/Flexbox layout snapshots |
| Paint | [`phantom-paint`](crates/phantom-paint) | Renderer-neutral paint commands |
| Text | [`phantom-text`](crates/phantom-text) | Text shaping boundary and metrics |
| Images | [`phantom-image`](crates/phantom-image) | Raster probing/decoding and image metadata |
| Network | [`phantom-net`](crates/phantom-net) | HTTP(S), URL/origin and cache foundations |
| Security | [`phantom-security`](crates/phantom-security) | Explicit capability primitives |

JavaScript execution is **not** implemented yet. Phantom is intentionally building lower layers and security contracts before adding a JavaScript runtime.

## Architecture at a glance

```text
                         ┌──────────────────────┐
                         │   Phantom Browser    │
                         └──────────┬───────────┘
                                    │
                         ┌──────────▼───────────┐
                         │    Phantom Engine    │
                         └──────────┬───────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              │                     │                     │
      ┌───────▼───────┐     ┌───────▼───────┐     ┌───────▼───────┐
      │  Web Runtime  │     │   Security    │     │ Intelligence  │
      │ HTML / DOM    │     │ Capabilities │     │ future/local  │
      │ CSS / Layout  │     │ Policy gates │     │ human-bound   │
      │ Paint / Image │     └───────────────┘     └───────────────┘
      │ Network / Text│
      └───────────────┘
```

The intended dependency direction is explicit and one-way. Layout does not own DOM pointers; paint consumes cold layout/style snapshots; privileged operations must cross defined security boundaries.

Read the full [architecture document](Architecture.md).

## Engineering constitution

```text
correctness > convenience
security    > compatibility shortcuts
explicit    > implicit
composition > inheritance
reviewable  > clever
```

- Rust is the primary implementation language.
- `unsafe` is forbidden by default in Phantom code.
- Domain types are preferred over primitive strings and boolean switches.
- Expected failures use typed errors and `Result`.
- Production paths avoid `unwrap()`, `expect()`, `panic!()`, `todo!()` and `unimplemented!()`.
- Inputs crossing trust boundaries are untrusted by default.
- Dependencies are part of the attack surface.
- Security-sensitive behavior must be testable and auditable.

See [Coding.md](Coding.md) and [Diretivas.md](Diretivas.md).

## Build and verify

Install the Rust toolchain described by [`rust-toolchain.toml`](rust-toolchain.toml), then run from the repository root:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Windows release build:

```powershell
cargo build --release -p phantom-browser --locked
.\target\release\phantom-browser.exe
```

## Security model

Phantom treats web content, network responses and future cross-process messages as hostile input. Security is part of the architecture rather than a post-release feature.

Do **not** report suspected vulnerabilities in public issues. Read the [Security Policy](Security.md).

## Documentation

The engineering record is intentionally public. Milestone notes, standards maps, ADRs and continuity records live under [`docs/`](docs/README.md).

Core documents:

- [Manifesto](Manifesto.md) — product and human-control thesis.
- [Architecture](Architecture.md) — system boundaries and target process model.
- [Roadmap](Roadmap.md) — high-level sequencing.
- [Coding Standard](Coding.md) — implementation rules.
- [Project Directives](Diretivas.md) — deeper engineering doctrine.
- [Contributing](Contributing.md) — contribution and review expectations.

## Contributing

Phantom favors small, reviewable changes that preserve security and architectural invariants. Start with [Contributing.md](Contributing.md).

## License

Phantom core is licensed under the **Mozilla Public License 2.0 (MPL-2.0)**. See [`LICENSE`](LICENSE).
