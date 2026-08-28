# Phantom Coding Standard

## Mandatory rules

1. Prefer small, single-purpose modules.
2. Prefer domain types to raw strings, integer flags and boolean switches.
3. Prefer composition and traits to inheritance-style design.
4. Keep dependencies explicit; hidden mutable global state is prohibited.
5. `unsafe` is prohibited unless a dedicated low-level crate is approved by architecture and security review.
6. Do not use `unwrap()`, `expect()`, `panic!()`, `todo!()` or `unimplemented!()` in production paths.
7. Expected failures use typed errors and `Result`.
8. Public APIs document invariants, errors, side effects and security implications.
9. Privileged side effects require explicit dependencies or capabilities.
10. Inputs crossing trust boundaries are validated before use.
11. Security-sensitive code requires focused tests and threat-model notes.
12. Circular crate dependencies are forbidden.
13. A change should be easy to remove or replace without rewriting unrelated subsystems.
14. Cleverness is not a substitute for clarity.

## State modeling

Prefer enums and private fields to stringly typed state.

## Errors

Errors are part of the API contract. Core APIs must not use `Result<T, String>`.

## Comments

Comments explain why, invariants, threat assumptions or non-obvious constraints. They do not narrate obvious syntax.

Any future unsafe block must include a `// SAFETY:` comment with the complete safety argument.

## Reviewability

Every meaningful change should answer:

- What changed?
- Why is it necessary?
- Which invariant or behavior is affected?
- How was it verified?
- What is the security impact?
