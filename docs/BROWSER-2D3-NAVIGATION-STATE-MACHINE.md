# Phantom 2D-3 — Navigation State Machine

## Goal

Replace the implicit top-level navigation state spread across:

- `pending: Option<PendingNavigation>`
- `page_loaded: bool`
- `document_error: Option<DocumentPageError>`

with one explicit source of truth:

- `NavigationState`

`status: String` remains internal engineering/diagnostic information. It does
not decide which browser surface is rendered.

## States

```text
Empty
  -> Fetching
  -> Parsing
  -> Ready

Fetching
  -> Failed

Parsing
  -> Failed

Ready
  -> Fetching

Failed
  -> Fetching
```

Same-document fragment navigation keeps the state `Ready` because no new
document representation is fetched or parsed.

## Types

`NavigationPhase` is the lightweight observable phase:

- Empty
- Fetching
- Parsing
- Ready
- Failed

`NavigationState` owns the state-specific data:

- `Fetching(PendingNavigation)` owns the receiver/action/generation;
- `Parsing(NavigationAction)` preserves the navigation intent while the engine
  synchronously parses/commits;
- `Failed(DocumentPageError)` owns the page error;
- `Ready` means one committed document is active;
- `Empty` means no committed document and no active navigation.

## Mutation boundary

State mutation is centralized in `BrowserTab`:

- `begin_fetching`
- `begin_parsing`
- `mark_navigation_ready`
- `fail_navigation`
- `clear_navigation_state`

No direct `tab.navigation = ...` assignments exist outside those methods.

## UX

The content surface now derives strictly from `NavigationPhase`.

Loading remains centered and gains action-specific copy:

- Abrindo página
- Restaurando histórico
- Recarregando página

The Phantom logo is shown above the loading indicator, matching the established
empty/error content-surface language.

No status bar or diagnostic chrome is introduced.
