# PHANTOM — CONTINUIDADE APÓS 2D-6

## Estado

2D-6 entrega Parser/Layout & Supply-Chain Security Gate, mas só deve ser homologada após todos os gates técnicos e de governança.

Não iniciar 2E enquanto `SECURITY GATE 2D = PASS` não estiver confirmado.

## Ordem de fechamento

1. `CHECK-BASELINE-2D6.ps1 = PASS` antes da instalação.
2. `git apply --check` do patch CSS = PASS.
3. fmt/check/clippy/test/doc = PASS.
4. testes adversariais 2D-6 = PASS.
5. cargo audit = PASS.
6. cargo deny = PASS.
7. release smoke com cargo-auditable = PASS.
8. GitHub CI = PASS.
9. ruleset `main` ativo.
10. ruleset de tags `v*` ativo.

Depois registrar:

`SECURITY GATE 2D = PASS`

## Próximo roadmap autorizado após PASS

### 2E-1 — Tokenizer Foundation

- deterministic token stream;
- source positions;
- parse-error accounting;
- data/tag/comment/doctype states;
- start/end tags;
- bounded attributes;
- character reference seam;
- budgets da 2D-6 permanecem contrato obrigatório;
- adversarial tests desde o início.

Depois: 2E-2 Tree Builder Foundation; 2E-3 Error Recovery / Character References; 2E-4 HTML Compatibility Suite / WPT subset.

A linha 2D não deve receber escopo novo salvo descoberta de bloqueador crítico durante os gates.
