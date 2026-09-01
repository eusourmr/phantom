# Phantom — Continuidade após 2E-2

## Source of truth

This file becomes the continuity handoff **after 2E-2 is homologated**.

Baseline entering 2E-2:

- Security Gate 2D: PASS
- 2D-6: homologated
- 2E-1 Tokenizer Foundation: merged to protected `main`
- 2E-1 merge commit on `main`: `4c914cc525d1e11250368e34c667fd23bf728c2d`

## Cadence rule from 2E-2 onward

Each milestone should remain small but evolve three product layers together:

1. **engine** — the milestone's primary technical objective;
2. **internal intelligence** — deterministic observation/explanation of engine state;
3. **browser experience** — one restrained, visible UX/design refinement when it
   can be added without destabilizing the primary objective.

Security and compatibility gates remain cumulative.

This is not permission to inflate releases. The primary objective stays narrow;
the other two layers advance in small, auditable increments.

## 2E-2 deliverables

### Engine

- token stream consumed by an independent tree-builder module;
- initial insertion-mode state machine;
- explicit/implicit html/head/body construction;
- bounded open-elements stack;
- void-element handling;
- paragraph implicit close;
- first misnested-end-tag recovery;
- inherited tree budgets.

### Internal intelligence

- `TreeBuildReport`;
- bounded structural recovery diagnostics;
- maximum open-elements observation;
- implicit scaffold observations;
- tokenizer error/reference seam propagated into the structural report.

The report is passive and deterministic. It grants no permission and invokes no
AI.

### Browser experience

- deterministic navigation-phase copy;
- compact `NET` / `DOM` / `ERR` phase badge in the floating navigation surface;
- committed DOM node count exposed without adding permanent browser chrome;
- bounded address display during loading.

## Important compatibility decision

The current rendering-facing `phantom_html::parse` remains the active parser in
2E-2.

This is intentional, not unfinished wiring.

The new 2E-1 -> 2E-2 pipeline is connected and directly testable through
`tree_builder::tokenize_and_build`, but it is not yet used to render arbitrary
web pages because 2E-1 does not implement RAWTEXT/RCDATA/script-data or
character-reference resolution.

Switching the browser prematurely would regress sites that the existing parser
already handles.

The cutover is reconsidered in 2E-3 once those semantics exist.

## Next milestone

### 2E-3 — Error Recovery + Character References

Primary engine work:

- named/numeric character-reference resolution;
- data vs attribute context rules;
- RCDATA foundation;
- RAWTEXT/script-data foundation;
- stronger malformed-tag recovery;
- connect those semantics to the 2E-2 tree builder;
- evaluate migration of `phantom_html::parse` to the new pipeline.

Internal intelligence:

- recovery categories become more standards-specific;
- report unresolved/invalid references and raw-text transitions under bounded
  diagnostics.

Browser experience:

- one small error/recovery transparency improvement;
- continue micro-polish without adding a large new UI surface.

### 2E-4 — HTML Compatibility Suite / WPT subset

- curated WHATWG/WPT oracle cases;
- compatibility score/profile;
- regression corpus;
- cutover hardening;
- real-page compatibility validation.

## Non-negotiable constraints

- independent Rust engine;
- no Chromium/WebKit/Gecko/WebView;
- no `unsafe` in Phantom;
- no weakening of 2D-6 budgets to gain compatibility;
- no AI authority;
- deterministic security before intelligence;
- local-first posture;
- no speculative dependencies;
- small auditable diffs;
- protected-main PR workflow with linear history.
