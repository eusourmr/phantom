# Phantom Architecture

## Architectural intent

Phantom's implemented core is a web engine plus native browser shell. The long-term intelligence thesis is deliberately separated from the browser-engine execution roadmap.

### System A — Web engine (active)

Parse documents, compute style, lay out content, paint, fetch resources and enforce security/resource boundaries.

### System B — Intelligence runtime (deferred product layer)

Semantic entities, user-authorized memory and human-controlled actions are post-browser-beta work. They may never bypass web/security boundaries and are **not prerequisites for Engine Beta**.

## Current implemented dependency shape

```text
phantom-browser
      |
      v
phantom-engine
      |
      +-- phantom-dom
      +-- phantom-html
      +-- phantom-css
      +-- phantom-layout
      +-- phantom-paint
      +-- phantom-text
      +-- phantom-image
      +-- phantom-net
      +-- phantom-security
      `-- phantom-core
```

The repository creates new crates only for real implementation needs.

## Execution order

The project follows this priority order:

1. browser correctness and crash resistance;
2. parser/layout/network compatibility;
3. script-ready architecture and dynamic-page support;
4. security/isolation hardening proportional to implemented capabilities;
5. product intelligence only after browser fundamentals are stable.

This order resolves an important architectural rule: **Phantom must remain a browser with optional intelligence, not an intelligence system with a browser attached.**

## Scripting boundary

JavaScript is no longer treated as a remote, isolated “phase 5”. The architecture must be script-ready before substantial dynamic-page work.

The future scripting layer interacts through explicit contracts:

```text
ECMAScript Runtime
       |
       v
phantom-script boundary
       |
       +-- DOM mutation commands
       +-- events/task queues
       +-- timers
       `-- network/fetch capability
```

The ECMAScript implementation can be replaceable. Browser independence does not require owning a JIT compiler. A third-party ECMAScript runtime may initially be embedded behind a Phantom-controlled boundary, subject to security, licensing and maintenance review.

## Parser ownership

`phantom-html` and `phantom-css` are currently bounded, independent implementations. They do not claim full WHATWG/CSS conformance.

Their compatibility strategy is:

- measured subsets;
- explicit error recovery;
- WPT-derived regression cases;
- fuzzing for untrusted input;
- no silent “full standards compliance” claim.

See `docs/PARSER-STRATEGY.md`.

## Rendering

The native shell currently paints Phantom's own renderer-neutral commands through its chosen UI backend. A bespoke GPU compositor is **not an Engine Beta gate**. GPU architecture is introduced only when compatibility/performance evidence justifies the cost.

## Network and security

Network URLs, response budgets, cache semantics and isolation keys are explicit Phantom domain concepts. Network and parser input are untrusted.

Future process sandboxing remains a target, but the roadmap distinguishes current in-process boundaries from a future multi-process security architecture.

## Target process model (future, not current)

```text
Browser Process
  |-- Policy Broker
  |-- Network Service       [candidate future sandbox]
  |-- GPU Service           [candidate future sandbox]
  `-- Site Instances        [candidate future sandbox]
```

This diagram is a target architecture and must not be read as implemented functionality.

## Core principles

- strong domain types;
- explicit side effects;
- invalid states prevented where practical;
- `unsafe` forbidden by default;
- bounded resource consumption on hostile input;
- observable/testable behavior over speculative abstraction.
