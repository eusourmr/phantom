# Phantom — Third-Party Attributions

## Policy

Phantom is an independent browser engine, but independence does not mean
pretending external open-source work is ours. Any third-party asset or library
that becomes directly visible in the product surface must have a traceable
record containing:

1. project/resource name;
2. upstream project;
3. exact package/version or asset revision used by Phantom;
4. license;
5. role inside Phantom;
6. whether Phantom modified the resource.

This file is the human-readable attribution register for product-surface
resources. Cargo dependency licensing remains governed by each dependency's
published license metadata; a generated full dependency notice can be added
later without replacing this register.

## Lucide Icons

- **Resource:** Lucide Icons
- **Phantom package:** `lucide-icons = 1.34.0`
- **Upstream:** `lucide-icons/lucide`
- **Website:** https://lucide.dev/
- **Primary license:** ISC License
- **Historical portions:** the upstream Lucide license also preserves MIT
  licensing/copyright for icons originating from Feather Icons.
- **Use in Phantom:** native browser-chrome iconography, including navigation,
  add/close tab, pin, reload, and window controls.
- **Delivery:** Phantom uses the Lucide font bytes exposed by the Rust
  `lucide-icons` crate through `LUCIDE_FONT_BYTES`.
- **Modification:** Phantom does not redraw or claim authorship of Lucide glyphs;
  it selects glyphs and controls their size/layout/color through egui.

Upstream license reference:
https://github.com/lucide-icons/lucide/blob/main/LICENSE

## Product requirement

Before Phantom Beta, the application should expose an **About / Licenses**
surface that presents this register (or a generated equivalent) to users. Until
that UI exists, this repository document is the canonical attribution record.
