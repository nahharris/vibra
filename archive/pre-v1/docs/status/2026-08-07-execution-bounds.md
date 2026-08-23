---
title: Execution bounds implementation status
category: status
status: blocked
updated: 2026-08-07
issue: 251
---

# Execution bounds (#251)

This is the canonical implementation-status snapshot for issue #251. It is a
status note, not a replacement for the project-manifest or async-runtime
reference contracts.

## Safe subset landed

- `ScopeLimits` carries fuel and memory ceilings with monotone child-scope
  narrowing and active-ancestor accounting.
- Fuel is charged at function entry and loop headers in both execution
  backends. Exhaustion cancels/closes the active scope and preserves cleanup
  ordering; it is reported as an uncatchable execution abort.
- Memory is charged as deterministic logical high-water accounting whenever
  the Wasm host arena inserts a value. This is intentionally not described as
  a physical working-set bound.
- `project.vib` accepts optional `(limits fuel: ... memory: ...)` declarations,
  and project/run/test/package paths load them independently from capability
  grants.
- Focused tests cover fuel and memory exhaustion, loop charging, deadline
  precedence, narrowing rejection, nested inheritance, abort cleanup, and
  manifest lookup.

## Blocker: sound arena reclamation

The arena is a single append-only vector of untagged absolute handles. The
current executed language has no source-level scope construct aligned with
the standalone async scheduler scopes. Values can escape through returns,
`join`, task captures, mutable/reference `Rc` cells, frame/binding side tables,
and host handles. Reclaiming a candidate region without proving all of those
routes safe would create stale-handle aliasing or orphan a host resource.

Therefore this implementation does not reclaim arena regions and does not
pretend that scope teardown proves value escape safety. The required follow-up
is generation-tagged handles plus a dedicated escape/ownership analysis, with
explicit treatment of function frames, task captures, aliases, bindings, and
host handles. The concrete evidence is recorded in
[`251-escape-analysis.md`](../plans/risk-findings/251-escape-analysis.md).
