# Phantom 2C-12 — Link Interaction UX

## Visible behavior

Supported `<a href>` content now behaves like a browser link:

- pointer changes to a pointing hand;
- resolved target URL appears in a compact lower-left preview;
- primary click navigates the current tab;
- `target="_blank"` opens the link in a new Phantom tab;
- Ctrl/Cmd-click opens the link in a new Phantom tab.

The preview resolves relative links against the committed page URL, not against
whatever unsubmitted text may currently be present in the floating address bar.

## Security boundary

Only targets accepted by `HttpUrl` can be activated in this milestone.
Unsupported schemes may be previewed as raw `href` text but are not dispatched
to the operating system.

Phantom therefore does not accidentally turn a webpage into an arbitrary
external-protocol launcher.

## Chrome preservation

2C-12 does not change:

- the full-width tab strip validated in 2C-11 FIX 3;
- site favicons;
- pinned tabs;
- recently closed tabs;
- the approved `Maximize2` window-control icon;
- floating navigation behavior.
