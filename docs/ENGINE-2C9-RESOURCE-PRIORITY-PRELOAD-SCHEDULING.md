# Phantom 2C-9 — Resource Priority + Preload Scheduling

## Scope

2C-9 adds the first explicit resource scheduler on top of the 2C-8 partitioned
network/cache boundary. The engine remains responsible for HTML semantics. The
browser shell remains responsible for URL resolution, scheduling, networking,
decoding, and native UI.

The scheduler is intentionally limited to image resources because images are
the only fetched subresource class currently implemented end-to-end in Phantom.
No CSS, font, script, media, or speculative parser behavior is claimed here.

## Engine contract

`phantom-engine` exposes two independent concepts:

- `ResourcePriority::{High, Auto, Low}` parsed from HTML `fetchpriority`;
- `ImagePreloadRequest` discovered from `<link rel="preload" as="image">`.

`ImageRequest` now carries both `loading` and `priority`. These are deliberately
separate signals:

- `loading` expresses viewport-related eagerness (`eager`/`lazy`);
- `fetchpriority` expresses relative scheduling priority (`high`/`auto`/`low`).

The browser scheduler orders priority before viewport distance, so a high
priority image is not silently demoted simply because another resource appears
earlier in the DOM.

## Image preload slice

This version recognizes the following bounded preload subset:

- `rel` token list containing `preload`;
- `as="image"`;
- `href`;
- `imagesrcset`;
- `imagesizes` using Phantom's existing bounded `sizes` implementation;
- simple `media` expressions already supported by the responsive-image slice;
- supported image `type` values;
- `fetchpriority`.

A preload is not decoded or painted. The browser fetches it into the existing
2C-7/2C-8 bounded, partitioned HTTP cache. If the corresponding `<img>` later
requests the same URL in the same `NetworkIsolationKey`, its normal fetch can
reuse that cached response before decoding.

This is a deliberate architecture boundary: preload warms network state; the
normal image path owns decode, metadata installation, raster budgeting, texture
creation, and paint.

## Scheduling order

Within one image resource batch Phantom currently orders work by:

1. `fetchpriority`: High, Auto, Low;
2. preload hints before normal image decode at the same priority;
3. `loading`: Eager before Lazy;
4. document position (`top`) for remaining ties.

Lazy images outside the existing viewport margin remain deferred. Explicit
preloads are always eligible for the immediate resource batch.

The scheduler remains generation-scoped and cancellation-aware. Navigating away
invalidates outstanding work through the existing document generation and
atomic cancellation mechanism.

## Privacy invariant

Every preload carries the same `NetworkIsolationKey` as ordinary images. A
preload from one top-level origin cannot warm the cache partition of another
top-level origin merely because the resource URL is identical.

2C-9 does not weaken the 2C-8 cache partitioning invariant.

## Not complete in 2C-9

This version does **not** claim:

- the full HTML preload processing model;
- `crossorigin` credential-mode semantics;
- CSP integration;
- `referrerpolicy` integration;
- `integrity`/SRI;
- HTTP `Link` header preloads;
- Early Hints (103);
- module/script/style/font preload;
- parser-level speculative loading;
- Priority Hints behavior for resource classes other than images;
- HTTP/2 or HTTP/3 stream priority control.

`fetchpriority` here is a browser scheduler hint, not a claim that Phantom can
currently map priority to protocol-specific transport stream weights.

## Standards references

- WHATWG HTML — link type `preload`
- WHATWG HTML — `fetchpriority` / fetch priority attribute
- Fetch Standard — request priority concepts

The executable tests in `crates/phantom-engine/tests/resource_priority.rs`
cover the supported 2C-9 slice rather than claiming full WPT conformance.
