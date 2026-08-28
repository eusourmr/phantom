# Phantom Manifesto

The web became one of humanity's main interfaces with knowledge, work, commerce and communication. Yet the dominant browser model still treats people primarily as operators of pages, tabs and forms.

Phantom starts from a different premise:

> The browser should understand the user's objective, preserve context, explain what it knows, and act only within explicit human-controlled boundaries.

## Independent by design

Phantom is not intended to be a Chromium, WebKit or Gecko fork. Compatibility with the open web matters, but architectural independence matters too. The engine will be built incrementally from its own components and contracts.

## The page is not the final abstraction

Pages are containers. What people actually care about are entities, relationships, changes, decisions and actions: a flight, a price, a document, a person, a law, a reservation, a deadline.

Phantom will therefore evolve toward a semantic runtime alongside the traditional DOM.

## Memory should have meaning

History should not be only a list of URLs. With user authorization, Phantom should be able to remember context, decisions, sources and changes over time while keeping personal memory separated from telemetry and external services.

## Intelligence must not bypass security

AI is not a privileged superuser. Agents must operate through the same explicit capability and policy boundaries as every other component. High-impact actions require explicit policy evaluation and, where appropriate, human approval.

## Security is architecture

Security is not a feature added after rendering works. Phantom is designed around memory safety, least privilege, sandboxing, typed boundaries, capability-based access, explicit side effects and continuous adversarial testing.

## Auditability over cleverness

A contributor should be able to understand why code exists, which invariant it protects, what it is allowed to access, and how it was tested. Reproducibility and traceability are product requirements, not paperwork.

## Open means inspectable

The core is open source. Architectural decisions are recorded. Security-sensitive changes are reviewable. Dependencies are treated as part of the attack surface. The goal is software that can be independently studied, built and challenged.

## Human control remains final

Phantom may search, compare, organize, propose and eventually execute. But it must distinguish observation from inference and proposal from authorization.

The browser of the future should not merely open the web faster.

**It should make the web understandable, contextual and actionable without making the human invisible.**
