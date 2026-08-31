# Phantom — Continuidade após 2D-4

With 2D-4 validated, the 2D foundation is frozen:

- 2D-1 Document Loader Hardening
- 2D-2 URL/Origin Domain Consolidation
- 2D-3 Navigation State Machine
- 2D-4 Navigation Compatibility Suite

Future changes should preserve this suite unless a deliberate contract change
is documented.

Next: 2E-1 — Tokenizer Foundation.

The 2E line begins HTML parser maturation: deterministic token stream,
doctype/comments/text/start/end tags, attribute hardening, parse-error
recording and focused compatibility tests. Browser chrome should remain stable
while tokenizer infrastructure matures.
