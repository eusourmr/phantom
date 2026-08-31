# PHANTOM — CONTINUIDADE APÓS 2E-1

## Baseline

- Security Gate 2D: PASS.
- 2D-6 homologada.
- `main` de referência ao iniciar 2E-1: `481750b`.
- `phantom-html` continua independente; nenhum Chromium/WebKit/Gecko/Servo é incorporado.
- Rust `unsafe` continua proibido.
- Budgets 2D-6 continuam contrato obrigatório.

## 2E-1 — Tokenizer Foundation

2E-1 introduz uma fronteira de tokenização separada em `phantom_html::tokenizer`.

Entregas:

- stream determinístico;
- posições de origem por byte;
- contabilidade ordenada de parse errors recuperáveis;
- estados Data / Tag / Attribute / Comment / Doctype;
- start/end tags;
- atributos ordenados e limitados;
- comentários e DOCTYPE básico;
- recuperação inicial para markup malformado;
- seam explícito para character references;
- testes adversariais e de regressão desde a fundação.

A função atual `parse()` ainda não consome o tokenizer. Isso é intencional para evitar uma migração grande e arriscada no mesmo passo.

## Arquivos 2E-1

- `crates/phantom-html/src/lib.rs` — exporta o novo módulo.
- `crates/phantom-html/src/tokenizer.rs` — implementação nova.
- `crates/phantom-html/tests/tokenizer_foundation.rs` — testes novos.
- `docs/HTML-2E1-TOKENIZER-FOUNDATION.md` — contrato técnico.
- `docs/PHANTOM-CONTINUIDADE-APOS-2E1.md` — continuidade.

Nenhuma dependência nova.

## Gate de homologação

Antes de marcar `2E-1 = PASS`:

1. `git apply --check` = PASS.
2. fmt = PASS.
3. check workspace/all-targets = PASS.
4. clippy `-D warnings` = PASS.
5. testes `tokenizer_foundation` = PASS.
6. testes `security_limits` = PASS.
7. testes workspace = PASS.
8. rustdoc `-D warnings` = PASS.
9. GitHub CI = PASS.
10. smoke do navegador permanece sem regressão observável.

## Próxima versão após PASS

### 2E-2 — Tree Builder Foundation

- consumir o stream de tokens 2E-1;
- separar definitivamente tokenização de construção da árvore;
- preservar budgets de DOM/nesting/text;
- insertion-mode subset explícito e pequeno;
- manter script sem execução;
- preservar style raw-text para CSS;
- testes diferenciais entre parser legado e novo caminho durante a transição.

Depois:

- 2E-3 — Error Recovery / Character References;
- 2E-4 — HTML Compatibility Suite / WPT subset.
