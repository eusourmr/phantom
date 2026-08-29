# Phantom Architecture

## Architectural intent

Phantom is designed as two cooperating systems:

1. **Web Engine** — parse, execute, layout, render, communicate, store and isolate web content.
2. **Intelligence Runtime** — understand semantic entities, retain user-authorized memory, reason about goals and propose or execute actions through explicit policy gates.

The intelligence runtime is not allowed to bypass browser security boundaries.

**Phantom Guardian** is a local-first protection layer that observes security-relevant events from both systems, correlates risk signals and explains suspicious behavior without becoming a privileged security authority. Deterministic browser controls, isolation, origin policy, capability enforcement and explicit human approval remain authoritative.

## Dependency direction

```text
Browser / Embedders
        |
        v
Application / Engine orchestration
        |
        v
Domain components
        |
        v
Core types and contracts
```

Infrastructure adapters implement contracts exposed toward the domain/application boundary. Domain crates do not depend on the UI, operating-system shell, telemetry backend or concrete storage implementation.

## Target process model

```text
Browser Process
  |-- Policy Broker
  |-- Network Service       [sandboxed]
  |-- GPU Service           [sandboxed]
  |-- Storage Service
  |-- Guardian Service      [sandboxed]
  |-- Intelligence Service  [sandboxed]
  |-- Extension/WASM Service[sandboxed]
  `-- Site Instances        [sandboxed]
```

No process is trusted merely because it belongs to Phantom. IPC input is validated at every boundary.

## Core principles

### Strong types
Represent domain concepts as domain types. Avoid primitive obsession.

### Composition
Behavior is assembled from small components and traits. Deep inheritance-style hierarchies are prohibited.

### Explicit side effects
Network, filesystem, microphone, camera, clipboard, location, credential and action execution access require explicit capabilities.

### Invalid states should be unrepresentable
Use enums, constructors and private fields to enforce invariants at compile time whenever practical.

### Unsafe isolation
The default workspace forbids unsafe Rust. If low-level FFI becomes necessary, it must live in a dedicated crate with a documented safety contract, focused fuzzing and explicit security ownership.

## Planned crate families

```text
core/
  phantom-core
  phantom-protocol

web/
  phantom-dom
  phantom-html
  phantom-css
  phantom-style
  phantom-layout
  phantom-web-api
  phantom-js

render/
  phantom-scene
  phantom-render
  phantom-gpu

platform/
  phantom-network
  phantom-storage
  phantom-sandbox
  phantom-os

intelligence/
  phantom-semantic
  phantom-memory
  phantom-agent
  phantom-policy
  phantom-guardian

product/
  phantom-engine
  phantom-browser
  phantom-embed
```

Crates are added only when an implementation need exists; this map is a target, not permission to create empty abstractions.

## Semantic runtime

Phantom will maintain a semantic representation separate from the DOM. The semantic graph may identify typed entities such as flights, prices, people, organizations, documents, products, laws, reservations and actions.

Every semantic assertion should preserve provenance and distinguish observed data, parsed structured data, model inference, user-provided information, stale information and conflicts.

## Human-controlled agent runtime

```text
Agent proposal
      |
      v
Policy evaluation
      |
      +--> deny
      +--> execute low-risk capability
      `--> require human approval --> execute
```

High-impact actions must never be silently escalated by an agent.

## Phantom Guardian

Guardian is planned as a constrained local security-intelligence service, not as a replacement for the browser's security kernel. It may combine deterministic signals, heuristics and later a compact local model to detect patterns such as suspicious redirects, origin changes, credential-harvesting behavior, unusual permission requests, tracking fanout or other anomalies; however, it must not directly grant capabilities, bypass sandbox or origin rules, read arbitrary local data, silently transmit page content to external services or execute high-impact actions. Guardian recommendations remain advisory unless an independent deterministic policy authorizes the action or the user explicitly approves it.

### Guardian Security Event Contract

The Guardian Security Event Contract defines a small typed stream of security-relevant observations emitted by browser subsystems—such as navigation and origin transitions, blocked mixed content, private-network attempts, permission requests, cross-origin form submissions, certificate state, downloads and other policy decisions—with provenance, document/site generation, correlation identifiers and severity metadata; producers must not depend on Guardian, events must contain only the minimum data required for assessment, secrets and full page content must not be included by default, and consumers such as audit logging, security UI or Guardian intelligence may observe these events but cannot use the event channel itself to acquire new privileges or bypass deterministic policy gates.

## Auditability

Security-sensitive actions emit structured audit events with correlation identifiers. Audit events record the requested action, policy decision, capability used, approval state when applicable and execution outcome without leaking secrets.

## Compatibility strategy

Phantom pursues standards compliance incrementally. Compatibility shortcuts must not silently weaken security invariants. Exceptions require documentation and tests.
