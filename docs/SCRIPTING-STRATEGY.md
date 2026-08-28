# Phantom Scripting Strategy

## Strategic correction

JavaScript must not arrive as a late bolt-on after the browser architecture has become immutable. The scripting **boundary** moves into the pre-Beta architecture plan.

This does not mean Phantom should immediately build an ECMAScript engine/JIT from scratch.

## Independence definition

Phantom is an independent browser engine when Phantom owns browser architecture: DOM integration, layout, paint, navigation, security policy, resource lifecycle and product behavior.

Embedding a replaceable ECMAScript runtime is compatible with that independence. It is not a Chromium/WebKit/Gecko fork.

## Initial architecture

A future `phantom-script` crate should own contracts rather than a specific runtime:

```text
Runtime Adapter
    |
    v
phantom-script
    |
    +-- ScriptRealm / Document context
    +-- DOM mutation commands
    +-- event dispatch
    +-- task queue / microtask hooks
    +-- timer capability
    `-- fetch/network capability
```

The runtime must not receive unrestricted internal Rust references to DOM/layout/network state.

## Runtime implementation decision

Before Browser Technology Preview, evaluate:

- Rust-native embeddable ECMAScript engines;
- small external runtimes behind an FFI isolation crate;
- the cost of a Phantom-owned interpreter.

Selection criteria:

- conformance trajectory;
- memory safety boundary;
- maintenance activity;
- license compatibility;
- dependency size;
- embedding API quality;
- event-loop/host-function integration;
- reproducible builds.

A home-grown JIT is explicitly deferred unless the project later has the engineering capacity and evidence to justify it.

## Minimum dynamic-web milestone

The first scripting milestone needs only enough host integration to validate the architecture:

- execute a script;
- read/mutate a bounded DOM surface;
- dispatch a basic event;
- schedule a task/timer;
- perform a capability-mediated network request;
- trigger style/layout/paint invalidation safely.

This is intentionally much smaller than “implement JavaScript”.
