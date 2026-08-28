# Phantom 2C-8 — Network Isolation Key + Cache Partitioning

## Status

**IMPLEMENTAÇÃO GERADA — AGUARDANDO GATES E HOMOLOGAÇÃO.**

Base obrigatória: **2C-7 homologada, incluindo FIX 1 do Clippy Rust 1.95**.

## Objetivo

Transformar a privacidade do cache HTTP em uma propriedade estrutural do
runtime de rede antes de ampliar o cache para novos tipos de recurso ou disco.

Na 2C-7 a chave do cache binário era apenas a URL do recurso. Isso permitia que
um recurso de terceiro idêntico pudesse, em princípio, reutilizar estado entre
dois sites de topo diferentes dentro do mesmo processo.

A 2C-8 introduz uma `NetworkIsolationKey` explícita e altera a chave lógica para:

```text
(NetworkIsolationKey, resource URL)
```

## NetworkIsolationKey v1

A chave possui duas dimensões:

```text
top_level_origin
frame_origin
```

- `top_level_origin`: documento visível de topo;
- `frame_origin`: documento/frame que iniciou o subresource request.

No pipeline atual de imagens do Phantom ainda não existem browsing contexts de
iframe completos. Portanto, imagens diretamente pertencentes ao documento de
topo usam:

```text
top_level_origin == frame_origin
```

A API já nasce dupla para não exigir uma quebra arquitetural quando frames forem
introduzidos.

## Origem, não registrable site — decisão consciente

Esta versão usa **schemeful origin** (`scheme + host + effective port`) como
identificador, e não eTLD+1/registrable domain.

Motivo: o Phantom ainda não possui uma dependência de Public Suffix List na
camada de rede. Adicionar uma heurística caseira de domínio registrável seria
pior do que usar uma fronteira correta e mais restritiva.

Consequência:

- pode haver menos compartilhamento de cache entre subdomínios do mesmo site;
- não ocorre fusão indevida entre origens diferentes;
- uma futura migração para schemeful site pode ser feita dentro da construção
  da `NetworkIsolationKey`, sem mudar o contrato do cache.

## Cache partitioning

`BinaryHttpCache` deixa de usar:

```text
BTreeMap<String, CachedBinaryResponse>
```

para usar conceitualmente:

```text
BTreeMap<PartitionedCacheKey, CachedBinaryResponse>

PartitionedCacheKey {
    isolation_key,
    resource_url,
}
```

Toda operação é particionada:

- lookup;
- fresh hit;
- conditional revalidation;
- `304 Not Modified`;
- insertion;
- removal por `no-store`;
- stale-if-error recovery;
- LRU accounting.

O orçamento continua global e bounded. Partições distintas competem pelo mesmo
limite total de memória, mas nunca compartilham uma representação por chave.

## API

A 2C-8 adiciona:

```rust
NetworkIsolationKey::new(top_level_url, frame_url)
NetworkIsolationKey::from_top_level(top_level_url)
NetworkClient::fetch_bytes_partitioned(&isolation_key, &url)
```

A API binária anterior sem contexto explícito é removida nesta etapa. O caminho
cacheável passa a exigir `fetch_bytes_partitioned`, evitando que um consumidor
futuro contorne o boundary de privacidade por acidente.

Os testes da 2C-7 e o pipeline de imagens do browser são migrados para a nova
API explícita.

## Browser wiring

Cada `ImageLoadRequest` carrega a `NetworkIsolationKey` derivada do `base_url`
do documento que descobriu a imagem. Isso é importante porque workers de
imagem são assíncronos e o `NetworkClient` é compartilhado entre abas.

A chave viaja junto com o request, evitando depender de:

- variável global de aba ativa;
- thread-local implícito;
- último documento navegado;
- estado mutável compartilhado de contexto.

Esse desenho mantém concorrência entre abas sem misturar partições.

## O que permanece da 2C-7

Sem regressão intencional:

- cache `max-age` / `Age`;
- ETag / `If-None-Match`;
- Last-Modified / `If-Modified-Since`;
- 304;
- retry bounded;
- stale-if-error;
- must-revalidate;
- no-store/no-cache;
- Vary conservador;
- LRU por bytes e entradas;
- limite de corpo binário.

## Testes determinísticos

Novo integration test:

```text
crates/phantom-net/tests/cache_partitioning.rs
```

Ele prova:

1. mesma partição + mesma URL => `Miss` seguido de `Fresh`;
2. mesma URL em dois top-level origins => duas idas à rede e dois `Miss`;
3. mesmo top-level com dois frame origins => partições independentes.

Os testes usam servidor HTTP local efêmero. O teste de isolamento usa listener
non-blocking com deadline para falhar em vez de travar caso uma reutilização
cross-partition ocorra por regressão.

## Fora de escopo

Não entram na 2C-8:

- Public Suffix List / eTLD+1;
- cookies partitioned;
- DNS cache partitioning;
- connection pool partitioning;
- TLS session partitioning;
- HSTS partitioning;
- disk cache;
- cache de CSS/fontes/scripts;
- Service Workers;
- preload scheduler;
- HTTP/2 ou HTTP/3 próprios.

Esses subsistemas não devem reutilizar `NetworkIsolationKey` de forma parcial e
enganosa: quando forem implementados, a mesma chave deve atravessar o resource
request desde sua origem.

## Gates obrigatórios

Depois de aplicar o wiring do browser:

```powershell
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

## Critério de homologação

Marcar **2C-8 HOMOLOGADA** somente quando:

- todos os gates Rust passam;
- os testes de partição passam;
- release build passa;
- browser abre;
- imagens continuam carregando normalmente;
- navegação entre abas não produz regressão aparente.
