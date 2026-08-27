# Phantom Engine 2B-7 — Flexbox Core v1

## Objective

Introduce Phantom's first real Flex Formatting Context while keeping the
engine architecture small, explicit and auditable.

The pipeline remains:

HTML
  ->
DOM
  ->
ComputedStyle
  ->
LayoutSnapshot
  ->
PaintList
  ->
Renderer

Flexbox is implemented inside `phantom-layout`.

Paint does not calculate Flexbox.

## CSS properties

Implemented in the first core:

- `display: flex`
- `flex-direction: row`
- `flex-direction: column`
- `justify-content: flex-start`
- `justify-content: center`
- `justify-content: flex-end`
- `justify-content: space-between`
- `align-items: stretch`
- `align-items: flex-start`
- `align-items: center`
- `align-items: flex-end`
- `gap`
- `flex-grow`
- `flex-shrink`
- `flex-basis`

Basic `flex` shorthand support:

- `flex: none`
- `flex: auto`
- `flex: initial`
- `flex: <grow>`
- `flex: <grow> <shrink>`
- `flex: <grow> <basis>`
- `flex: <grow> <shrink> <basis>`

## Architecture

No new pointer-heavy Flexbox tree is introduced.

The algorithm works directly with short-lived planning records and emits the
existing cold `LayoutSnapshot`.

Temporary planning data is owned by one layout pass and discarded afterward.

Hot persistent output remains:

- `Vec<LayoutBox>`
- compact `LayoutId`
- shared text storage
- numeric geometry

## Two-pass flex layout

A flex container is processed in two conceptual passes.

### Pass 1 — measurement and allocation

Phantom:

1. collects direct visible flex items;
2. calculates the initial main-axis basis;
3. accounts for margins and gap;
4. calculates positive or negative free space;
5. distributes space using grow or shrink;
6. lays out item content.

### Pass 2 — final placement

After the container's final content size is known, Phantom:

1. applies `justify-content`;
2. applies `align-items`;
3. positions item subtrees;
4. applies cross-axis stretching where supported.

This lets an auto-height container be measured before its final alignment is
applied.

## Row behavior

The row algorithm supports:

- intrinsic fallback width for `width:auto`;
- explicit width;
- `flex-basis`;
- grow distribution;
- shrink distribution;
- min/max-width interaction;
- margin participation;
- gap;
- main-axis justification;
- cross-axis alignment.

## Column behavior

The column algorithm supports:

- natural item height;
- explicit height;
- `flex-basis` in logical pixels;
- grow and shrink;
- min/max-height constraints;
- gap;
- vertical justification;
- horizontal alignment.

Percentage flex-basis on the column main axis deliberately falls back to
natural sizing until Phantom has a complete containing-height percentage
model.

## Direct text children

Non-empty text directly inside a flex container becomes a lightweight
anonymous flex item.

Whitespace-only direct text nodes are ignored.

This prevents common simple flex markup from silently losing text without
creating a permanent anonymous object graph.

## Sustainability rules

Flexbox Core v1 deliberately does not introduce:

- a new DOM representation;
- mutable shared layout state;
- renderer-side Flexbox calculations;
- CSS parsing inside Layout;
- DOM access inside Paint;
- recursion through pointer-owned Flexbox objects.

## Tests added

The source includes tests for:

- computed Flexbox properties;
- basic `flex` shorthand expansion;
- centered row placement with gap;
- row `flex-grow`;
- row `flex-shrink`;
- column justification;
- column cross-axis alignment.

## Intentionally deferred

The first core does NOT claim complete CSS Flexbox compatibility.

Deferred to 2B-8 or later:

- `row-reverse`
- `column-reverse`
- `flex-wrap`
- `flex-flow`
- `align-content`
- `align-self`
- `order`
- auto margins in Flexbox
- baseline alignment
- min-content / max-content
- fit-content
- complete percentage-height resolution
- exact CSS intrinsic sizing algorithm
- replaced element intrinsic sizing
- advanced overflow interactions
- full WPT conformance

## Next milestone

2B-8 — Flexbox Correctness + Wrapping

Recommended scope:

- `flex-wrap`
- multiple flex lines
- `align-content`
- `align-self`
- reverse directions
- stronger intrinsic sizing
- nested Flexbox regression corpus
- first focused Flexbox WPT subset

The priority remains correctness before breadth.
