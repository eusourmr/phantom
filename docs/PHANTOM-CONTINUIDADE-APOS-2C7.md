# PHANTOM — CONTINUIDADE APÓS 2C-7

## Marco atual

**2C-7 — Image Recovery + HTTP Cache Revalidation**

Status ao gerar este pacote:

**IMPLEMENTADA / NÃO HOMOLOGADA AINDA.**

A homologação depende dos gates Rust no workspace oficial.

## Base técnica usada

HEAD consultado antes da geração:

```text
4d21f9141086483b3c655a006b42ea1ea3b0f5f0
```

Esse estado já continha o runtime de imagens com:

- lazy loading por viewport;
- deferred image requests;
- pausa de animação offscreen/inactive tab;
- document generation;
- cancelamento de trabalho stale.

A 2C-7 foi construída por cima desse estado sem mover responsabilidade para
DOM/Layout/Paint.

## Arquivos da 2C-7

```text
crates/phantom-net/src/lib.rs
crates/phantom-net/tests/http_cache.rs
docs/ENGINE-2C7-IMAGE-RECOVERY-HTTP-CACHE-REVALIDATION.md
docs/STANDARDS-MAP-HTTP-CACHE-REVALIDATION.md
docs/PHANTOM-CONTINUIDADE-APOS-2C7.md
```

Nenhum `Cargo.toml` ou `Cargo.lock` precisa ser substituído nesta etapa.

## O que entrou

- binary HTTP cache v1 in-memory e bounded;
- max-age + Age;
- ETag / If-None-Match;
- Last-Modified / If-Modified-Since;
- 304 revalidation;
- retry único para falhas transitórias;
- stale-if-error explícito;
- must-revalidate;
- Vary conservador;
- cache status tipado;
- deterministic local HTTP tests.

## Limites que permanecem explícitos

- sem cache partitioning;
- sem cache em disco;
- sem Expires/freshness heurística completa;
- sem stale-while-revalidate;
- sem cache genérico para todos os subresources;
- sem alegação de RFC 9111 integral.

## Gates

```powershell
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-net
cargo test -p phantom-net --test http_cache
```

Depois:

```powershell
taskkill /F /IM phantom-browser.exe
cargo build --release -p phantom-browser
.\target\release\phantom-browser.exe
```

## Critério de homologação

Somente marcar **2C-7 HOMOLOGADA** quando:

- fmt passa;
- clippy `-D warnings` passa;
- workspace tests passam;
- phantom-net tests passam;
- release build passa;
- browser abre e navegação/imagens continuam funcionais.

## Próximo risco arquitetural

Antes de ampliar o HTTP cache para fontes, CSS, scripts ou persistência, criar
uma `NetworkIsolationKey` e particionar o cache por contexto de top-level site.
Isso reduz superfície de tracking/timing cross-site e mantém privacidade como
propriedade estrutural do motor.
