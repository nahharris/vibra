# v0.0.2 M3: identity and repair loop

**Planned.** The services an agent needs to inspect, change, and retest a
project, over ordinary source files.

## Scope

- **Snapshots and identity.** Source files and pinned snapshots are the
  authority. Each declaration has its anchor identity; its claim (signature,
  contracts, effect row) and its witness (body) are hashed separately. A
  content-addressed store is at most a derived cache that can be deleted.
- **Protected evidence.** Test bodies, fixtures, and the runner and policy
  versions are pinned for each run and distinguished from the code under
  repair. An approval names what it approved; it is void when the test, the
  policy, or the claim changes.
- **Queries.** Read-only, bounded, versioned: declaration, references, type,
  contracts, effect row, grant requirements, and diagnostics at a position
  or entity. Results name the snapshot they describe.
- **Bounded evaluation.** Evaluate a pure expression against a snapshot
  under stated limits. Exhaustion is an incomplete outcome, not a failure of
  the program.
- **Edits.** Preview a file edit against a named snapshot revision and apply
  it atomically. A preview shows identity-affecting changes, such as a
  rename. A stale revision is rejected.
- **Structured test results.** Failures carry spans, expected and actual
  encodings, and the audit trace.
- **Generated references.** The syntax card, checked examples, and grammar
  and schema data are generated from the implementation and checked against
  it.

One workspace engine serves the CLI. The command names start from the
existing hierarchy; `query` and `edit` become available.

## Exit evidence

- The repair scenario of the [release gate](../README.md#release-gate), from
  a defect seeded by someone other than the author of the repair run.
- Rejections: stale edit, replaced test, changed policy, changed claim under
  an old approval.
- Every machine output validates against its schema.

## Not in this milestone

An authoritative code database, a publication workflow, persistent sessions
with live redefinition, MCP, and editor integrations.
