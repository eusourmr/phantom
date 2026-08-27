# Standards Map — Phantom Responsive Images

## Canonical source families

HTML behavior is implemented against the WHATWG HTML Standard and the
`whatwg/html` source repository.

CSS object sizing behavior is implemented against CSSWG Editor Drafts in
`w3c/csswg-drafts`, primarily CSS Images and CSS Sizing.

Interoperability tests should be mapped to the upstream Web Platform Tests
repository without silently changing upstream expectations.

## 2C-3 implementation map

| Feature | Phantom owner | 2C-3 status |
|---|---|---|
| `img src` | Engine resource discovery | supported |
| density `srcset` | Engine candidate selector | first executable slice |
| width `srcset` | Engine candidate selector | first executable slice |
| `sizes` | Engine candidate selector | px/vw + simple width media |
| `picture/source` | Engine + DOM | first executable slice |
| `source type` | Engine | PNG/JPEG/WebP types |
| `object-fit` | CSS → Paint → Renderer | five core keywords |
| `object-position` | CSS → Paint → Renderer | percentages + core keywords |
| WebP | phantom-image | static only |
| duplicate image URL | browser resource coordinator | one fetch/decode per document batch |
| raster memory | browser resource cache | bounded |

## Conformance rule

A passing Phantom-native regression test is not reported as an upstream WPT
pass unless the unmodified upstream WPT test is actually executed in Phantom's
future WPT harness.

Unsupported syntax must fall back predictably; it must not be presented as full
standards support.
