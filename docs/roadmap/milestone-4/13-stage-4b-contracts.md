# Step 13 — freeze Stage 4B contracts

Prerequisite: Step 12 merged (PR #370, merge d567003; this step was written on its head and
rebased onto `origin/m4` after the merge, so that its
branch holds only Step 13 commits). This is a specification prerequisite. It
does not implement or claim any language feature, adds no host operation, and
emits no WebAssembly. No Stage 4B behavior step begins until every decision below
has canonical prose, registry or schema evidence where a code or atom is added,
and review.

## Read before editing

- [Charter](../../spec/00-charter.md): all of it; the decision order governs every
  tie below.
- [Effects](../../spec/03-effects.md): all of it; it changes in this step.
- [Runtime](../../spec/06-runtime.md): **External providers**, **Traps**,
  **WebAssembly boundary**, **Determinism and observability**, **Canonical value
  encoding**.
- [Tooling](../../spec/05-tooling.md): the command contract, the exit mapping, the
  payloads, the position envelope, and `@index.v1`.
- [Projects](../../spec/04-programs-and-packages.md): targets, **Tests**,
  **Toolchain standard-library input**.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): the registry,
  `@audit-trace.v1`, effect coverage, **Differential execution**.
- [Beyond v1](../beyond-v1/README.md#what-v1-carries-for-later-lines): the four
  obligations M4 carries, which the registry rule, the audit encoding, and the row
  record serve.
- [The Step 1 guide](01-contracts.md), which this guide follows, and the
  [ledger](decision-ledger.md).

## Decision checklist

Each row was closed by the maintainer's decision of 2026-10-17 or by the
recommendation shown, written into its owning chapter, and recorded in the ledger
with one proof owner.

| Gap | Ledger | Decision | Closed in |
| --- | --- | --- | --- |
| G12 | D17.1–D17.6 | The 17-row registry, `(result t e)` everywhere, `path`, the closed error enums, whole-value standard input | Runtime: The host registry |
| G13 | D18.1–D18.3 | The `@audit-trace.v1` event record, unknown kinds preserved, the CLI `AuditEvent` | Runtime: The audit trace; Tooling; the command-result schema |
| G14 | D19.1–D19.2 | `vibra_v1`, registry `1.N`, `since`, the derived minor, the snapshot | Runtime: Registry versioning |
| G15 | D20.1–D20.4 | The in-memory host world for `test` and the harness, `@host-world.v1`, the corpus input | Runtime: The host world; Projects: Tests; Diagnostics |
| G16 | D21.1 | `@command.program-failed` and exit code 6 | Runtime: Entry outcome; Tooling |
| G17 | D23.1–D23.2 | The effect and host diagnostics, the witness | Diagnostics: Effect and host diagnostics; Effects |
| G18 | D24.1 | The complete row of a callee; no row subtyping | Effects: Function effect rows |
| G19 | D25.1 | A row is a record | Effects; Runtime: Canonical value encoding; Tooling |
| G20 | D26.1 | No terminating source | Effects: Standard effect inventory |
| G21 | D27.1 | The manifest `host` list, `0.3.0`, five modules | Projects: Toolchain standard-library input |
| G22 | D28.1 | `effects` in the envelope and `ceiling`, `external` in the index | Tooling |
| G23 | D29.1 | The named baseline on exact metrics | [README](README.md#performance-baseline) |
| G24 | D22.1 | `vibra run TARGET -- ARG...` | Tooling: command contract |
| — | D30.1–D30.3 | The `@host` ABI, the value-access rule, `@runtime.invalid-host-value` in the schemas | Runtime: Host calls and the builders; the command-result schema |
| — | D31.1 | The ambient-read test | Diagnostics: Required implementation suites |
| — | D32.1 | The contradictions fixed | each chapter |

## Ordered work

1. Read the nine decisions and the recommendations, and build a clause table of
   what each chapter says today against what it must say. Do not reopen a decision
   of 2026-10-17.
2. Close each gap in its owning chapter, all chapters in one change, following
   [`AGENTS.md`](../../../AGENTS.md).
3. Add the two registry codes and update the table in the diagnostics chapter and
   the registry crate together; add the `@command.program-failed` atom, the
   `@runtime.invalid-host-value` trap code, and the event-record shape to the
   command-result schema; add the CLI result variant with its exit code; add the
   producer and consumer tests.
4. Update the M4 surface inventory (I10–I19) and its test, the README, and the
   ledger; write the guides for Steps 14–22 using the
   [required guide contents](../execution.md#required-contents-of-an-implementation-guide).
5. Refresh `docs/roadmap/milestone-1/syntax-examples.tsv` for the lines the edits
   moved, and review the refreshed rows.
6. Run the [pre-merge list](validation.md#before-merging-each-step) and confirm the
   corpus still reports 475 passed, 0 failed, 0 unavailable.

## Test matrix

This is a specification step; its evidence is the registry and schema tests.

- Positive: the two new codes are in the table and the registry with level
  `@error`, domain, and no fix; a `run` envelope with `@command.program-failed`,
  a `programResult`, a null `trap`, and no diagnostics validates; a trace of host
  events validates in `run` and in a `test` item; an event of an unknown kind
  validates; `@runtime.invalid-host-value` validates as a trap code.
- Negative: the strings of the earlier trace format are rejected; a host event
  with a missing or extra field is rejected; an unknown kind with a non-text field
  is rejected; a program failure with a trap, a diagnostic, or a null result, or on
  any command but `run`, is rejected.
- Recovery and boundary: the table, the registry, and the spec agree on 93 codes.
- Formatter: none; no source form changes.

## Diagnostic and schema changes

`@project.duplicate-effect-root` and `@external.owner-mismatch` (both `@error`, no
fix); `@command.program-failed` (exit 6); `@runtime.invalid-host-value` listed in the
`run` and `test` trap enumerations; `auditTrace` becomes an array of event objects.
The index, position-envelope, and `fn`-type schemas change with the steps that own
their producers (Steps 14 and 20), so that no schema ahead of its producer breaks
the existing cases; the spec states the target and each step's guide names its
schema work.

## Validation

The [pre-merge list](validation.md#before-merging-each-step) plus the focused
checks `cargo test --locked --offline -p vibra-diagnostics -p vibra-schema -p
vibra-cli` and `cargo test --locked --offline -p vibra-conformance --test
m4_contract_inventory --test diagnostic_registry --test evidence_step11`.

## Excluded

- Every implementation: no `deffect`, no effect checking, no host operation, no
  provider, no world decoder, no `vibra run` argument parsing, no wiring of
  `@command.program-failed` into `run`.
- Append, remove, and create-directory, user-supplied worlds for `vibra test`,
  replay, sealed roots, row variables, and a `dict` builder.
- The M7 build record of the required symbols and the derived minor.

## Done

Every atomic ledger row links to reviewed canonical prose and has registry,
schema, or inventory evidence; the guides for Steps 14–22 exist and are linked
from the [README](README.md#steps); the corpus and both suites still pass.

## Outcome

The [ledger](decision-ledger.md#stage-4b-contracts-step-13) records D17–D32.
Hannah made nine decisions on 2026-10-17, written in without change; the rest were
closed with the recommendation. Three decisions in the ledger were filled in by the
author with the smallest rule that fits and need Hannah's confirmation:

- **D20.4**, the defaults the decisions leave open: the clock `step` defaults to
  `1`, a monotonic read past the `u64` range is `unavailable`, a world path resolves
  from the root without links, and the random state continues across calls.
- **D17.6**, the parameter name `at`, which the checker forced: `(defn file (path
  path) …)` in a module that declares the type `path` is `@name.redeclaration` at
  the parameter, verified on the workspace checker; a signature mismatch against
  the host registry keeps `@type.type-argument-mismatch`, since the decision's
  condition, "if that has no code today", does not hold.
- **JSON `stdout` and `stderr`** carry non-UTF-8 program bytes with U+FFFD
  replacement, while the exact bytes stay in the audit-event arguments and human
  mode writes them directly; the closed `stdout: string` field leaves no other
  choice without a new field.

Two open observations are recorded rather than decided: the index field `external`
is `void` on every record of a local package, because a user package cannot write
`external:`, so it is non-void only on an embedded standard-library record; and the
target-array omission has no code of its own, so it uses `@effect.outside-ceiling`
at the `entry:` value with the witness as related spans.

Step 16 was split into 16a and 16b, and Step 18 gained the program arguments; the
reasons are in the [README](README.md#steps).
