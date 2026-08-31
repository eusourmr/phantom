# Phantom 2C-15 — Site Identity II

## Candidate policy

The engine now exposes every supported document-declared icon in DOM order
through `Engine::site_icon_requests()`.

Explicit raster MIME support:
- image/png
- image/jpeg / image/jpg
- image/gif
- image/webp
- image/x-icon
- image/vnd.microsoft.icon
- image/ico

A missing MIME type remains eligible. The decoder is authoritative after fetch.
Typed SVG remains outside the current raster identity boundary.

The compatibility `site_icon_request()` API remains and returns the first
candidate.

## Document title

`Engine::document_title()` returns normalized `<title>` text for browser chrome.
The browser falls back to URL-derived title only when HTML provides no useful
title.
