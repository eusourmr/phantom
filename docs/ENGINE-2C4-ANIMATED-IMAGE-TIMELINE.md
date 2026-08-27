# Phantom Engine 2C-4 — Animated Image Timeline

## Objective

Add real GIF and animated-WebP playback without moving animation state into DOM,
Layout or Paint.

The resource path remains:

```text
HTML / responsive candidate selection
        ↓
phantom-net bounded bytes
        ↓
phantom-image metadata + bounded frame decode
        ↓
browser resource cache
        ↓
renderer-side animation timeline
        ↓
existing PaintCommand::Image
```

Paint remains renderer-neutral. It still carries one opaque `ImageResourceId`,
`object-fit` and `object-position`. The browser decides which decoded frame is
current at paint time.

## Formats

2C-4 supports:

- PNG static
- JPEG static
- WebP static
- WebP animated
- GIF single-frame and animated through the animation path

APNG remains outside this milestone.

## Bounded animation decode

`phantom-image` adds:

- `AnimationDecodeLimits`
- `AnimationFrame`
- `DecodedAnimation`
- `AnimationLoopCount`
- `AnimatedImageDecoder`

The default animation policy retains no more than 256 decoded frames and no
more than 128 MiB of aggregate RGBA8 data for one image.

The browser's existing 256 MiB per-tab raster budget also accounts for all
retained animation frames.

These are Phantom safety policies, not HTML/CSS semantics.

## Timing

Frame timing is decoded in `phantom-image` and returned as integer milliseconds.
The native renderer owns the monotonic clock (`Instant`) and chooses the current
texture from the decoded sequence.

A renderer-side minimum scheduling delay of 10 ms prevents malformed or zero
frame delays from creating a busy repaint loop. This scheduling floor is a
resource-safety policy and must not be presented as a Web Platform rule.

Finite animations stop on their final frame. Infinite animations continue while
the tab exists.

Only the active tab requests animation repaints. Background tabs therefore do
not create repaint work, although their monotonic animation phase advances.

## Cache integration

Static images cache one texture.

Animated images cache one immutable vector of frame textures behind `Arc`.
Multiple `<img>` elements resolving to the same URL share that vector and its
animation start time for the current document generation.

The total RGBA byte cost of all frames is charged once to the bounded resource
cache.

## Architecture invariants

- DOM stores no frame buffers.
- Layout stores no frame buffers or animation clocks.
- Paint stores no frame index and performs no animation scheduling.
- `phantom-image` exposes no `image` crate frame type.
- codecs remain an infrastructure boundary.
- the browser shell owns texture lifetime and repaint scheduling.

## Explicitly deferred

- APNG
- visibility-based pause for offscreen elements
- page lifecycle freeze/resume semantics
- HTTP cache revalidation
- animated-image streaming / incremental decode
- color-profile application
- EXIF orientation integration

## Gate

```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-image --test animation_decode
cargo test -p phantom-image --test raster_decode
```

Then build the release browser and test real GIF/WebP pages.
