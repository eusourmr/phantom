# Phantom 2C-15 — Navigation Polish

## Favicon fallback lifecycle

The browser resolves document-declared favicon candidates in declaration order,
deduplicates resolved URLs and attempts them sequentially.

If all declared candidates fail, `/favicon.ico` is appended as the final
same-site fallback.

One malformed, unsupported or unavailable favicon therefore no longer prevents
a later valid candidate from becoming site identity.

## Titles

Successful document commits now use the real normalized HTML `<title>` when
available. URL-derived titles remain the fallback.

## Preserved behavior

The milestone does not alter:
- 2C-14 fragment/history lifecycle;
- 2C-13 browser inputs and GET forms;
- 2C-12 link hit testing and target preview;
- document/image cache semantics;
- tab pinning/recently closed;
- full-width top tab strip;
- approved Maximize2 controls.
