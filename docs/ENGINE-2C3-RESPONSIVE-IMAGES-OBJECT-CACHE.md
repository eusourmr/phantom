# Phantom Engine 2C-3 — Responsive Images + Object Sizing + Resource Cache v1

## Objective

2C-3 moves Phantom from one-URL-per-`img` rendering into the first responsive
image pipeline while preserving the walls established in 2C-1 and 2C-2.

The browser still does not parse HTML or CSS semantics. The engine selects the
HTML image candidate, the style system computes object sizing, the browser
coordinates network/cache/decode, and Paint remains renderer-neutral.

## Pipeline

```text
HTML / DOM
   ↓
responsive candidate selection
   ↓
ImageRequest { resource, selected source }
   ↓
HTTP(S) resolution
   ↓
document-generation image cache
   ↓
bounded fetch + decode
   ↓
metadata → relayout
   ↓
texture binding
   ↓
PaintCommand::Image { rect, fit, position }
   ↓
raster paint
```

## Responsive images slice

Implemented in this milestone:

- `img[src]` fallback;
- `img[srcset]` density descriptors such as `1x`, `2x`;
- `img[srcset]` width descriptors such as `400w`, `800w`;
- `sizes` using `px`, `vw`, and simple `min-width` / `max-width` media clauses;
- `<picture>` with source-order selection;
- `<source media>` with the same bounded width-media subset;
- `<source type>` for PNG, JPEG and WebP;
- device-pixel-ratio input through `Engine::image_requests_for_device`.

This is a deliberate first standards slice. It is not a claim of complete
WHATWG responsive-image candidate selection.

Still deferred:

- `calc()` in `sizes`;
- compound media conditions (`and`, `or`, ranges, orientation, etc.);
- data URLs and commas embedded inside `srcset` URL tokens;
- full MIME sniffing and type support matrix;
- DPR/reactive candidate reselection after live monitor movement;
- complete `sizes="auto"` semantics.

## Object sizing

The computed-style layer now contains typed values:

```text
ObjectFit
ObjectPosition
```

Supported `object-fit` values:

- `fill`;
- `contain`;
- `cover`;
- `none`;
- `scale-down`.

`object-position` supports the first percentage/keyword subset, including
`left`, `center`, `right`, `top`, `bottom`, percentages, and both keyword axes.

Paint carries the resolved values but does not read CSS. The native renderer
uses intrinsic texture dimensions plus the content-box geometry to calculate
raster destination/UV geometry.

## WebP static decode

`phantom-image` now enables the mature WebP codec behind the existing
`ImageDecoder` contract.

The external codec remains fully hidden from DOM, Style, Layout and Paint.

Static WebP is supported. Animated WebP is explicitly rejected by the decoder
until Phantom has a bounded animation timeline and frame disposal policy.

GIF remains metadata-only for the same reason.

## Resource Cache v1

The first cache is intentionally conservative:

- scoped to the current document generation in a tab;
- key is the resolved absolute HTTP(S) URL;
- identical URLs are grouped before fetch/decode;
- one decoded texture can bind to multiple `ImageResourceId` values;
- decoded raster memory is bounded by the existing 256 MiB per-tab policy;
- least-recently-used entries can be evicted to respect the memory budget;
- cache is cleared on navigation so Phantom does not invent HTTP freshness
  semantics before implementing validators/cache-control correctly.

This is deliberately **not** yet an HTTP cache.

Future network-cache work must implement Cache-Control, validators, Vary and
partitioning rather than treating URL equality as permanent freshness.

## Architecture invariants preserved

- DOM never stores decoded pixels.
- Layout never owns texture handles or decoders.
- Paint never performs `srcset` selection.
- Browser shell never parses responsive-image HTML semantics.
- `phantom-image` remains the only owner of codec-library types.
- resolved URLs remain inside the `phantom-net::HttpUrl` boundary.
- no permanent pointer-heavy resource graph is introduced.

## Standards references

Primary references remain:

- WHATWG HTML — images, `picture`, `source`, `srcset`, `sizes` and candidate selection;
- CSS Images — object sizing and positioning;
- CSS Sizing / replaced elements;
- Web Platform Tests responsive-images and object-fit families.

Observed behavior in Chromium, WebKit or Gecko remains diagnostic evidence, not
Phantom's normative source.

## Exit gate

```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-engine --test responsive_images
cargo test -p phantom-image --test raster_decode
```

Only after those commands pass in the Rust 1.95 workspace should 2C-3 be called
homologated.

## Next recommended milestone

**2C-4 — Animated Images + Image Lifecycle**

Proposed scope:

- GIF decode and frame timeline;
- animated WebP;
- frame duration / loop count / disposal;
- bounded frame memory;
- offscreen animation throttling;
- image cancellation and navigation generation IDs;
- image error state and retry contract.
