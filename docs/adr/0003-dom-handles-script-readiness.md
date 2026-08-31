# ADR-0003 — DOM Handles and Script Readiness

**Status:** Accepted as a pre-scripting constraint  
**Date:** 2026-08-28

## Context

The current `phantom-dom` stores owned nodes and exposes stable `NodeId` values. It does not use `Rc<RefCell<Node>>`. At the current rendering stage, nodes are created during parsing and are not arbitrarily removed/reused by JavaScript.

A future scripting runtime changes the problem: script objects may retain node references while DOM mutations delete, move or replace nodes. Plain reusable integer indexes would then permit stale references to accidentally target a different node.

## Decision

1. Keep the existing `NodeId` model through the current static-document/navigation milestones.
2. Before exposing persistent live DOM handles to JavaScript, introduce explicit stale-handle semantics.
3. Prefer a **generational arena/slot model** unless benchmarking or a stronger design demonstrates a better alternative.
4. A future handle should contain both slot identity and generation/version. Reusing a slot increments its generation.
5. DOM handles exposed to scripting must resolve through a checked boundary; raw Rust references are not retained by the runtime.
6. Do not make the entire DOM `Send + Sync` merely for theoretical parallelism.
7. Layout/style may consume immutable snapshots or bounded read models, allowing parallel work without arbitrary concurrent DOM mutation.

## Consequences

- No unnecessary DOM rewrite in 2C.
- Script integration has a concrete memory-safety/stale-reference gate.
- Future slot storage may migrate from `BTreeMap` to a denser arena when mutation/performance data justifies it.
- The exact arena implementation remains replaceable; no third-party arena crate is selected prematurely.
