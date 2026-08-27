# Phantom Engine 2B-3 — Paint Pipeline v2

## Architectural milestone

The renderer no longer consumes the DOM-oriented bootstrap DisplayList.

The active path becomes:

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
Native shell
  ->
WGPU compositor later

## Concrete wall

`phantom-paint` has production dependencies only on:

- phantom-css
- phantom-layout

It does not depend on `phantom-dom` or `phantom-html`.

Paint therefore cannot traverse the DOM.

## PaintList

The first renderer-neutral command stream contains:

- FillRect
- Text

Text payloads use one shared UTF-8 buffer and compact u32 byte ranges.

Each text command carries only what painting needs:

- geometry
- color
- font size
- coarse font weight
- font posture
- coarse font family
- underline state

## Visible consequence

CSS values can now reach native page painting through the full Phantom path.

Examples already represented:

- color
- background-color
- font-size
- font-weight
- font-style
- monospace family
- underline

The browser shell no longer decides heading sizes or link colors from HTML
semantics. Those values now come from computed style.

## Memory direction

The renderer receives contiguous command and text buffers rather than walking
object graphs.

Current structure:

PaintList {
    Vec<PaintCommand>,
    String text_buffer,
    viewport_width,
    content_height
}

The command format remains renderer-neutral so it can later be serialized,
submitted across threads, culled, tiled, or translated into WGPU batches.

## Still intentionally missing

- real font shaping
- line boxes
- glyph runs
- clipping
- borders
- images
- gradients
- shadows
- transforms
- layers
- compositor surfaces
- dirty-region tracking
- GPU batches

## Next milestone

2B-4 — Inline Formatting Context + Line Boxes.

That stage replaces approximate text geometry with explicit line fragments,
wrapping, inline flow, and the first basis for real text layout.
