# Phantom Engineering Documentation

This directory is the engineering record for Phantom: milestone notes, standards maps, architecture decisions, continuity snapshots and the static project page.

> Documentation is treated as code. Paths are case-sensitive on GitHub; use the exact filenames shown below.

## Start here

- [Manifesto](../Manifesto.md) — product thesis and human-control principles.
- [Architecture](../Architecture.md) — system boundaries and target process model.
- [Roadmap](../Roadmap.md) — high-level sequencing.
- [Coding Standard](../Coding.md) — mandatory implementation rules.
- [Project Directives](../Diretivas.md) — deeper engineering doctrine.
- [Security Policy](../Security.md) — reporting and security principles.
- [Contributing](../Contributing.md) — contribution and review expectations.
- [Engine Principles](ENGINE-PRINCIPLES.md) — engine-specific invariants.
- [Engineering Standards Doctrine](ENGINEERING-STANDARDS-DOCTRINE.md) — quality and process doctrine.

## Engine evolution

### 2B — style, layout and paint foundations

- [2B1.5 — CSS Consolidation](ENGINE-2B1.5-CSS-CONSOLIDATION.md)
- [2B2 — Layout Snapshot](ENGINE-2B2-LAYOUT-SNAPSHOT.md)
- [2B3 — Paint Pipeline](ENGINE-2B3-PAINT-PIPELINE.md)
- [2B4 — Inline Formatting](ENGINE-2B4-INLINE-FORMATTING.md)
- [2B5 — Text Shaping Boundary](ENGINE-2B5-TEXT-SHAPING-BOUNDARY.md)
- [2B6 — Box Model](ENGINE-2B6-BOX-MODEL.md)
- [2B7 — Flexbox Core](ENGINE-2B7-FLEXBOX-CORE.md)
- [2B8 — Flexbox Correctness](ENGINE-2B8-FLEXBOX-CORRECTNESS.md)
- [2B9 — Flexbox Multiaxis & Intrinsic](ENGINE-2B9-FLEXBOX-MULTIAXIS-INTRINSIC.md)
- [2B10 — Flexbox Final Core](ENGINE-2B10-FLEXBOX-FINAL-CORE.md)

### 2C — images, resources and network foundations

- [2C1 — Images / Replaced Elements](ENGINE-2C1-IMAGES-REPLACED.md)
- [2C2 — Image Fetch, Decode & Raster](ENGINE-2C2-IMAGE-FETCH-DECODE-RASTER.md)
- [2C3 — Responsive Images / Object Cache](ENGINE-2C3-RESPONSIVE-IMAGES-OBJECT-CACHE.md)
- [2C4 — Animated Image Timeline](ENGINE-2C4-ANIMATED-IMAGE-TIMELINE.md)
- [2C5 — Image Lifecycle / Lazy / Cancellation](ENGINE-2C5-IMAGE-LIFECYCLE-LAZY-CANCELLATION.md)
- [2C7 — Image Recovery / HTTP Cache Revalidation](ENGINE-2C7-IMAGE-RECOVERY-HTTP-CACHE-REVALIDATION.md)
- [2C8 — Network Isolation / Cache Partitioning](ENGINE-2C8-NETWORK-ISOLATION-CACHE-PARTITIONING.md)

## Standards and executable slices

- [Animated Images Standards Map](STANDARDS-MAP-ANIMATED-IMAGES.md)
- [HTTP Cache Revalidation Standards Map](STANDARDS-MAP-HTTP-CACHE-REVALIDATION.md)
- [Image Loading Standards Map](STANDARDS-MAP-IMAGE-LOADING.md)
- [Images Standards Map](STANDARDS-MAP-IMAGES.md)
- [Network Partitioning Standards Map](STANDARDS-MAP-NETWORK-PARTITIONING.md)
- [Responsive Images Standards Map](STANDARDS-MAP-RESPONSIVE-IMAGES.md)
- [WPT Flexbox Executable Slice](WPT-FLEXBOX-EXECUTABLE-SLICE.md)
- [WPT Flexbox Seed](WPT-FLEXBOX-SEED.md)

## Architecture decisions

Architecture Decision Records live under [`adr/`](adr/).

- [ADR-0001 — Foundation](adr/0001-foundation.md)

ADRs are append-only engineering history. Superseded decisions should be marked as such rather than deleted.

## Continuity snapshots

Continuity documents preserve enough technical context to resume a milestone without reconstructing decisions from chat history.

- [After 2C4](PHANTOM-CONTINUIDADE-APOS-2C4.md)
- [After 2C7](PHANTOM-CONTINUIDADE-APOS-2C7.md)
- [After 2C8](PHANTOM-CONTINUIDADE-APOS-2C8.md)

Older implementation notes that no longer belong in the repository root are preserved under [`history/`](history/).

## Project page

`index.html` and `styles.css` contain the static project page source. The page is intentionally framework-free and can be published with GitHub Pages when repository settings enable Pages from `/docs`.
