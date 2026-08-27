# Phantom Standards Map — Image Loading 2C-2

## Canonical references

HTML:

- https://html.spec.whatwg.org/
- https://github.com/whatwg/html

CSS:

- https://drafts.csswg.org/css-images/
- https://drafts.csswg.org/css-sizing/
- https://github.com/w3c/csswg-drafts

Tests:

- https://github.com/web-platform-tests/wpt

## 2C-2 mapping

| Phantom behavior | Standards area | Status |
| --- | --- | --- |
| `<img>` replaced geometry | HTML + CSS Sizing | foundation active |
| intrinsic raster width/height | HTML/CSS replaced sizing | active |
| relative `src` resolution | URL/HTML fetching | basic final-document URL resolution |
| PNG/JPEG decode | codec infrastructure boundary | active |
| resource limits | Phantom security policy | active, non-normative |
| `srcset` candidate selection | HTML responsive images | deferred |
| `<picture>` | HTML responsive images | deferred |
| `object-fit` | CSS Images | deferred |
| `object-position` | CSS Images | deferred |
| animated images | HTML/image animation behavior | deferred |

## Conformance rule

A decoded image appearing on screen proves codec/resource integration only.

It does not prove complete HTML image conformance.

Official compatibility claims require the relevant upstream WPT cases to run
unchanged under a reproducible Phantom test harness.
