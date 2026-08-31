# Phantom 2C-13 — Browser Control Layout Boundary

`phantom-layout` now emits `LayoutKind::Control` for the supported first form
slice.

The layout crate knows only geometry and a coarse `ControlKind`:

- TextInput
- SearchInput
- SubmitButton

It does **not** know current values, submission fields, URL actions or browser
navigation.

Default first-slice control geometry:

- text/search: 200 × 30 logical px;
- submit: 96 × 30 logical px.

Explicit CSS width and px height are honored in the bounded geometry slice.

## Why not overlay arbitrary coordinates in the browser?

A browser widget that is not represented in Layout does not participate in
line wrapping or document height and drifts away from the engine's own
geometry. Creating a renderer-independent control box preserves the pipeline:

DOM -> Style -> Layout -> Browser-native widget

## Paint

2C-13 intentionally does not add a renderer-neutral control paint command.
The native shell paints the first interactive control implementation.
A later renderer/compositor milestone can promote controls to a fully
renderer-neutral visual contract.
