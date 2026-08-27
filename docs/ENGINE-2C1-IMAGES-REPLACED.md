# Phantom Engine 2C-1 — Images + Replaced Elements Boundary

## Objective

Introduce images without allowing image fetching, decoding, layout and painting
to collapse into one subsystem.

The active pipeline becomes:

HTML
  ->
DOM
  ->
ComputedStyle
  ->
Image metadata boundary
  ->
LayoutSnapshot
  ->
PaintList
  ->
Native Renderer

The image decoder remains outside DOM, CSS, Layout and Paint.

## New crate

`phantom-image`

This crate owns image-resource data contracts, not browser UI.

It currently provides:

- `ImageResourceId`
- `ImageFormat`
- `IntrinsicSize`
- `ImageMetadata`
- `ImageDecodeLimits`
- `DecodedImage`
- `ImageCatalog`
- `ImageDecoder`
- `probe_image(...)`

## Safe metadata probing

The first metadata probe recognizes intrinsic dimensions for:

- PNG
- GIF
- JPEG

It does not decode pixels.

All discovered dimensions pass through `ImageDecodeLimits` before they are
accepted.

The default policy currently bounds:

- width
- height
- pixel count
- decoded RGBA8 byte budget

These values are Phantom resource-safety policy, not Web Platform semantics.
They remain configurable.

## Decode boundary

`ImageDecoder` is deliberately narrow:

```text
probe(bytes, limits)
decode(bytes, limits)
```

A future production decoder can be plugged into this contract.

The decoder must not own:

- DOM
- CSS
- Layout
- Paint
- browser chrome

`DecodedImage` validates that an RGBA8 buffer exactly matches its declared
dimensions before it can enter a future renderer resource cache.

## Replaced element layout

`<img>` is now represented as:

```text
LayoutKind::Image
```

The cold layout snapshot stores only:

- geometry
- source NodeId by value
- opaque ImageResourceId
- compact source-string range
- compact alt-text range
- padding / border / margin

There are no decoded pixels inside LayoutSnapshot.

## Intrinsic dimensions

When metadata exists in `ImageCatalog`, Layout can use the natural width,
natural height and natural aspect ratio.

The current engine maps:

```text
ImageResourceId(NodeId::as_u64())
```

for the active document generation.

This is intentionally an opaque generation-local identity, not a permanent URL
cache key.

A URL/resource cache identity will be designed in the resource-fetch milestone.

## HTML dimension attributes

The first replaced-image implementation recognizes:

- `width`
- `height`

as non-negative integer dimension hints.

CSS width/height still participate in used sizing.

When one axis is specified and a natural aspect ratio exists, Phantom derives
the auto axis from that ratio.

## Default object size

For an image element with a non-empty `src` whose intrinsic metadata is not
available yet, Phantom uses the Web Platform default object-size foundation of:

```text
300 × 150 CSS px
```

This is a temporary concrete-size input while resource metadata is pending.

It is not a claim that broken-image rendering is fully implemented.

## CSS interaction

The current subset integrates image geometry with:

- width
- height
- percentage width
- box-sizing
- padding
- border
- margin
- min-width
- max-width
- min-height
- max-height
- intrinsic aspect ratio

Images participate in normal inline flow.

Block-displayed images also generate replaced boxes.

Direct flex-item images use the existing Flexbox allocation path instead of
creating a second layout engine.

## Paint

`PaintCommand` gains:

```text
Image {
    rect,
    resource,
    alt
}
```

The command contains no DOM pointer and no decoder object.

The content rectangle is calculated from the cold border/padding geometry.

The current egui shell renders an unresolved image placeholder with alt text.

This is deliberate.

Actual raster pixels will be connected in 2C-2 without changing image layout
semantics.

## Engine resource installation

`Engine` now owns an `ImageCatalog`.

A resource subsystem can install intrinsic metadata through:

```text
install_image_metadata(...)
```

The engine then performs relayout and rebuilds PaintList.

This establishes the future asynchronous sequence:

```text
HTML parsed
   ↓
initial image placeholder geometry
   ↓
resource metadata arrives
   ↓
catalog update
   ↓
relayout
   ↓
new PaintList
```

No HTML reparsing or CSS recascade is required for metadata-only image updates.

## Standards mapping

Primary references for this milestone:

- WHATWG HTML — `img` and dimension attributes
- WHATWG HTML Rendering — replaced elements
- CSS Images — object-size negotiation
- CSS 2 / CSS Sizing — replaced width/height rules

Phantom treats specification text as the source of semantics.

Existing browser behavior remains interoperability evidence, not the
specification itself.

## Security direction

Image data is attacker-controlled binary input.

Therefore:

- dimensions are checked before decoded allocation;
- pixel budgets are explicit;
- decoded byte budgets are explicit;
- decoder ownership is isolated;
- Layout never parses image bytes;
- Paint never parses image bytes;
- DOM never owns decoded pixel memory.

Future decoders should be fuzzed independently.

## Explicitly not implemented yet

2C-1 does NOT claim complete image support.

Deferred:

- network image fetching
- URL resolution and base URL integration
- redirects / CORS image policy
- real PNG/JPEG/GIF pixel decode
- WebP
- AVIF
- SVG image resources
- animated images
- `srcset`
- `<picture>`
- density descriptors
- `sizes`
- `loading=lazy`
- `decoding`
- `object-fit`
- `object-position`
- broken-image icon behavior
- full alt-text fallback rendering
- image texture cache
- GPU upload
- resource cache keys
- WPT image/replaced-element runner slice

## Exit gate

The milestone is complete only when:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-layout --test replaced_images
```

and the release browser still builds.

## Next recommended milestone

**2C-2 — Image Fetch + Decode + Raster Paint**

Recommended scope:

- resource request identity
- URL/base resolution
- bounded image fetch
- decoder backend integration
- decoded-image cache
- first actual raster paint
- renderer resource lookup by ImageResourceId
- PNG first
- JPEG second
- failure placeholder
- targeted fuzz tests
