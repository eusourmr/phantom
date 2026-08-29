# Phantom 2C-15 — Bounded ICO Boundary

ICO support is added without a new crate.

The existing `image = 0.25.10` dependency enables its `ico` feature. Phantom
still performs its own bounded directory probe before full decode.

The ICO probe:
- validates ICONDIR length;
- validates the declared directory count;
- validates the complete directory table exists;
- derives candidate dimensions from directory entries;
- applies the existing `ImageDecodeLimits` before raster decode.

Site icons retain their stricter 512x512 / 262,144-pixel / 1 MiB RGBA policy.

ICO is static for Phantom's animation routing.
