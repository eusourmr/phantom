# Phantom — Continuidade após 2C-15

## 2C status

After homologation, **2C is frozen as complete**.

Do not continue appending unrelated capabilities to 2C. Regression fixes may be
2C-15 FIX releases, but new architecture begins in 2D.

## Next line: 2D

### 2D-1 — Document Loader Hardening
- MIME acceptance/rejection policy;
- charset/BOM/content-type resolution;
- document byte and decoded-text limits;
- unsupported/non-HTML response handling;
- error-page contract.

Small browser increment:
- clear, minimal unsupported-document/error presentation using the existing
  content surface, not another toolbar.

### Then
- 2D-2 URL/Origin Domain Consolidation
- 2D-3 Navigation State Machine
- 2D-4 Navigation Compatibility Suite

## Architecture gates retained

- no custom replacement for rust-url;
- no DOM move into phantom-core;
- generational/revocable DOM handles before persistent JS bindings;
- WPT adoption remains incremental and measurable;
- GPU, QUIC and tracing enter only when their owning milestone requires them.
