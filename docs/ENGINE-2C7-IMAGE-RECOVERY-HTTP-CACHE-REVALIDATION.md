# Phantom 2C-7 — Image Recovery + HTTP Cache Revalidation

## Status

**IMPLEMENTAÇÃO GERADA — AGUARDANDO GATES E HOMOLOGAÇÃO.**

A 2C-7 não deve ser marcada como homologada antes da execução dos gates no
workspace Rust 1.95 do Phantom.

## Objetivo

Tornar o carregamento de imagens mais resiliente sem deslocar semântica de
rede para DOM, Layout, Paint ou decoder.

A etapa adiciona ao `phantom-net`:

- cache HTTP binário em memória e bounded;
- freshness por `Cache-Control: max-age`;
- desconto de `Age` sobre a janela de freshness;
- revalidação por `ETag` / `If-None-Match`;
- revalidação por `Last-Modified` / `If-Modified-Since`;
- reutilização do corpo em `304 Not Modified`;
- retry único e bounded para falhas transitórias;
- `stale-if-error` somente quando explicitamente autorizado pela origem;
- bloqueio de fallback stale quando `must-revalidate` estiver presente;
- política conservadora para `Vary`;
- LRU bounded por bytes e número de entradas.

## Boundary arquitetural

A decisão permanece em Network/Resource Runtime:

```text
Browser Resource Coordinator
        ↓
phantom-net
  ├─ transport
  ├─ validators
  ├─ cache policy
  ├─ bounded retry
  └─ bounded binary cache
        ↓
phantom-image decoder
        ↓
texture/runtime
```

Não foi adicionado estado HTTP ao DOM, LayoutSnapshot ou PaintList.
O decoder continua recebendo apenas bytes e limites de decode.

## Contrato preservado

O browser continua usando:

```rust
NetworkClient::fetch_bytes(&HttpUrl)
```

A API existente foi mantida para evitar retrabalho no shell e no pipeline de
imagens. `BinaryResponse::body()` continua expondo `&[u8]`.

Foi acrescentado `CacheStatus` para diagnóstico tipado:

- `Miss` — resposta veio da rede;
- `Fresh` — representação fresh veio do cache;
- `Revalidated` — origem respondeu 304 e o corpo anterior foi reutilizado;
- `StaleIfError` — fallback stale autorizado pela origem após falha recuperável.

## Política de cache desta versão

### Limites default

- corpo binário por resposta: **16 MiB**;
- cache binário agregado: **64 MiB**;
- máximo: **128 entradas**;
- eviction: LRU simples e determinístico;
- memória compartilhada somente dentro do processo atual.

Uma resposta individual maior que o orçamento do cache pode ser entregue ao
consumidor dentro do limite de resposta, mas não é inserida no cache agregado.

### Cache-Control reconhecido

Nesta etapa:

- `no-store`;
- `no-cache`;
- `must-revalidate`;
- `max-age=N`;
- `stale-if-error=N`.

Diretivas desconhecidas são ignoradas, sem inventar semântica.

### Vary

A requisição de imagem usa um `Accept` fixo controlado pelo Phantom. Por isso:

- sem `Vary`: cache permitido quando as demais regras permitem;
- `Vary: Accept`: permitido;
- `Vary: *`: não armazenar;
- qualquer outro campo em `Vary`: não armazenar nesta versão.

Essa decisão evita reutilizar representação para uma chave que o runtime ainda
não sabe reproduzir corretamente.

## Revalidação

Quando a entrada deixa de ser fresh:

1. se existir `ETag`, enviar `If-None-Match`;
2. se existir `Last-Modified`, enviar `If-Modified-Since`;
3. se a origem responder `304`, manter os bytes anteriores;
4. atualizar validators/metadados aplicáveis;
5. reiniciar o tempo de armazenamento;
6. aplicar a nova política quando o 304 trouxer `Cache-Control` ou `Vary`.

Um `304` sem representação previamente armazenada é erro tipado
`NetworkError::UnexpectedNotModified`.

## Image Recovery

A recuperação é deliberadamente pequena e bounded.

### Retry

No máximo **2 tentativas totais** para uma requisição binária.

Uma segunda tentativa é permitida para:

- falha de transporte;
- HTTP 408;
- HTTP 500;
- HTTP 502;
- HTTP 503;
- HTTP 504.

Não existe loop infinito, backoff oculto ou worker permanente.

### stale-if-error

Após a tentativa final, uma representação stale só pode ser usada quando:

- a origem declarou `stale-if-error=N`;
- a representação ainda está dentro dessa janela;
- `must-revalidate` não está presente;
- a falha é de transporte ou HTTP 500/502/503/504.

HTTP 408 pode disparar retry, mas não foi tratado como autorização para
`stale-if-error` nesta implementação conservadora.

## Segurança e privacidade

- cache exclusivamente em memória;
- nenhum cache persistente em disco;
- sem cookies, credenciais ou Authorization adicionados por esta etapa;
- credentials embutidas em URL continuam rejeitadas;
- redirects continuam revalidados pelo boundary `HttpUrl`;
- limites de corpo continuam obrigatórios;
- `no-store` remove a representação correspondente;
- cache lock envenenado degrada para network path, sem `panic!`.

### Limitação conhecida: partitioning

A 2C-7 **ainda não implementa cache partitioning por top-level site**.
O cache binário pertence ao `NetworkClient` compartilhado no processo.

Por isso esta etapa deve ser descrita como **HTTP cache revalidation v1**, e não
como cache HTTP final/privacy-complete. Antes da estabilização do subsistema de
rede, o Phantom deve introduzir uma `NetworkIsolationKey` explícita e incluir a
partição na cache key.

## Fora de escopo

Não entram na 2C-7:

- cache persistente em disco;
- `Expires` / freshness heurística por `Date` + `Last-Modified`;
- `s-maxage`, `proxy-revalidate` e semântica de shared proxy cache;
- `stale-while-revalidate` assíncrono;
- cache de documento HTML;
- cache de CSS/fontes/scripts;
- Range requests / partial representations;
- cache partitioning por top-level site;
- Service Worker Cache API;
- prefetch/prerender;
- retry ilimitado.

## Testes determinísticos adicionados

`crates/phantom-net/tests/http_cache.rs` usa servidor HTTP local efêmero e cobre:

1. fresh cache evita segunda ida à rede;
2. entrada stale com ETag envia `If-None-Match` e aceita 304;
3. 503 recebe um único retry e pode recuperar com 200;
4. falha de transporte usa `stale-if-error` quando autorizado;
5. `must-revalidate` bloqueia fallback stale.

Também há unit tests para parsing da política e `Vary`.

## Gates obrigatórios

```powershell
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-net
cargo test -p phantom-net --test http_cache
```

Se todos passarem:

```powershell
taskkill /F /IM phantom-browser.exe
cargo build --release -p phantom-browser
.\target\release\phantom-browser.exe
```

## Homologação visual/operacional sugerida

Após os gates:

- abrir página pública com múltiplas imagens;
- confirmar carregamento normal na primeira visita;
- reload da mesma página e observar ausência de regressões;
- validar imagens com ETag/Last-Modified em servidor de teste quando possível;
- simular indisponibilidade temporária de uma imagem cacheável com
  `stale-if-error` e confirmar recuperação;
- navegar entre abas e confirmar que o runtime permanece responsivo.

Somente depois disso: **2C-7 HOMOLOGADA**.
