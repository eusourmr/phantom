# Phantom Engine 2B-6 — CSS Box Model + Borders + min/max constraints

## Objetivo

Consolidar a geometria CSS básica antes do primeiro Flexbox próprio.

O pipeline permanece:

HTML
  ->
DOM
  ->
Computed Style
  ->
LayoutSnapshot
  ->
PaintList
  ->
Renderer

A diferença desta etapa é que `width` e `height` deixam de ser tratados
como dimensões isoladas. O Layout passa a trabalhar explicitamente com:

- content box
- padding box
- border box
- margin
- min/max constraints

## Box sizing

Suportado:

- `box-sizing: content-box`
- `box-sizing: border-box`

### content-box

`width` e `height` representam a caixa de conteúdo.

A dimensão externa inclui:

content + padding + border

### border-box

`width` e `height` representam a border box.

O conteúdo disponível é calculado subtraindo:

padding + border

## Borders

Primeiro subconjunto:

- `border`
- `border-width`
- `border-top-width`
- `border-right-width`
- `border-bottom-width`
- `border-left-width`
- `border-color`
- `border-style`

Estilos visuais suportados nesta etapa:

- `none`
- `solid`

A largura inicial interna é equivalente a um `medium` simplificado de 3 px,
mas `border-style: none` produz largura geométrica efetiva zero.

`border-color` usa o foreground `color` quando não existe cor explícita,
aproximando `currentColor`.

A PaintList continua renderer-independent. Bordas sólidas são decompostas
em retângulos simples, uma representação adequada para batching futuro na GPU.

## Constraints

Suportado:

- `min-width`
- `max-width`
- `min-height`
- `max-height`

Para largura:

- px
- em
- rem
- %
- auto/none conforme o contexto

Percentuais de largura são resolvidos contra a containing block.

Para altura, percentuais continuam deliberadamente não resolvidos enquanto
o motor não possui uma containing-height model completa. Isso evita inventar
semântica incorreta.

## Cold Layout Snapshot

`LayoutBox` passa a carregar também:

- margin
- padding
- border

A geometria `rect` continua representando a border box.

Nenhum ponteiro para DOM é introduzido.

## Paint

`phantom-paint` passa a desenhar:

1. background
2. bordas sólidas
3. texto

As bordas são geradas a partir da geometria fria do LayoutSnapshot.

Paint continua sem depender de HTML ou DOM.

## Testes adicionados

- content-box soma padding + border à largura externa
- border-box preserva a largura externa declarada
- max-width limita width
- min-height limita height
- border:none remove largura efetiva
- cascade do box model
- current border color / border shorthand

## Ainda fora desta etapa

- margin collapsing
- border-radius
- dashed/dotted/double
- bordas individuais com estilos e cores diferentes
- inline fragment borders
- outline
- intrinsic sizing
- min-content/max-content
- fit-content
- height percentage completo
- aspect-ratio
- replaced elements
- Flexbox real

## Próxima etapa recomendada

2B-7 — Flexbox Core v1

Começar somente com:

- `display:flex`
- `flex-direction: row | column`
- `justify-content`
- `align-items`
- `gap`
- `flex-grow`
- `flex-shrink`
- `flex-basis`

Sem Grid, order avançado ou baseline complexo no primeiro corte.
