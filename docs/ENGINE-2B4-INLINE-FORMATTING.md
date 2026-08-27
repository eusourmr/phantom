# Phantom Engine 2B-4 — Inline Formatting Context + Line Boxes

## Milestone

The Phantom layout stage now owns text flow.

Before this milestone, text geometry was one approximate rectangle per DOM text
node. Paint and the native shell could display it, but the layout model did not
have real lines.

The active pipeline is now:

HTML
  ->
DOM
  ->
Computed Style
  ->
Block Formatting
  ->
Inline Formatting Context
  ->
Line Boxes
  ->
Text Fragments
  ->
PaintList
  ->
Native shell

## Concrete additions

### Explicit Line Boxes

`LayoutKind::Line` represents one line in the cold snapshot.

A line owns positioned text fragments through compact `LayoutId` relationships.

### Text Fragments

A DOM text node may now produce multiple layout text fragments.

Each fragment has:

- x
- y
- width
- height
- UTF-8 range
- source NodeId by value
- effective underline decoration

Paint does not wrap text.

### Whitespace wrapping

For normal flow:

- consecutive whitespace collapses;
- words wrap when the next word does not fit;
- leading whitespace after a wrap is discarded;
- unbreakable words may overflow until overflow-wrap exists.

### Forced breaks

`<br>` closes the current line and starts the next line.

### Preformatted text

`<pre>` preserves hard newlines and tabs are expanded to four spaces.

Full CSS `white-space` is intentionally not implemented yet.

## Architecture

Paint still does not depend on DOM.

The line algorithm lives entirely in `phantom-layout`.

`phantom-paint` consumes only:

- LayoutSnapshot
- StyleMap

Line boxes are structural and do not themselves paint.

## Memory direction

The old bootstrap DOM-oriented DisplayList has been removed from
`phantom-layout`.

The layout snapshot remains:

- one contiguous Vec<LayoutBox>
- one shared UTF-8 String
- u32 relationships
- u32 text ranges

New metrics:

- line_count()
- text_fragment_count()
- estimated_storage_bytes()

These metrics will later feed memory/performance benchmarks.

## Current text measurement

The first inline formatter uses deterministic, renderer-independent advance
estimation based on:

- font size
- font family category
- font weight
- font style
- basic character-width classes

This is deliberately not called font shaping.

It provides stable geometry while keeping Layout independent from egui.

## Not yet implemented

- font shaping
- glyph IDs
- kerning
- ligatures
- Unicode bidi
- script shaping
- CSS line-height
- text-align
- white-space property
- overflow-wrap
- word-break
- hyphenation
- inline backgrounds across fragmented lines
- vertical-align
- baseline alignment
- replaced inline elements such as images

## Next recommended milestone

2B-5 — Font Metrics + Text Shaping Boundary.

The goal is not to embed shaping into Layout. The goal is to define a narrow
text-measurement contract so a future font/shaping subsystem can provide glyph
metrics while Layout remains renderer-independent.
