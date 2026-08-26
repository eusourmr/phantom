# Contributing to Phantom

Phantom welcomes contributions that preserve the project's architectural and security constraints.

## Before coding

1. Read `MANIFESTO.md`.
2. Read `ARCHITECTURE.md`.
3. Read `CODING_STANDARD.md`.
4. Keep changes within an existing responsibility boundary whenever possible.
5. For structural changes, add an ADR under `docs/adr/` before or with the implementation.

## Required checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

## Pull requests

A pull request should contain:

- problem statement,
- design summary,
- testing evidence,
- security impact,
- compatibility impact,
- follow-up work, if any.

Security-critical changes should be intentionally small and reviewed separately from unrelated refactors.
