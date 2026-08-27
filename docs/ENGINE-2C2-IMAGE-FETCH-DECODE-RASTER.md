# Phantom Engine 2C-2 — Image Fetch + Decode + Raster Paint

## Objective

Turn the replaced-image geometry introduced in 2C-1 into real visible raster
content without allowing networking, codec state, DOM ownership and renderer
state to collapse into one subsystem.

The active path becomes:

```text
<img src>
    ↓
LayoutSnapshot image resource id + source
    ↓
Browser resource coordinator
    ↓
phantom-net HttpUrl resolution
    ↓
phantom-net bounded binary fetch
    ↓
phantom-image probe + bounded raster decode
    ↓
egui texture upload
    ↓
PaintCommand::Image
    ↓
raster on screen
```

The engine continues to own HTML/CSS/layout semantics. The browser shell owns
the temporary resource-coordination loop. The codec remains behind
`phantom-image::ImageDecoder`.

## Formats enabled

Raster decode enabled in this milestone:

- PNG
- JPEG

Metadata probing remains available for:

- PNG
- JPEG
- GIF

GIF raster decode is deliberately not enabled yet. Animated image semantics
need a separate timing/frame model and should not be smuggled into the static
raster path.

## Mature decoder boundary

`phantom-image` uses the mature Rust `image` crate only behind the narrow
`ImageDecoder` contract.

No `image` crate type crosses into:

- DOM
- CSS
- Layout
- Paint

The decoded result is converted immediately into Phantom-owned RGBA8 data.

This follows the project dependency doctrine: mature libraries are acceptable
at narrow infrastructure/codec boundaries while Phantom retains ownership of
browser semantics and architecture.

## Network boundary

`phantom-net` now exposes a bounded binary response path in addition to the
text-document path.

Separate limits are maintained for:

- text documents
- binary subresources

The first image-resource budget is 16 MiB of downloaded bytes per response.
The body is read through ureq's explicit bounded reader.

The request advertises PNG/JPEG preference. This reduces accidental negotiation
of formats that Phantom does not yet decode.

## Decode budgets

The browser resource coordinator applies a stricter raster policy than the
absolute decoder default:

```text
max source width       8,192 px
max source height      8,192 px
max pixels            16,777,216
max RGBA8 bytes       67,108,864
```

A tab also carries an approximate raster/texture budget:

```text
256 MiB
```

The first coordinator loads at most 64 image resources from one document
revision.

These are Phantom safety policies, not Web Platform semantics. They can become
configurable later, but they must remain explicit and bounded.

## Progressive loading

The document becomes renderable before images finish downloading.

Each successfully decoded image:

1. installs intrinsic metadata into the existing engine image catalog;
2. triggers relayout without reparsing HTML or recomputing CSS;
3. uploads one texture to the native renderer;
4. replaces the existing placeholder for the same `ImageResourceId`.

This preserves the 2C-1 architecture instead of creating a second image layout
path.

## Resource identity

The current technical-preview generation maps an image element to an opaque
`ImageResourceId` derived from its stable DOM node id.

Renderer texture lookup uses only this opaque id.

The renderer never receives the image URL.

## URL resolution

The first resource coordinator resolves `src` against the final document URL
using `phantom-net::HttpUrl`.

Still deferred:

- HTML `<base>` integration
- `srcset`
- `sizes`
- `<picture>` source selection
- data URLs
- blob URLs
- CSS `background-image`

These omissions remain visible rather than being approximated incorrectly.

## Texture ownership

Decoded CPU pixels are temporary.

After validation and texture upload, the browser retains only the renderer
texture handle and an estimated raster-byte budget.

Navigation drops the per-tab texture map, allowing renderer resources to be
released naturally.

A future resource cache can move below the browser shell after cache keys,
origin partitioning and eviction policy are specified.

## Known rendering limitations

This milestone performs basic object stretching into the content rectangle.

Not yet implemented:

- `object-fit`
- `object-position`
- EXIF orientation behavior
- animated GIF
- WebP
- AVIF
- responsive image candidate selection
- lazy-loading policy
- request priority
- HTTP cache integration
- CORS/taint state for future Canvas
- image decoding off the resource worker
- color profile management

## Standards direction

Image semantics remain mapped to the project's standards doctrine:

- WHATWG HTML — `img` and image fetching/selection
- CSS Images
- CSS Sizing
- CSS replaced-element sizing
- Web Platform Tests

Codec implementation is not a substitute for those semantics.

## Next milestone

**2C-3 — Responsive Images + Object Sizing + Resource Cache v1**

Recommended scope:

- `srcset`
- `sizes` minimum useful subset
- `<picture>` source selection boundary
- `object-fit`
- `object-position`
- URL-keyed decoded/texture cache with bounded eviction
- request deduplication
- first image-loading conformance tests

Only after candidate selection is stable should newer codecs such as WebP/AVIF
be widened aggressively.
