# Phantom

**Phantom** is an independent, open-source browser and web-engine project written primarily in Rust. It is being built incrementally from first principles around safety, auditability, modularity and explicit human control.

Phantom is **not a Chromium, WebKit or Gecko fork**. Independence does not mean reimplementing every supporting component in-house: standards, test suites and carefully selected open-source libraries may be used where they do not replace Phantom's browser-engine architecture.

> Correctness before convenience. Security before compatibility. Explicit before implicit. Auditable before clever.

## Current status

Phantom is an **active pre-alpha engine and native browser shell**, not a production-ready general-purpose browser.

The repository already contains executable Rust code for:

- native desktop browser shell (`phantom-browser`);
- DOM (`phantom-dom`);
- independent bounded HTML parser/tree builder (`phantom-html`);
- CSS parsing, cascade and computed style (`phantom-css`);
- block/inline/Flexbox layout (`phantom-layout`);
- renderer-neutral paint commands (`phantom-paint`);
- text boundary (`phantom-text`);
- raster/animated image handling (`phantom-image`);
- HTTP/HTTPS transport, bounded response policy and partitioned cache (`phantom-net`);
- security capabilities (`phantom-security`);
- engine orchestration (`phantom-engine`).

The current development line is the **2C series**, focused on robust real-network document/image loading, cache semantics, navigation lifecycle and browser-chrome usability.

The project does **not** claim full HTML/CSS/JavaScript compatibility today.

## Near-term goal: Engine Beta, not “Chrome replacement”

The near-term milestone is deliberately bounded: **Phantom Engine Beta**.

Engine Beta means a testable independent engine capable of loading and painting a documented subset of the open web with robust navigation, networking, HTML/CSS parsing, text and images. It does **not** mean full parity with Chromium, Gecko or WebKit.

See [BETA-SCOPE](docs/BETA-SCOPE.md).

A later **Browser Technology Preview** introduces the scripting runtime boundary and enough JavaScript/DOM integration to exercise dynamic pages. Full ECMAScript engine/JIT work is not a prerequisite for Engine Beta and is not promised as a solo-project deliverable.

## Architecture

```text
Native Phantom Browser
        |
        v
phantom-engine
        |
        +-- phantom-html  -> phantom-dom
        +-- phantom-css
        +-- phantom-layout
        +-- phantom-paint
        +-- phantom-text
        +-- phantom-image
        +-- phantom-net
        `-- phantom-security
```

The semantic/memory/agent vision remains part of Phantom's long-term product thesis, but it is **post-browser-beta work and does not block the browser engine**.

See [ARCHITECTURE.md](ARCHITECTURE.md), [ROADMAP.md](ROADMAP.md), [PARSER-STRATEGY](docs/PARSER-STRATEGY.md) and [SCRIPTING-STRATEGY](docs/SCRIPTING-STRATEGY.md).

## Build and quality gates

Rust 1.95 is pinned by `rust-toolchain.toml`.

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

CI runs against real Rust code. Linux performs format, check, Clippy, tests and docs; Windows checks/tests and builds the native browser shell.

## Engineering rules

- Rust is the primary implementation language.
- `unsafe` is forbidden by default.
- Production code avoids `unwrap()`, `expect()`, `panic!()`, `todo!()` and `unimplemented!()`.
- New dependencies require a concrete implementation reason.
- Web input is untrusted by default.
- Compatibility claims require tests.
- Large future subsystems are not created as empty abstractions.

## Security

Do not report vulnerabilities in public issues. See [SECURITY.md](SECURITY.md).

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). Structural changes require an ADR when appropriate.

## License

Phantom core is licensed under the **Mozilla Public License 2.0 (MPL-2.0)**.
