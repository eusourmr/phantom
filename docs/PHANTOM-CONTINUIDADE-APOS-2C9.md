# Phantom — Continuidade após 2C-9

## Baseline homologável

2C-9 is the candidate baseline after 2C-8 Network Isolation Key + Cache
Partitioning.

The version has two intentionally separated deliverables:

### Engine/network resource path

- `ResourcePriority::{High, Auto, Low}`;
- `<img fetchpriority>` propagation;
- `<link rel="preload" as="image">` discovery;
- `href`, `imagesrcset`, `imagesizes`, bounded media/type support;
- priority-aware resource ordering;
- preload-only fetch into the partitioned HTTP cache;
- document-generation cancellation preserved;
- no new dependency;
- no relaxation of 2C-8 `NetworkIsolationKey` partitioning.

### Native browser shell

- follows egui system light/dark theme preference;
- hard-coded light fills removed from principal chrome surfaces;
- Lucide established as canonical open-source iconography;
- third-party attribution policy added;
- regular tab close button moved inside the tab;
- pinned tabs implemented and kept before regular tabs;
- tab context menu: pin/unpin, reload, close;
- canonical shortcuts: Command/Ctrl+L, T, W, R;
- frameless icon buttons for a cleaner chrome.

## Required gates

```powershell
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-engine --test resource_priority
cargo test -p phantom-net --test http_cache
cargo test -p phantom-net --test cache_partitioning
```

Then:

```powershell
taskkill /F /IM phantom-browser.exe
cargo build --release -p phantom-browser
.\target\release\phantom-browser.exe
```

## Visual validation

1. Confirm Windows light mode produces a light Phantom chrome.
2. Change Windows to dark mode, restart Phantom if the platform does not push a
   live theme event, and confirm dark chrome without white central surface.
3. Create several tabs with Ctrl+T.
4. Confirm the X is inside each regular tab.
5. Right-click a tab and pin it.
6. Confirm the pinned tab becomes compact and moves before regular tabs.
7. Confirm pinned tab has no normal X but can be closed from context menu or
   Ctrl+W when active.
8. Validate Ctrl+R reload and Ctrl+L location focus.
9. Re-test image-heavy pages and verify there is no regression in 2C-7/2C-8
   cache behavior.

## Recommended next engine milestone

**2C-10 — Navigation Robustness + Redirect/Document Cache Semantics**

Why next: image subresources now have responsive selection, animation, lazy
loading, cancellation, cache revalidation, partitioning, priority, and preload.
The highest-value next step is to harden the top-level document path rather than
keep adding image-only features.

Candidate 2C-10 scope:

- explicit navigation request policy;
- top-level response cache semantics separate from subresource cache;
- redirect-chain observability and limits;
- reload semantics (`normal` vs revalidation intent);
- structured navigation errors/status;
- groundwork for history restoration without re-fetching unsafe state.

UI refinements should continue incrementally without changing the engine
milestone numbering or coupling native chrome state to Web Platform semantics.
