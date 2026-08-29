# Phantom — Continuidade após 2D-5

When 2D-5 passes the Rust 1.95 gates, Security Gate findings SG-001 through
SG-004 are considered mitigated for the current Alpha architecture.

The next milestone remains:

## 2D-6 — Parser/Layout & Supply-Chain Security Gate

Required scope:

- parser/DOM/CSS/layout structural budgets;
- depth/attribute/text/comment limits;
- remove raw-text O(n²)-style lowercasing scans;
- CSS numeric magnitude/finite policy;
- adversarial parser/layout corpus;
- fuzz targets/seeds;
- branch protection/ruleset plan;
- CI least privilege;
- pinned GitHub Action SHAs;
- `--locked` gates;
- cargo-audit/cargo-deny;
- SBOM/attestation foundation.

Only after 2D-6 passes should Security Gate 2D be marked PASS and 2E-1 begin.
