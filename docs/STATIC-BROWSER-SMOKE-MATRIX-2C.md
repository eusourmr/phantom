# Phantom 2C — Static Browser Smoke Matrix

This is the closure matrix for the pre-script 2C line.

| Surface | Expected 2C behavior |
|---|---|
| HTTP/HTTPS document | loads through Phantom network boundary |
| Redirects | bounded, relative Location supported, loop guarded |
| Document reload | explicit revalidation semantics |
| Document cache | bounded, fresh reuse and 304 body reuse |
| Image cache | partitioned, revalidated and recovery-aware |
| Raster images | PNG, JPEG, GIF, WebP; ICO for site identity |
| Responsive images | bounded selection pipeline |
| Lazy images | deferred/cancelled by document generation |
| Animation | bounded GIF/WebP frame lifecycle |
| Links | hover, destination preview, click navigation |
| target=_blank | new Phantom tab |
| GET forms | text/search/hidden/submit first slice |
| Fragment links | same-document navigation without refetch |
| History | Back/Forward plus entry-specific scroll |
| Site identity | ordered declared icons + ICO/root fallback |
| Page title | normalized HTML title with URL fallback |
| Chrome | tabs, pinned tabs, recently closed, native controls |

2C does not claim JavaScript, POST forms, full HTML/CSS conformance, cookies,
storage, process sandboxing or GPU composition.
