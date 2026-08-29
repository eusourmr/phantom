# Phantom Guardian

Phantom Guardian is the planned local-first protection intelligence layer for Phantom. Its purpose is to observe security-relevant behavior, correlate signals that deterministic controls see only in isolation, explain risk clearly to the user and recommend safer choices without becoming part of the browser's root of trust.

Guardian is intentionally subordinate to the security kernel. Origin rules, network policy, sandbox boundaries, capability checks, resource budgets, permission enforcement and explicit human approval remain deterministic and authoritative. A Guardian assessment may influence UI, logging or a recommendation, but it cannot create a permission, bypass a policy decision, silently access privileged resources or turn an advisory signal into an unreviewed high-impact action.

The intended implementation path is incremental: begin with typed security events and deterministic heuristics, add risk scoring only when the signals are stable and observable, and consider a compact local model later if it provides measurable defensive value without unacceptable memory, CPU, privacy or attack-surface cost. Page content and personal data are not to be transmitted to an external AI service by default, and any future optional remote intelligence must be explicit, separable and governed by user-controlled policy.

The authoritative one-paragraph **Guardian Security Event Contract** is defined in [Architecture.md](../Architecture.md). Runtime implementation is deliberately deferred until the event producers, process isolation and capability boundaries are mature enough to make Guardian an observer rather than a privileged shortcut.
