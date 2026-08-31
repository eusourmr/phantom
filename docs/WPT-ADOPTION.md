# WPT Adoption Strategy

## Goal

Measure Phantom against external web-platform behavior instead of only project-authored examples.

## Principles

- WPT is an oracle/corpus, not a runtime dependency.
- Test inputs must be pinned to a known upstream commit.
- Ordinary CI must remain deterministic and bounded.
- Phantom initially runs only tests whose platform surface it claims to implement.
- A failing unsupported test is not hidden as “supported”; support manifests must be explicit.

## Adoption stages

### Stage A — parser fixtures
Import a small pinned subset of HTML/CSS parsing cases into repository fixtures. Record upstream path and commit for every imported case.

### Stage B — compatibility harness
Create a harness that maps WPT inputs to Phantom's public parser/layout APIs and records pass/fail/unsupported.

### Stage C — periodic broader WPT job
A scheduled or manually triggered CI job may fetch the pinned WPT revision and run a broader supported subset. Do not fetch the internet during ordinary unit tests.

### Stage D — browser-level WPT
After scripting/events/forms mature, add browser-level harness support where test semantics require a live browsing context.

## Metrics

Report:
- tests attempted;
- tests passed;
- tests failed;
- tests explicitly unsupported;
- upstream WPT revision.

Do not publish a percentage without its exact selected corpus.
