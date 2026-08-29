# Phantom 2C-14 — Fragment Navigation

The engine now exposes `Engine::fragment_target(fragment)`.

It resolves a visible HTML `id` to the first cold-layout position produced by
that element or its descendants. History, scrolling and URL parsing remain
outside the engine.

Deferred: full percent-decoding/Unicode fragment rules, historical `a[name]`,
text fragments, CSS `:target`, focus transfer and accessibility side effects.
