# 2C-4 FIX 1 — image-rs 0.25.10 unification

Historical implementation note preserved from the former root `Readme.txt`.

## Problem

- `phantom-image` required `image = "=0.25.10"`.
- `phantom-browser` required `image = "=0.25.8"`.
- Cargo could not resolve two conflicting exact versions in the same dependency graph.

## Correction

- `phantom-browser` → `image = "=0.25.10"`
- `phantom-image` → `image = "=0.25.10"`
- `default-features = false` preserved.
- Browser kept only the `png` feature.
- `phantom-image` enabled `png`, `jpeg`, `gif` and `webp`.

## Technical reason

`image` 0.25.10 added `metadata::LoopCount` and `AnimationDecoder::loop_count`, APIs used by the 2C-4 animated GIF/WebP work.

## Historical verification commands

```powershell
cargo update -p image --precise 0.25.10
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-image --test animation_decode
cargo test -p phantom-image --test raster_decode

taskkill /F /IM phantom-browser.exe
cargo build --release -p phantom-browser
.\target\release\phantom-browser.exe
```

This file is retained for audit history only; current build instructions live in the repository root documentation and CI configuration.
