# Phantom 2D-3 — Navigation Transition Matrix

| Current | Event | Next | Notes |
|---|---|---|---|
| Empty | New/History/Reload network navigation | Fetching | worker owns request |
| Ready | New/History/Reload cross-document navigation | Fetching | current visual content is replaced by loading surface |
| Failed | New/History/Reload navigation | Fetching | error surface is cleared |
| Fetching | response received | Parsing | receiver leaves state before engine parsing |
| Fetching | document/network error | Failed | typed 2D-1 error becomes state-owned error |
| Fetching | worker disconnected | Failed | controlled browser error |
| Parsing | engine commit succeeds | Ready | history/title/resources commit |
| Parsing | engine parse/render preparation fails | Failed | render error becomes state-owned error |
| Ready | same-document fragment navigation | Ready | no network document transition |
| Any active fetch | stale generation guard | Empty | defensive invalidation |

## Invariants

1. `Ready` is the only state that means a committed document is renderable.
2. `Failed` is the only state that owns `DocumentPageError`.
3. `Fetching` is the only state that owns `PendingNavigation`.
4. `Fetching` and `Parsing` are the only loading states.
5. `status` never controls page routing.
6. Image/site-icon loading remains a separate subresource lifecycle.
