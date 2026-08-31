# Source Provenance & Dependency Inventory — 2D-6

## Baselines auditadas

| Arquivo | Git blob esperado |
|---|---|
| `crates/phantom-html/src/lib.rs` | `f2ca05db0b9b4217dc08ce1cccfff5b60be0668c` |
| `crates/phantom-dom/src/lib.rs` | `7b9b0af759434093e8639190364123b7b401ae1a` |
| `crates/phantom-css/src/lib.rs` | `924349c19e55589141069f80e7f21d4b61c8df69` |
| `crates/phantom-security/src/lib.rs` | `4ba4608971bb023163e4aabe774f78ab1cbb9283` |
| `.github/workflows/ci.yml` | `d5bad02fb5c0c3877ebba2d791fb674b919c4d1c` |
| `.github/workflows/release.yml` | `28335a699e5b37c8fa165a39286c599921682ebe` |

`CHECK-BASELINE-2D6.ps1` usa `git hash-object` localmente e para a instalação se qualquer baseline divergir.

A baseline CSS/CI acima foi confirmada diretamente no clone local em 2026-08-29 após o checker original bloquear a instalação.

## Toolchain

- Rust `1.95.0`
- cargo-audit `0.22.2`
- cargo-deny `0.20.2`
- cargo-auditable `0.7.5`

Todos os comandos Cargo de build/check/test/doc usam `--locked` quando aplicável.

## Actions imutáveis

- `actions/checkout`: `11d5960a326750d5838078e36cf38b85af677262`
- `actions/upload-artifact`: `ea165f8d65b6e75b540449e92b4886f43607fa02`
- `actions/download-artifact`: `d3f86a106a0bac45b974a628896c90dbdf5c8093`
- `actions/attest-build-provenance`: `977bb373ede98d70efdf65b84cb5f73e068dcc2a`

## Dependency inventory

A fonte canônica continua sendo `Cargo.lock`. O gate executa `cargo audit` e `cargo deny`. Releases são compiladas com `cargo auditable`, incorporando metadados de dependências no executável, e o job de publicação gera attestation de provenance para os artefatos publicados.

Para inspeção humana adicional:

```text
cargo metadata --locked --format-version 1
```

O spot-check anterior de `arrayref` não substitui estes gates automatizados.
