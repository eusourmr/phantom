# Phantom 2D-1 — Document Error Surface

The floating navigation bar is navigation-only.

The Alpha engineering status line that previously displayed image/cache counters
below the address field is removed from the visible chrome. Internal status
strings remain available for later diagnostics/observability work.

Top-level document failures now use the content area:

- Phantom logo;
- concise error title;
- one explanatory line;
- target URL in muted text.

Examples:
- unsupported document format;
- unidentifiable response type;
- no-content response;
- partial-content response;
- network/charset/UTF-8 failure;
- engine render failure.

No permanent status bar is added.
