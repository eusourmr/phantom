# Phantom 2C-9 — Chrome UX: Theme, Tabs and Iconography

## Purpose

The 2C-9 browser-shell work is deliberately separate from the web engine. It
improves Phantom's own native chrome without introducing page-rendering rules
into the UI layer.

## System theme

Phantom now sets egui to `ThemePreference::System` instead of forcing the light
visual theme. The custom top chrome, central panel, active/inactive tab fills,
and floating navigation surface use the active egui visual palette instead of
hard-coded light backgrounds.

On platforms where egui/eframe receives OS theme information, Phantom therefore
tracks light/dark preference and can follow changes delivered by the platform.

### Explicit limitation

2C-9 does not read the operating system's accent color. Doing that portably
would require an additional platform integration boundary. Phantom keeps its
own brand accent while adopting the system light/dark palette. We should not
add OS-specific accent-color dependencies merely for cosmetic parity.

## Official iconography

Phantom standardizes browser-chrome icons on **Lucide Icons**, already present
in the workspace through `lucide-icons = 1.34.0`.

The 2C-9 UI uses the bundled Lucide font for navigation, add/close, pin,
reload, minimize, maximize, and related chrome actions. Icon buttons are now
frameless by default and reveal interaction through the active egui theme,
reducing the heavy square-button appearance visible in the earlier shell.

Attribution and licensing rules are recorded in
`docs/THIRD-PARTY-ATTRIBUTIONS.md`.

## Tab model

Each `BrowserTab` now has an explicit `pinned` state.

### Regular tabs

- wider tab body;
- title and close `X` live inside the same visual tab;
- right-click context menu;
- can be pinned, reloaded, or closed.

### Pinned tabs

- compact representation using the Lucide pin symbol until favicon support is
  implemented;
- pinned tabs are kept before regular tabs;
- normal close `X` is hidden to reduce accidental closure;
- close remains available through the context menu and keyboard shortcut.

Pin/unpin is a browser-shell state only. It does not alter page history or
network isolation.

## Keyboard shortcuts

2C-9 establishes the first canonical native shortcut set:

- `Ctrl+L` (Command+L on macOS): focus location field;
- `Ctrl+T` / Command+T: new tab;
- `Ctrl+W` / Command+W: close active tab;
- `Ctrl+R` / Command+R: reload active tab;
- `Esc`: dismiss the floating navigation focus/surface.

The implementation uses egui's `modifiers.command`, so the platform-appropriate
primary command modifier is used instead of hard-coding Windows-only control
logic.

## Next UI work intentionally deferred

- real site favicons for pinned tabs;
- tab drag/reorder by pointer;
- closed-tab restore (`Ctrl+Shift+T`);
- audio/activity indicators;
- tab groups;
- settings UI for theme override;
- native OS accent-color extraction;
- visible About/Licenses screen generated from the attribution register.
