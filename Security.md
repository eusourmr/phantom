# Security Policy

Phantom is a security-sensitive project. Treat all web content, IPC messages, extension inputs, media inputs, fonts, network responses, serialized state and agent-generated actions as untrusted.

## Reporting a vulnerability

Do **not** open a public issue for a suspected vulnerability.

Until a dedicated private vulnerability-reporting channel is configured, contact the repository owner privately through GitHub account contact mechanisms. A GitHub Security Advisory workflow should be enabled before the first public test release.

## Security principles

- memory-safe implementation by default,
- least privilege,
- capability-based access,
- process isolation,
- deny-by-default permissions,
- typed IPC,
- input validation at every trust boundary,
- auditable high-impact actions,
- reproducible releases as a project goal,
- supply-chain review and SBOM generation as release requirements.

## Current status

The foundation-stage code is not a production browser and has not undergone an independent security audit. No security guarantee should be inferred merely from the use of Rust.
