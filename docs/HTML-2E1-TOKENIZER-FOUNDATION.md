# Phantom 2E-1 — Tokenizer Foundation

## Status

Implementation candidate for validation on top of the homologated 2D-6 baseline (`main` at `481750b` when this package was prepared).

2E-1 does **not** replace the existing DOM-producing `parse()` path yet. It adds the deterministic tokenization boundary that 2E-2 will consume. This keeps the 2D-6 behavior stable while the standards-shaped parser architecture is introduced incrementally.

## Scope

2E-1 adds `phantom_html::tokenizer` with:

- deterministic ordered token stream;
- byte-accurate source spans;
- recoverable parse-error accounting;
- explicit tokenizer state vocabulary;
- character data;
- start and end tags;
- ordered attributes;
- comments;
- basic DOCTYPE tokenization;
- malformed tag/comment recovery seams;
- duplicate-attribute handling;
- NUL replacement accounting;
- character-reference candidates with data/attribute context;
- EOF token;
- inherited 2D-6 admission budgets.

No external parser and no new dependency are introduced.

## Security contract

The 2D-6 budgets remain authoritative.

Tokenizer-enforced admission limits:

- HTML source: 4 MiB;
- raw start tag: 2 MiB;
- attributes scanned per element: 128;
- aggregate normalized attribute bytes per element: 1 MiB;
- comment body: 256 KiB.

The remaining 2D-6 budgets are intentionally enforced by the layers that retain or construct their corresponding resources:

- DOM node count and nesting depth: tree construction;
- ordinary retained text and aggregate retained text: tree construction;
- style body: parser/tree integration;
- CSS budgets: CSS layer.

This separation prevents the tokenizer from rejecting bytes that later states may intentionally ignore (for example script data) merely because they are not DOM-retained text.

Fatal resource-budget violations return `TokenizerError`. Malformed but bounded HTML is represented by ordered `ParseError` values and continues deterministically where the 2E-1 recovery subset defines a path.

## Character-reference seam

2E-1 does not resolve `&...` references. It records every candidate ampersand with:

- exact source span;
- `Data` or `Attribute` context.

The original characters remain in the emitted token value. 2E-3 can therefore add character-reference resolution without changing the token source-position model.

## Compatibility boundary

The current `parse()` function remains unchanged except for exporting the tokenizer module.

This is deliberate:

1. 2E-1 validates tokenization independently.
2. 2E-2 moves token-to-tree construction onto this stream.
3. 2E-3 expands error recovery and character references.
4. 2E-4 adds the curated HTML compatibility/WPT subset.

## Validation

Run after applying the package:

```text
cargo fmt --all
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p phantom-html --test tokenizer_foundation --locked
cargo test -p phantom-html --test security_limits --locked
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

On PowerShell, set `RUSTDOCFLAGS` before the final command:

```powershell
$env:RUSTDOCFLAGS="-D warnings"
cargo doc --workspace --no-deps --locked
```

## Acceptance criteria

2E-1 can be homologated when:

- the patch applies cleanly to the homologated 2D-6 baseline;
- existing parser/security tests remain green;
- tokenizer foundation tests pass;
- full fmt/check/clippy/test/doc gates pass;
- GitHub PR CI passes;
- no 2D-6 security budget is weakened;
- no existing `parse()` compatibility behavior regresses.

## Next

`2E-2 — Tree Builder Foundation`

The next step consumes the 2E-1 token stream and separates tokenization from DOM tree construction while preserving the existing bounded behavior.
