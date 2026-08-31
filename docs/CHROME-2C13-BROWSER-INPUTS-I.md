# Phantom 2C-13 — Browser Inputs I

The native Phantom shell overlays egui widgets exactly on the control rectangles
produced by `phantom-layout`.

Supported interaction:

- click/focus text and search fields;
- single-line typing/editing;
- placeholder;
- initial HTML value;
- disabled text inputs are non-interactive;
- Enter submits the owning GET form without a submitter field;
- clicking submit sends that submitter's name/value;
- submit buttons use `<input value>` or `<button>` descendant text as label.

Form edits are tab-local and document-generation-local.

## Preserved browser behavior

2C-13 keeps:

- 2C-12 clickable links and target preview;
- Ctrl/Cmd-click and target=_blank link behavior;
- 2C-11 site identity/favicons;
- recently closed tabs;
- the full-width top tab strip;
- Maximize2 and the validated window-control placement.
