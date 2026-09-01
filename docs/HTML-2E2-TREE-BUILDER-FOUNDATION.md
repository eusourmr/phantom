# Phantom 2E-2 — Tree Builder Foundation

Status: **candidate for local validation**

Baseline: `main` at `4c914cc525d1e11250368e34c667fd23bf728c2d`
Previous homologated milestone: **2E-1 — Tokenizer Foundation**

## Objective

2E-2 connects the deterministic token stream established in 2E-1 to a
standards-shaped, bounded DOM tree builder.

This milestone deliberately keeps the rendering-facing `phantom_html::parse`
compatibility path unchanged. The cutover is deferred until 2E-3 completes the
raw-text/RCDATA and character-reference semantics that the current rendering
path already depends on.

The result is a real parallel parser architecture without risking a regression
in the browser's currently homologated page rendering.

## Engine scope

The new `phantom_html::tree_builder` module adds:

- connected `tokenize -> tree builder -> Phantom DOM` pipeline;
- insertion-mode foundation:
  - Initial;
  - Before html;
  - Before head;
  - In head;
  - After head;
  - In body;
  - After body;
- deterministic implicit `html`, `head`, and `body` scaffolding;
- open-element stack;
- inherited 2D-6 nesting enforcement;
- inherited per-node and aggregate text limits;
- void-element handling;
- non-void self-closing recovery;
- initial paragraph implicit-close behavior;
- initial misnested-end-tag recovery;
- bounded structural diagnostics.

No Chromium, WebKit, Gecko, Servo, WebView, or external HTML parser is added.

## Structural intelligence

2E-2 also introduces `TreeBuildReport`.

The report is deterministic and passive. It does not authorize any behavior and
does not invoke AI. It records:

- tokens processed;
- DOM nodes created;
- maximum open-element depth;
- tokenizer parse-error count;
- deferred character-reference candidates;
- structural recovery count;
- bounded recovery diagnostics;
- implicit document scaffold decisions;
- DOCTYPE / quirks signal.

Recovery diagnostics are capped at 4,096 entries. Recovery continues after the
cap and the report sets a truncation flag, preventing hostile malformed HTML
from creating unbounded diagnostic memory.

## Browser / UX micro-evolution

The browser receives a small visible refinement instead of waiting for the
entire parser roadmap to finish:

- loading UI distinguishes network work from structural page construction;
- the floating navigation bar exposes a compact deterministic phase badge:
  - `NET` while fetching;
  - `DOM` while building;
  - `DOM <nodes>` for a committed page;
  - `ERR` after a failed navigation;
- the badge tooltip reuses the browser's existing status explanation;
- long loading addresses are visually bounded.

This remains browser chrome owned by Phantom and does not change page content.

## Security contract inherited from 2D-6

2E-2 must preserve:

- HTML source: 4 MiB;
- DOM nodes: 65,536 including root;
- nesting depth: 256 open elements;
- attributes: 128 per element;
- retained attribute bytes: 1 MiB per element;
- raw start tag: 2 MiB;
- text node: 1 MiB;
- aggregate retained text: 3 MiB;
- comment: 256 KiB;
- style body: 1 MiB in the existing compatibility parser.

The tokenizer remains authoritative for source/start-tag/attribute/comment
admission. The tree builder enforces the tree-specific depth and retained-text
budgets, while the DOM itself retains the node-count gate.

## Compatibility boundary

2E-2 is intentionally not the final WHATWG tree builder.

Deferred to 2E-3 and 2E-4:

- full malformed-markup recovery matrix;
- named and numeric character references;
- RCDATA;
- RAWTEXT;
- script-data states;
- complete insertion-mode coverage;
- adoption agency algorithm;
- active formatting elements;
- table foster parenting;
- template insertion-mode stack;
- foreign-content integration;
- broad WPT conformance.

This boundary prevents the 2E-2 foundation from pretending to be more
standards-complete than it is.

## Files

Repository paths changed by this candidate:

1. `crates/phantom-html/src/lib.rs`
2. `crates/phantom-html/src/tree_builder.rs`
3. `crates/phantom-html/tests/tree_builder_foundation.rs`
4. `crates/phantom-browser/src/main.rs`
5. `docs/HTML-2E2-TREE-BUILDER-FOUNDATION.md`
6. `docs/PHANTOM-CONTINUIDADE-APOS-2E2.md`

No dependency or lockfile changes are required.

## Local validation

Run:

```powershell
cargo fmt --all
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p phantom-html --test tree_builder_foundation --locked
cargo test -p phantom-html --test tokenizer_foundation --locked
cargo test -p phantom-html --test security_limits --locked
cargo test --workspace --locked

$env:RUSTDOCFLAGS="-D warnings"
cargo doc --workspace --no-deps --locked
Remove-Item Env:RUSTDOCFLAGS

cargo audit
cargo deny --locked --all-features check advisories bans sources
cargo build --release -p phantom-browser --locked
```

## Acceptance gate

2E-2 is PASS only after:

- patch applies cleanly to the 2E-1 merged baseline;
- rustfmt passes;
- workspace check passes;
- Clippy passes with warnings denied;
- new tree-builder tests pass;
- 2E-1 tokenizer tests remain green;
- 2D-6 HTML security tests remain green;
- workspace tests and rustdoc remain green;
- supply-chain gates remain green;
- native browser release build passes;
- visible browser smoke confirms the phase badge/loading copy;
- PR CI passes on Linux and Windows;
- protected `main` receives the squash/rebase-compatible integration.

Only then is 2E-2 homologated.
