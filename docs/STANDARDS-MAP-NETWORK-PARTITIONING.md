# Standards Map — Network State Partitioning — Phantom 2C-8

## Princípio

Caches e outros estados de rede reutilizáveis podem formar canais de tracking
cross-site quando são compartilhados apenas pela URL/origem do recurso.
Navegadores modernos particionam estado de rede usando contexto adicional do
site de topo.

## Referências de engenharia

### Mozilla / MDN — State Partitioning

A documentação de State Partitioning descreve network partitioning e lista o
HTTP Cache e Image Cache entre os estados permanentemente particionados por
contexto de top-level site.

Referência:

```text
https://developer.mozilla.org/en-US/docs/Web/Privacy/Guides/State_Partitioning
```

### Chromium — Site / network isolation direction

A arquitetura Chromium usa o conceito de site/contexto como boundary de
isolamento para dados e recursos do browser. O Phantom não copia a arquitetura
do Chromium; a referência serve apenas para confirmar a propriedade de
segurança que um browser moderno precisa preservar.

Referências:

```text
https://www.chromium.org/Home/chromium-security/site-isolation/
https://www.chromium.org/developers/design-documents/site-isolation/
```

### WHATWG URL / rust-url

A serialização de origem da `url` crate segue o modelo WHATWG de origin. A 2C-8
usa essa serialização como boundary v1 enquanto o Phantom ainda não possui uma
Public Suffix List.

## Mapeamento Phantom

```text
Top-level document URL
        ↓
NetworkIsolationKey
  ├─ top_level_origin
  └─ frame_origin
        ↓
PartitionedCacheKey
  ├─ NetworkIsolationKey
  └─ resource_url
        ↓
BinaryHttpCache
```

## Regra permanente

Nenhum subsistema futuro deve inventar sua própria chave paralela de
partitioning quando `NetworkIsolationKey` já carregar o contexto necessário.
A chave deve ser propagada pelo request/coordinator e consumida pela camada de
rede, sem depender de estado global da aba ativa.

## Limite desta versão

`origin` é uma fronteira mais fina que `schemeful site`. Essa diferença é
explícita. O Phantom não declara equivalência integral com Firefox/Chromium nem
conformidade com uma especificação de storage partitioning ainda em evolução.
