# Phantom Engineering + Standards Conformance Doctrine

This document is a permanent project rule.

Phantom is allowed to innovate aggressively in browser architecture, human
control, context preservation, privacy, intelligence and interaction.

Phantom is not allowed to invent incompatible HTML/CSS semantics merely because
a different implementation would be easier.

## Canonical sources

### HTML

Primary normative source:

https://html.spec.whatwg.org/

Source repository:

https://github.com/whatwg/html

### CSS

Primary Editor Draft source family:

https://drafts.csswg.org/

Source repository:

https://github.com/w3c/csswg-drafts

For Flexbox:

https://drafts.csswg.org/css-flexbox/

### Web Platform Tests

Canonical cross-browser test repository:

https://github.com/web-platform-tests/wpt

CSS Flexbox test family:

https://github.com/web-platform-tests/wpt/tree/master/css/css-flexbox

## Source precedence

When implementing Web Platform behavior, use this evidence order:

1. current normative specification text;
2. specification algorithms and definitions;
3. Web Platform Tests;
4. current standards-group issues/discussion;
5. interoperable deployed behavior as compatibility evidence;
6. individual-browser behavior only as diagnostic evidence.

No single existing browser is Phantom's specification.

## Implementation rule

Every meaningful Web Platform feature should eventually record:

- specification;
- relevant section/algorithm;
- supported subset;
- intentionally unsupported behavior;
- regression tests;
- WPT mapping where available;
- known deviations.

## WPT rule

Never edit an imported upstream expected result merely to make Phantom pass.

When official WPT tests are vendored or pinned:

- record upstream repository;
- record commit revision;
- preserve license/attribution;
- keep skipped tests separate from failures;
- classify failures by subsystem;
- make results reproducible.

## Innovation boundary

The Phantom Human Context Layer, capability model, explainability model,
privacy architecture and future search/intelligence layers sit above the Web
Platform engine.

They may extend what a browser can do.

They must not silently change what an HTML element, CSS declaration, origin,
permission or Web API means.

## Engineering architecture

Phantom uses object-oriented engineering discipline adapted to Rust:

- encapsulation;
- strong domain objects;
- traits as contracts;
- composition over inheritance;
- dependency inversion;
- explicit ownership;
- typed states;
- typed errors;
- narrow public APIs.

Hot engine data remains data-oriented:

- contiguous vectors;
- compact IDs;
- arenas where justified;
- shared buffers;
- immutable snapshots;
- bounded queues;
- no pointer-heavy object graph in hot layout/paint paths.

Object-oriented architecture and data-oriented runtime design are complementary.

## Clean code

Production code must prefer:

- small cohesive functions;
- explicit names;
- single-responsibility modules;
- deterministic state transitions;
- documented public contracts;
- no magic values;
- no hidden global mutable state;
- no silent fallback that changes semantics;
- no duplicated algorithms across crates.

## Safety

Permanent quality baseline:

```rust
unsafe_code = "forbid"
```

Do not weaken quality gates to make a build green.

Errors must be handled as data, not hidden through production `panic`,
`unwrap` or `expect`.

## Auditability

A future contributor should be able to answer:

- where did this behavior come from?
- which specification defines it?
- which crate owns it?
- which test proves it?
- which assumptions remain temporary?
- what memory/security cost does it introduce?

If the code cannot answer those questions, the implementation is not finished.

## Performance

Phantom aims to be agile, light and fast through architecture.

Never through false compatibility or unsafe shortcuts.

Performance claims require benchmarks.

Optimization priority:

1. remove unnecessary work;
2. improve data representation;
3. avoid allocation;
4. improve locality;
5. cache only with bounded policy;
6. parallelize only after contracts are stable;
7. optimize machine-level details only with measurement.

## Dependency rule

Phantom owns core browser semantics.

Mature libraries may be used at narrow infrastructure boundaries where
reimplementation would reduce security or consume disproportionate effort.

A dependency must never become the hidden architecture of the browser.

## Historical ambition

The objective is not to reproduce today's browsers line for line.

The objective is to build a standards-faithful Web engine whose architecture is
clean enough to support a new human-controlled browser model.

Innovation and standards fidelity are not opposites.

The engine should be conservative about Web semantics and ambitious about what
the browser can become.
