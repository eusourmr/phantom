# External Review Response — 2026-08-28

## 1. “No executable code” — factually incorrect, documentation drift exposed

The current repository contains a Cargo workspace, Cargo.lock and multiple Rust crates including browser, engine, HTML, CSS, layout, paint, images and networking.

The review was nevertheless useful because README/ROADMAP still described several implemented systems as “planned”. This package updates public documentation so repository status matches reality.

## 2. Scope is too large — accepted

The old roadmap mixed near-term engineering with long-term vision. The new roadmap introduces a bounded Engine Beta and explicitly defers JIT, full compositor/process architecture, extensions, semantic runtime, memory and agents.

## 3. JavaScript too late — accepted in architectural form

The script boundary moves earlier. Dynamic-web architecture will be validated before product breadth. Building a custom production JIT remains out of near-term scope.

## 4. Parser strategy absent — accepted

A dedicated parser strategy now defines bounded independent implementations, WPT/differential testing, malformed-input recovery and an ADR-based escape hatch if full parser ownership becomes schedule-prohibitive.

## 5. Intelligence/agents before browser — accepted

Semantic runtime, memory and agents are explicitly post-browser-beta and non-blocking.

## 6. CI not exercising code — factually incorrect, but strengthened

Existing CI already runs `cargo fmt`, Clippy, tests and rustdoc against the Rust workspace. The updated CI adds explicit `cargo check`, pins Rust 1.95 and adds a Windows native-browser job.
