# Phantom Engine 2B-9 — Flexbox Multi-Axis + Intrinsic Sizing Consolidation

## Objective

Close the largest structural gaps left by Flexbox Core v1 without turning
Phantom's layout engine into a compatibility patchwork.

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

Flexbox remains a layout concern.

Paint does not calculate Flexbox and does not traverse DOM.

## New CSS behavior

### wrap-reverse

Supported:

- `flex-wrap: wrap-reverse`

The cross-start and cross-end direction is reversed without mutating DOM source
order.

This applies to both row and definite-height column flex contexts.

### flex-flow

Supported shorthand:

- `flex-flow: <flex-direction>`
- `flex-flow: <flex-wrap>`
- `flex-flow: <flex-direction> <flex-wrap>`
- either component order

Examples:

```css
flex-flow: row wrap;
flex-flow: wrap column;
flex-flow: column-reverse wrap-reverse;
```

Missing shorthand components reset to their initial values, matching the
shorthand contract rather than preserving stale longhand state.

## Multi-axis wrapping

### Row axis

Existing row wrapping is retained and extended with `wrap-reverse`.

Multiple lines can be positioned from either physical cross edge.

### Column axis

Phantom can now wrap column and column-reverse flex containers when the
container has a definite logical-pixel height.

Example:

```css
.container {
    display: flex;
    flex-flow: column wrap;
    width: 400px;
    height: 300px;
}
```

Items fill the first vertical flex line and then create additional columns.

Grow and shrink are resolved independently inside each flex line during final
main-axis distribution.

### Why definite height is required

A vertical main axis needs a finite main-size boundary to decide where a line
ends.

Phantom deliberately does not invent a wrapping height when `height:auto` is
indefinite.

That is a correctness choice, not a missing fallback.

## align-content on both axes

`align-content` now participates in multi-line placement for:

- wrapped rows;
- wrapped columns.

`wrap-reverse` reverses the cross-axis origin while retaining the same
renderer-neutral layout representation.

## Intrinsic sizing consolidation

The previous intrinsic width approximation recursively summed descendants.

That was useful as a bootstrap but too crude for increasingly real Flexbox.

2B-9 separates two concepts.

### max-content-like width

Used for preferred auto width.

Phantom now distinguishes block/flex descendants from inline sequences:

- inline contributions accumulate on the current intrinsic line;
- block/flex descendants establish a wider independent contribution;
- hidden descendants are ignored.

This remains a bounded approximation and is not yet advertised as complete CSS
Sizing Level 3 conformance.

### min-content-like width

Used as an automatic flex shrink floor.

For text, Phantom measures the longest unbreakable whitespace-separated token.

For element trees, the widest minimum contribution becomes the floor.

This prevents a row flex item with `min-width:auto` from being shrunk below a
long unbreakable word merely to force it inside the container.

## min-width initial value

The computed-style default is now represented as:

```text
min-width: auto
```

instead of prematurely collapsing the value to `0px`.

Outside Flexbox, the existing block constraint resolver may still treat auto as
zero where appropriate for the current layout subset.

Inside row Flexbox, the preserved `auto` state enables the intrinsic minimum
floor.

This is an example of why Phantom keeps semantic states until the subsystem
that owns their resolution.

## Data-oriented architecture

No permanent Flexbox tree is introduced.

Persistent layout output remains:

- `Vec<LayoutBox>`
- compact `LayoutId`
- shared text buffer
- numeric geometry

Multi-line planning uses temporary vectors and line ranges that disappear
after the layout pass.

## Deliberately not faked

### auto margins

`margin:auto` is NOT implemented in 2B-9.

The current `EdgeSizes` type represents resolved numeric edges. Encoding
`auto` as `0.0` would erase semantic information and make later Flexbox
alignment behavior ambiguous.

A future milestone should introduce an explicit margin value type rather than
smuggling state through magic numbers.

### intrinsic height

Automatic min-content height for vertical Flexbox is not yet complete.

Column wrapping is therefore based on definite container height and natural
item height.

### stretch reflow

Changing a flex item's cross size can currently resize its cold box, but full
descendant reflow after every stretch mutation remains deferred.

That should be solved through explicit reflow contracts rather than hidden
recursive side effects.

## Regression coverage added

The source includes tests for:

- column wrapping into multiple columns;
- row `wrap-reverse`;
- `flex-flow`;
- automatic min-content floor during row shrink;
- previously existing row/column reverse behavior;
- nested Flexbox;
- box-model min/max constraints.

## Compatibility statement

2B-9 is a substantial Flexbox architecture milestone.

It is still not full Flexbox conformance.

Unsupported or incomplete behavior remains visible rather than silently
approximated.

## Next recommended milestone

**2B-10 — Flexbox Final Core + Auto Margins + Executable WPT Slice**

Recommended scope:

- typed `margin:auto`;
- Flexbox main-axis auto-margin distribution;
- cross-axis auto margins;
- nested stretch reflow contract;
- intrinsic height consolidation;
- first reproducible executable Flexbox WPT subset;
- failure classification by parser / cascade / sizing / layout / paint.

After that gate, Phantom can move into images and replaced elements with a much
more stable geometry foundation.
