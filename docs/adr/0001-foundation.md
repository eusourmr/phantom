# ADR-0001: Phantom foundation architecture

- Status: Accepted
- Date: 2026-08-26

## Context

Phantom intends to be an independent web engine and browser rather than a fork of Chromium, WebKit or Gecko. The codebase must remain clean, auditable, modular, secure by default and suitable for long-term open-source development.

## Decision

1. Rust is the primary implementation language.
2. The workspace uses small domain-oriented crates with explicit dependency direction.
3. `unsafe` Rust is forbidden by default.
4. Composition and traits are preferred over inheritance-style designs.
5. Security-sensitive side effects are modeled through explicit capabilities.
6. Expected failures use typed errors.
7. The engine and browser are separate products; the engine must remain embeddable in principle.
8. Semantic understanding, memory and agent execution are separate from the DOM and may not bypass browser security policy.
9. MPL-2.0 is the initial core license.

## Consequences

- Some low-level platform integration may require dedicated audited crates later.
- More type definitions and adapters are expected in exchange for stronger invariants.
- Initial feature velocity may be lower than a Chromium fork, but architectural independence is preserved.
- Compatibility work must be implemented and tested rather than inherited wholesale from another browser engine.
