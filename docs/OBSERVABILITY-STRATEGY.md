# Observability Strategy

## Current position

Phantom has asynchronous navigation/resource work but does not currently rely on production `println!` debugging. Adding `tracing` without defined event semantics would add dependencies without useful observability.

## Trigger for implementation

Introduce structured tracing when one of these occurs:
- multiple concurrent navigation/resource queues need latency diagnosis;
- scripting adds task/microtask/event-loop scheduling;
- a performance benchmark requires per-stage timing;
- debugging requires correlation across network -> parse -> layout -> paint.

## Initial event model

Recommended spans/events:
- `navigation` — request id, navigation generation, cache mode, redirect count;
- `network_fetch` — scheme/host category, cache status, bytes, duration (never credentials/secrets);
- `parse_html`;
- `compute_style`;
- `layout`;
- `paint_list`;
- `image_decode`;
- `site_icon`;
- future `script_task`.

## Privacy

Tracing must not record full sensitive URLs, request bodies, credentials, cookies or page text by default. Diagnostic builds may expose more only through explicit user-controlled settings.

## Backend

`tracing` / `tracing-subscriber` are good candidates but are not mandatory until implementation. Export to Chrome Trace Event JSON or Perfetto-compatible output can be added behind a diagnostic feature.

## Non-goal

Telemetry is not enabled by this strategy. Local structured tracing and remote telemetry are separate decisions.
