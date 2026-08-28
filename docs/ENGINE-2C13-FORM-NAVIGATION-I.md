# Phantom 2C-13 — Form Navigation I

## Goal

Introduce the smallest useful HTML form submission slice without mutating the
DOM or pretending that Phantom already supports the full forms specification.

## Supported form contract

- `<form method="get">` and omitted/empty method;
- `action` as a raw URL reference resolved by the browser;
- `<input>` with omitted/text/search/hidden/submit type;
- submit `<button>`;
- `name`, `value`, `placeholder`, `disabled`;
- deterministic successful name/value controls in DOM order;
- submitter name/value only when the submit button itself initiated submission.

POST is explicitly rejected. It is never silently converted to GET.

## State ownership

The DOM remains an immutable parsed document snapshot.

User-edited values live in `BrowserTab::form_values`, keyed by
`FormControlId`. The map is reset when `document_generation` changes.

This avoids prematurely introducing mutable DOM semantics before the planned
script-ready generational-handle milestone.

## Query encoding

The engine returns structured `(name, value)` fields.

The browser resolves the form action with `HttpUrl`, and `phantom-net`
delegates query serialization to `url::Url::query_pairs_mut`.

No custom percent-encoder is introduced.
