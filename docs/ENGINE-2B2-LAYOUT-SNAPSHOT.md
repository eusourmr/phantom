# Phantom Engine 2B-2 — Cold Layout Snapshot

## Contract

Input:

DOM snapshot + ComputedStyle snapshot + viewport width

Output:

LayoutSnapshot {
    Vec<LayoutBox>,
    shared UTF-8 text buffer,
    viewport width,
    content height
}

The layout snapshot is cold and geometry-oriented.

## Memory rules

- no Rc
- no RefCell
- no boxed child tree
- no references into DOM
- no raw pointers
- parent/child/sibling relations use compact u32 LayoutId values
- text payloads live in one shared buffer and boxes store compact ranges
- boxes live contiguously in Vec<LayoutBox>

A copied NodeId is retained only as a numeric source handle for future
hit-testing and event routing. It is not a DOM pointer.

## Current geometry scope

Implemented:
- block boxes
- inline boxes represented structurally
- display:none subtree elimination
- flex container marker
- vertical normal flow
- width auto
- width px
- width %
- height px
- margin
- padding
- deterministic text measurement approximation
- relayout without reparsing HTML or recomputing CSS

Not yet implemented:
- true inline formatting context
- line boxes
- intrinsic sizing
- margin collapsing
- Flexbox algorithm
- Grid
- absolute/fixed positioning
- replaced-element sizing
- font shaping
- GPU paint

## Next architectural move

Paint must stop reading the DOM.

The next path is:

LayoutSnapshot
    ->
PaintList / DisplayList v2
    ->
native shell first
    ->
WGPU compositor later
