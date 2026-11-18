# v0.0.2 M0: specification and transition

**Planned.** Make the written contracts complete and consistent before any
implementation relies on them.

## Scope

- **Specification review.** Review the eight chapters under `docs/spec/`
  against each other and against the carry-over inventory of current Vibra (741 construct ids, in
  the scratchpad debate record). Every construct of v0.0.1 is kept, changed,
  deferred, or dropped by an explicit rule or exclusion.
- **Specification tests.** Two host tests tie the specification to the
  code: the diagnostic registry table, and the inventory of source examples.
  Both fail against the redesigned specification, because the code still
  implements v0.0.1. They are marked ignored, with this milestone named as
  the reason, when the specification merges. This milestone restores the
  registry test against the new table; M1 restores the example inventory
  when the new reader exists.
- **The code store.** The workspace has a content-addressed codebase store:
  the `vibra codebase` commands, the `codebase` module of
  `vibra-workspace`, its SQLite dependency, its conformance cases, and
  `examples/codebase-demo`. Source files are the authority and nothing
  stages or publishes code, so the store is removed, with its diagnostic
  codes and schema fields.
- **Owners.** For every supported form, name its conformance case, its
  diagnostics, and its schema. A form without all three is not supported.
- **Frozen contracts.** Grammar and reserved words; the certified predicate
  fragment and the seven discharge rules, with the listed integer facts;
  constructor result types under `where … else`; effect rows, plans, and
  grants; policy document kinds; the `@project.v2` record; the entry
  outcome; identity scheme and schema versions; every exclusion.
- **Native contract.** Compare the existing lowering, execution IR, bridge,
  and runtime code with the new checked-IR contract. Record what is kept,
  what changes, and what is removed, before M4 work starts.
- **Windows file-scope feasibility.** Build the smallest adapter that opens
  a file relative to a held directory handle without following links or
  reparse points, checks the actual entry name, and does not truncate on
  first open. Run it against positive controls and the attack cases of the
  [effects chapter](../../../spec/03-effects.md). Record the result.
- **Pilot registration.** Keep the registered draft of the
  model-writing pilot current with the specification. It is not run in this
  milestone.

## Exit evidence

- A cross-chapter review record with no open contradiction.
- The owner table: form, conformance case, diagnostics, schema.
- The native contract comparison.
- The feasibility record. If the adapter cannot meet the contract, the file
  operations it would back are marked unsupported on Windows, M2's gate
  program cannot pass there, and the plan returns to the specification
  before M2 starts.

## Not in this milestone

Implementation of any language feature, and any model call.
