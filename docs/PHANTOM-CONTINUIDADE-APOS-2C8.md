# PHANTOM — CONTINUIDADE APÓS 2C-8

## Marco atual

**2C-8 — Network Isolation Key + Cache Partitioning**

Status ao gerar este pacote:

**IMPLEMENTADA / NÃO HOMOLOGADA AINDA.**

Base: **2C-7 homologada + FIX 1 Clippy Rust 1.95**.

## O que entrou

- `NetworkIsolationKey` tipada;
- dupla dimensão top-level + frame;
- schemeful origin canônica via `url` crate;
- `PartitionedCacheKey`;
- lookup/store/remove/revalidation particionados;
- browser image requests carregam a chave junto ao request;
- `fetch_bytes_partitioned` explícito;
- testes locais contra reutilização cross-partition;
- sem nova dependência e sem alteração em Cargo.lock.

## Arquivos desta etapa

```text
crates/phantom-net/src/lib.rs
crates/phantom-net/tests/http_cache.rs
crates/phantom-net/tests/cache_partitioning.rs
docs/ENGINE-2C8-NETWORK-ISOLATION-CACHE-PARTITIONING.md
docs/STANDARDS-MAP-NETWORK-PARTITIONING.md
docs/PHANTOM-CONTINUIDADE-APOS-2C8.md
APPLY-2C8.ps1
```

`APPLY-2C8.ps1` faz apenas o wiring mínimo no arquivo existente:

```text
crates/phantom-browser/src/main.rs
```

Ele valida cada ponto antes de substituir e é idempotente para os trechos já
aplicados. Não substitui o browser inteiro.

## Por que origin-scoped

Ainda não há Public Suffix List no `phantom-net`. Nesta versão a NIK usa origem
schemeful, uma fronteira conservadora e mais fina. Não usar heurística artesanal
de eTLD+1.

## Gates

```powershell
powershell -ExecutionPolicy Bypass -File .\APPLY-2C8.ps1
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-net
cargo test -p phantom-net --test http_cache
cargo test -p phantom-net --test cache_partitioning
```

Depois:

```powershell
taskkill /F /IM phantom-browser.exe
cargo build --release -p phantom-browser
.\target\release\phantom-browser.exe
```

## Homologação

Somente marcar **2C-8 HOMOLOGADA** depois dos gates e do teste operacional do
browser.

## Próximo marco recomendado

Depois de tornar o cache seguro por contexto, o próximo incremento de rede pode
voltar ao ganho de performance:

**2C-9 — Resource Priority + Preload Scheduling**

Direção:

- prioridade por tipo/contexto;
- fila bounded;
- concorrência global/per-origin;
- deduplicação de requests em voo;
- promoção de recurso quando entra na viewport;
- primeiro preload crítico;
- sem HTTP/2/3 próprios nesta fase.
