# Phantom Engine 2B-10 — Flexbox Final Core + Auto Margins + Executable WPT Slice

## Objective

Close the first sustainable Flexbox core before Phantom moves into images and
replaced elements.

"Final Core" does not mean complete CSS Flexbox conformance.

It means the architecture now has enough correctly separated pieces to stop
treating Flexbox as a bootstrap experiment.

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

Flexbox remains owned by Layout.

Paint still does not calculate Flexbox and does not traverse the DOM.

## Typed margin:auto

2B-9 deliberately refused to encode `auto` as a magic numeric zero.

2B-10 introduces an explicit semantic companion type:

```text
AutoEdges
```

Computed style now keeps two independent pieces of information:

```text
margin: EdgeSizes
margin_auto: AutoEdges
```

The numeric edge remains useful for hot-path geometry.

The semantic bit survives until the formatting context that owns its
resolution.

This preserves both performance and correctness.

## Flex main-axis auto margins

For positive remaining free space, automatic margins absorb the free space
before `justify-content` is applied.

Supported on:

- row
- row-reverse
- column
- column-reverse

Multiple automatic margins share the available space.

Examples:

```css
.item {
    margin-left: auto;
}
```

pushes the item toward the physical right side in a normal row.

```css
.item {
    margin-left: auto;
    margin-right: auto;
}
```

centers the item by distributing free space to both margins.

If free space is negative, automatic margins resolve to zero for this stage.

## Cross-axis auto margins

Automatic cross-axis margins override `align-self` / `align-items` placement.

Supported physical combinations include:

- top auto
- bottom auto
- both top and bottom auto
- left auto
- right auto
- both left and right auto

Cross-axis stretch is not applied when an automatic cross-axis margin owns the
remaining free space.

## Source order and reverse axes

No DOM mutation is performed.

Reverse flex directions remain geometry decisions over source-order layout
items.

Auto margins remain physical margin properties and are resolved consistently
with the current physical-axis subset.

## Outside Flexbox

The semantic `auto` state is preserved in computed style for every element.

2B-10 only resolves automatic margins inside the Flex Formatting Context.

Block formatting `margin:auto` centering is deliberately deferred until the
block width/containing-block rules are expanded enough to implement it
correctly.

Phantom does not erase the semantic state merely because one formatting context
does not resolve it yet.

## Executable WPT slice

A new integration-test target is included:

```text
crates/phantom-layout/tests/wpt_flexbox_slice.rs
```

Run it directly with:

```bash
cargo test -p phantom-layout --test wpt_flexbox_slice
```

The first slice checks executable geometry invariants for:

- row main-axis auto margin
- two automatic margins centering an item
- cross-axis auto margin
- column main-axis auto margin
- `flex: 1` equal main-axis distribution

The `flex: 1` case records its upstream WPT family:

```text
css/css-flexbox/flex-one-sets-flex-basis-to-zero-px.html
```

This milestone intentionally calls the test file a WPT slice, not a complete
WPT runner.

Phantom-native assertions are used while the full browser testharness/reftest
environment is not yet available.

No official upstream test is silently modified or represented as passing when
its complete prerequisites are unsupported.

## Standards fidelity

The Flexbox implementation is developed against:

- CSS Flexible Box Layout Module
- CSS Box Model
- CSS Sizing
- Web Platform Tests

Implementation comments, regression tests and future compatibility reports
should reference the relevant specification section or upstream WPT family.

Behavior observed in another browser is evidence, not specification.

If deployed browser behavior and normative text disagree:

1. record the difference;
2. inspect WPT interoperability evidence;
3. inspect current CSSWG discussion/issues;
4. do not silently hard-code a browser-specific quirk.

## Data-oriented architecture remains unchanged

Persistent layout output remains:

- flat `Vec<LayoutBox>`
- compact `LayoutId`
- shared text storage
- renderer-neutral geometry

The new `AutoEdges` value is four semantic bits, not another pointer graph.

No permanent Flexbox object tree is introduced.

## Still not complete Flexbox conformance

Explicitly deferred:

- auto margins in block formatting
- `order`
- baseline alignment
- full intrinsic height
- full min-content/max-content algorithms
- complete replaced-element sizing
- complete percentage height resolution
- descendant reflow after every cross-size stretch
- official WPT testharness execution
- broad Flexbox WPT pass-rate claims

## Exit gate

2B-10 is complete only when all normal quality gates pass plus the dedicated
Flexbox conformance slice:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-layout --test wpt_flexbox_slice
```

Only after that gate should Phantom move into images and replaced elements.

## Next recommended milestone

**2C-1 — Images + Replaced Elements Boundary**

Initial scope:

- `<img>`
- safe resource/decode boundary
- intrinsic width and height
- aspect ratio
- decoded-memory limits
- image paint command
- caching contract
- layout integration without decoder ownership
