# Contributing to Phantom

Phantom welcomes contributions that preserve the project's architectural, security and auditability constraints.

## Before coding

1. Read the [Manifesto](Manifesto.md).
2. Read the [Architecture](Architecture.md).
3. Read the [Coding Standard](Coding.md).
4. Review the deeper [Project Directives](Diretivas.md) when changing engine boundaries, dependencies or security-sensitive code.
5. Keep changes within an existing responsibility boundary whenever possible.
6. For structural changes, add or update an ADR under [`docs/adr/`](docs/adr/) before or with the implementation.

## Required checks

Run the quality gate from the repository root:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

## Pull requests

A pull request should explain:

- the problem being solved,
- the design and boundary affected,
- verification and test evidence,
- security impact,
- compatibility impact,
- dependency or supply-chain impact, when applicable,
- follow-up work, if any.

Security-critical changes should be intentionally small and reviewed separately from unrelated refactors.

## Review standard

A change is not complete only because it compiles. Reviewers should be able to answer:

- Which invariant does this change preserve or introduce?
- Which inputs are untrusted?
- Which failure modes are bounded?
- Which tests prove the intended behavior?
- Can the change be removed or replaced without rewriting unrelated subsystems?

## Security reports

Do not disclose suspected vulnerabilities in public issues. Follow the [Security Policy](Security.md).
