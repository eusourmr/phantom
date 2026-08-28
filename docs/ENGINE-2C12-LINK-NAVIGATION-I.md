# Phantom 2C-12 — Link Navigation Semantics I

## Objective

Make hyperlink interaction an engine concept derived from the same immutable
layout snapshot that is painted to the screen.

The browser shell should not reverse-engineer links from paint pixels or walk
the DOM independently.

## Engine contract

2C-12 introduces `LinkRegion`.

A region carries:

- the raw `href` declared by the nearest ancestor `<a href>`;
- a document-coordinate `Rect`;
- whether `target="_blank"` requests a new browsing context.

The engine exposes:

- `Engine::link_regions()`;
- `Engine::link_at(x, y)`.

## Snapshot rule

Link geometry is rebuilt whenever layout is rebuilt, including image-induced
relayout and viewport resize. There is no mutable pointer from the layout
snapshot back into the DOM.

Only rendered text/image fragments are interactive in this first slice.
An `<a>` with no generated text/image box does not acquire an artificial click
region.

## URL ownership

The engine deliberately preserves raw `href`.

Absolute/relative URL parsing and HTTP/HTTPS policy remain owned by
`phantom-net::HttpUrl` in the browser navigation layer.

This prevents URL policy from leaking into DOM/layout.

## Deliberate limits

2C-12 does not implement:

- same-document fragment scrolling;
- `download`;
- named browsing contexts other than `_blank`;
- `mailto:`, `tel:`, custom schemes or external protocol handlers;
- middle-click;
- context-menu link actions;
- visited-link styling;
- JavaScript click/event cancellation.

Those require later browser/script/security contracts.
