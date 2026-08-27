# Phantom Engine 2B-5 — Font Metrics + Text Shaping Boundary

## Objective

Text is a subsystem, not a helper function inside Layout.

The active architecture becomes:

CSS ComputedStyle
    ->
Phantom text request
    ->
TextShaper boundary
    ->
font metrics / measurement
    ->
Inline Formatting Context
    ->
Line Boxes
    ->
PaintList

Layout no longer contains character-width tables or font metric formulas.

## New crate

`phantom-text`

It has no production dependencies.

It does not depend on:

- DOM
- HTML
- CSS
- Layout
- Paint
- egui
- WGPU
- operating-system font APIs

That keeps the boundary narrow and independently testable.

## TextShaper contract

A backend provides three operations:

1. `font_metrics()`
   - ascent
   - descent
   - line gap

2. `measure()`
   - allocation-free horizontal advance for line breaking

3. `shape()`
   - backend-neutral glyph IDs
   - UTF-8 cluster offsets
   - advances
   - x/y offsets

The separation between `measure()` and `shape()` is intentional. Layout can
perform a large number of line-break checks without allocating a glyph vector.

## FallbackTextShaper

The first backend is deliberately lightweight and deterministic.

It keeps the behavior from the previous heuristic stage, but the heuristic is
now isolated behind the text boundary rather than embedded in Layout.

It is NOT claimed to provide:

- OpenType shaping
- ligatures
- kerning
- Unicode bidi
- Arabic shaping
- Indic shaping
- variable fonts
- system font fallback

## Why this stage matters for performance

A browser can spend a significant amount of time measuring text during layout.

The boundary allows future implementations to add:

- font-metric caches
- shaped-run caches
- per-font caches
- thread-local shaping contexts
- arena-backed glyph storage
- cache eviction policies
- SIMD-friendly glyph buffers

without changing the layout algorithm.

## Why we are not adding a large shaping dependency yet

The Phantom project is still stabilizing its own DOM, style, layout, paint, and
compositor contracts.

A mature shaping implementation can later be plugged into `TextShaper`, but it
should not define the architecture.

This keeps the motor independent while avoiding the mistake of writing font
format parsers, OpenType shaping, and Unicode algorithms prematurely inside
Layout.

## New public layout entry point

`build_layout_snapshot_with_shaper(...)`

This lets tests and future engine configurations inject a different text
backend while the ordinary `build_layout_snapshot(...)` uses the lightweight
fallback backend.

## Next milestone

Recommended:

2B-6 — CSS Box Model + Borders + min/max constraints.

That gives Layout/Paint a more complete geometric box before the first real
Flexbox algorithm.

A parallel future text milestone can replace `FallbackTextShaper` with real
font metrics and OpenType shaping without rewriting Layout.
