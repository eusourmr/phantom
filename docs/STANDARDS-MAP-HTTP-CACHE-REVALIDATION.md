# Phantom — Standards Map: HTTP Cache Revalidation 2C-7

## Fontes normativas e técnicas

### HTTP Caching

- RFC 9111 — HTTP Caching
  - https://www.rfc-editor.org/rfc/rfc9111

### HTTP Semantics / Conditional Requests

- RFC 9110 — HTTP Semantics
  - https://www.rfc-editor.org/rfc/rfc9110

### Stale controls

- RFC 5861 — HTTP Cache-Control Extensions for Stale Content
  - https://www.rfc-editor.org/rfc/rfc5861

### Fetch integration

- WHATWG Fetch Standard
  - https://fetch.spec.whatwg.org/

## Mapeamento 2C-7

| Tema | 2C-7 | Observação |
|---|---|---|
| Cache key por URL | Parcial | URL absoluta + política restrita de `Vary` |
| `Cache-Control: max-age` | Sim | `Age` reduz freshness inicial |
| `no-store` | Sim | representação não permanece no cache |
| `no-cache` | Sim | força revalidação a cada uso |
| `must-revalidate` | Sim | impede fallback stale |
| ETag | Sim | preservado como validator |
| `If-None-Match` | Sim | enviado em entrada stale |
| Last-Modified | Sim | preservado como validator |
| `If-Modified-Since` | Sim | enviado em entrada stale |
| 304 | Sim | reutiliza body e atualiza metadata |
| `Vary: Accept` | Sim | request `Accept` é fixo |
| `Vary: *` | Sim, conservador | não armazena |
| outros `Vary` | Não | não armazena para evitar chave incorreta |
| `stale-if-error` | Sim | bounded e explicitamente autorizado |
| `stale-while-revalidate` | Não | adiado |
| `Expires` | Não | adiado |
| freshness heurística | Não | adiado |
| cache partitioning | Não | obrigatório antes de maturidade de privacidade |
| cache persistente | Não | adiado |

## Decisões de correção

### Validação antes de reutilização stale

RFC 9111 estabelece conditional validation como o mecanismo normal quando uma
representação armazenada não pode ser servida fresh. A 2C-7 segue essa direção
com ETag e Last-Modified.

### Stale não é fallback implícito

O Phantom não usa conteúdo stale apenas porque houve erro. A exceção desta fase
é `stale-if-error`, quando a origem explicitamente autorizou a janela e
`must-revalidate` não a proíbe.

### Vary conservador

RFC 9111 exige que os request fields nomeados em `Vary` participem da seleção da
representação. Como a 2C-7 ainda não possui uma chave genérica de variantes,
qualquer `Vary` além de `Accept` desabilita armazenamento.

### Sem alegação de conformidade integral

Esta implementação é uma fatia executável e auditável do cache HTTP, não uma
implementação completa de RFC 9111/WHATWG Fetch.

## Próxima extensão correta

A evolução natural é uma chave de isolamento explícita:

```text
NetworkIsolationKey
  + request URL
  + supported Vary dimensions
  -> CacheKey
```

Isso deve preceder expansão do cache para outros tipos de subresource e cache
persistente.
