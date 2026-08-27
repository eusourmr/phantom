# Phantom Flexbox — WPT Seed Plan

This document is a preparation layer for the future Web Platform Tests runner.

It does not claim that Phantom currently passes official WPT files.

## First executable Flexbox categories

When the harness is introduced, select a deliberately small corpus covering:

1. row main-axis placement
2. column main-axis placement
3. row-reverse
4. column-reverse
5. flex grow
6. flex shrink
7. flex basis
8. gap
9. justify-content
10. align-items
11. align-self
12. row wrapping
13. align-content
14. nested flex containers
15. min/max constraints
16. content-box versus border-box

## Test adoption rule

For every imported WPT case:

- preserve upstream attribution and license;
- record upstream path and revision;
- do not edit expected behavior to make Phantom pass;
- classify failure as parser, cascade, sizing, layout, paint or unsupported;
- keep unsupported behavior visible.

## Gate

A future milestone may advertise a WPT pass percentage only when:

- the runner is reproducible;
- the selected revision is pinned;
- failures are retained;
- skipped tests are counted separately;
- results can be reproduced from a clean checkout.
