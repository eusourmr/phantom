# Phantom 2C-11 — Chrome UX III: Site Identity I

## Objective

Give tabs a real identity supplied by the loaded site instead of relying only on host text or Phantom-owned placeholder icons.

## Discovery

The engine exposes the first supported document-declared:

```html
<link rel="icon" href="...">
```

`rel` is tokenized case-insensitively, so `shortcut icon` also qualifies.

Site Identity I accepts the raster formats already inside Phantom's bounded image pipeline:

- PNG
- JPEG
- GIF
- WebP

A missing `type` is allowed and the decoder remains authoritative after fetch.

## Browser behavior

- icon fetch uses the document's `NetworkIsolationKey`;
- HTTP cache/revalidation is reused through `phantom-net`;
- decode is bounded to 512×512 and 1 MiB RGBA for the selected static frame;
- animated icons use only their first decoded frame in browser chrome;
- navigation invalidates the old tab icon;
- stale document-generation results are ignored;
- normal tabs display favicon + title;
- pinned tabs use the favicon as their compact visual identity;
- when no supported icon is available, the existing Lucide `Pin` fallback remains.

## Deliberate non-goals

2C-11 does not synthesize `/favicon.ico`, decode ICO, render SVG favicons, implement `mask-icon`, `apple-touch-icon`, theme-color or manifest icons.

These omissions are explicit so Site Identity does not silently broaden the image-codec scope of this engine milestone.
