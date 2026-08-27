# Phantom Engine 2B-8 — Flexbox Correctness + Wrapping

## Objective

Mature the first Phantom Flex Formatting Context without widening the engine
faster than its contracts can remain testable and auditable.

The active architecture remains:

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

Flexbox stays entirely inside `phantom-layout`.

Paint does not calculate Flexbox and does not traverse the DOM.

## New CSS surface

Added:

- `flex-direction: row-reverse`
- `flex-direction: column-reverse`
- `flex-wrap: nowrap`
- `flex-wrap: wrap`
- `align-content`
- `align-self`

`align-content` subset:

- `stretch`
- `flex-start`
- `center`
- `flex-end`
- `space-between`

`align-self` subset:

- `auto`
- `stretch`
- `flex-start`
- `center`
- `flex-end`

## Horizontal wrapping

Row and row-reverse flex containers can now create multiple flex lines.

The line-building sequence is:

1. collect visible direct flex items;
2. calculate hypothetical main-axis basis;
3. include physical margins and `gap`;
4. split items into line ranges;
5. run grow/shrink independently for each line;
6. lay out item subtrees;
7. measure each line cross size;
8. resolve container cross size;
9. distribute lines with `align-content`;
10. position each item with `align-items` or `align-self`.

Temporary line-planning data is discarded after layout.

The persistent cold output remains `Vec<LayoutBox>` plus compact IDs and shared
text storage.

## Reverse main axes

`row-reverse` positions the first source-order item from the right main-start
edge.

`column-reverse` positions the first source-order item from the bottom
main-start edge.

The DOM order is not mutated.

The cold layout tree remains source-oriented while geometry records the
reversed visual placement.

## align-self

Each flex item can override the container's `align-items`.

The layout resolves:

`align-self: auto`

to the parent container's `align-items` value.

This is resolved during layout, not Paint.

## align-content

Multiple horizontal flex lines can now be distributed across the container
cross axis.

Supported behavior:

- stretch line cross sizes
- cross-start
- centered lines
- cross-end
- space between lines

For one line the existing single-line behavior is preserved.

## Nested Flexbox

Regression coverage now explicitly exercises a flex container used as a child
of another flex container.

This protects the architectural rule that a flex item remains a normal layout
box whose own descendants can establish another formatting context.

No special pointer graph or alternate DOM is created for nesting.

## Data-oriented rule

The 2B-8 implementation does not add a permanent Flexbox object tree.

Temporary layout planning uses short-lived vectors and compact line ranges.

Persistent layout data remains:

- flat
- index-addressed
- renderer-neutral
- free of DOM pointers

## Correctness limitations still explicit

This milestone does NOT claim full CSS Flexbox compliance.

Deliberately deferred:

- column-axis wrapping
- wrap-reverse
- align-content for wrapped columns
- align-self baseline
- align-items baseline
- `order`
- auto margins as Flexbox alignment
- `flex-flow`
- exact min-content / max-content sizing
- fit-content
- complete percentage height resolution
- replaced-element intrinsic sizing
- reflow of nested content after every stretch mutation
- full Web Platform Test runner

Recognizing a property is not treated as proof of complete standards support.

## Regression cases included

The source now tests:

- row wrapping into additional lines
- row-reverse placement
- per-item `align-self`
- nested flex containers
- existing grow/shrink behavior
- existing column justification
- existing box-model constraints

## Next milestone

Recommended:

**2B-9 — Flexbox Multi-Axis + Intrinsic Sizing Consolidation**

Scope:

- definite-height flex context
- column wrapping
- wrap-reverse
- stronger intrinsic sizing
- flex-flow
- auto-margin alignment
- nested stretch reflow
- first executable Flexbox WPT harness slice

Only after that should Phantom consider the Flexbox foundation sufficiently
stable to move aggressively into images and replaced elements.
