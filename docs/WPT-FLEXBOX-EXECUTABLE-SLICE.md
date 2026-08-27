# Phantom Executable Flexbox WPT Slice

## Command

```bash
cargo test -p phantom-layout --test wpt_flexbox_slice
```

## What this milestone means

This is the first executable standards-conformance slice attached directly to
Phantom's layout pipeline.

It is not yet the official Python `wpt` runner and it does not claim a WPT pass
percentage.

The purpose is to establish the permanent workflow:

```text
spec behavior
    ↓
upstream WPT family
    ↓
Phantom-native executable assertion
    ↓
layout regression gate
```

As Phantom gains testharness.js, reftest and headless rendering support, these
native assertions will be complemented by unmodified upstream tests.

## Current cases

- main-axis `margin:auto` in row Flexbox
- two auto margins centering a flex item
- cross-axis auto margin overriding alignment
- main-axis auto margin in column Flexbox
- equal distribution for `flex:1`

## Upstream mapping

One current case explicitly maps to:

```text
css/css-flexbox/flex-one-sets-flex-basis-to-zero-px.html
```

Other cases map to the CSS Flexbox auto-margin algorithm and will receive exact
upstream file mappings as the pinned WPT corpus is introduced.

## Honesty rule

A Phantom-native assertion passing is not reported as the corresponding
official WPT file passing unless the complete upstream test is actually run.

That distinction remains permanent.
