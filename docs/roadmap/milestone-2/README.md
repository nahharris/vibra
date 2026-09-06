# Milestone 2 step plan

Status: planned; implementation steps not started
Milestone: [Milestone 2 — executable pure core](../v1.md#milestone-2--executable-pure-core)
Execution model: [execution.md](../execution.md)
Integration branch: `m2`

M2 delivers project initialization, formatting, checking, testing, and reference
execution for the pure subset. Each numbered row below is exactly one PR into
`m2`. This planning change is the branch bootstrap, not an implemented step.

## Start here

`m2` was created from refreshed `origin/main` at
`9c77b8642e1a7f3f4d8aab1eb0cd709bbbefebf6`, the M1 integration merge
([PR #283](https://github.com/nahharris/vibra/pull/283)). M1's exit-gate evidence
is in [its step plan](../milestone-1/README.md). Its statements that PR #283 is
awaiting review describe the pre-merge evidence record; the merge above is the
M2 prerequisite. Verify ancestry again before implementation.

1. Read `AGENTS.md`, [the charter](../../spec/00-charter.md), this plan,
   [implementation](implementation.md), and [validation](validation.md).
2. Read the chosen step's guide and its exact specification sections. Start
   with Step 1: the present specifications do not settle every M2 contract.
3. Fetch `origin/m2`; branch from its current head as
   `codex/m2-step-NN-<topic>`. Do not start from `main`, `m1`, or a previous
   step's feature branch. Preserve unrelated working-tree changes.
4. Implement the earliest unfinished row after verifying its predecessors
   have merged and passed. The default execution order is strictly 1 through
   14; one model owns one step PR, including its tests and documentation.
5. Use the [handoff template](validation.md#step-handoff). Set the step's row
   to `landed` in the completing PR, conditional on that PR merging; record
   its PR and then verify the actual merge commit. Start the next step only
   from the refreshed integration head.

Keep one standing draft PR from `m2` to `main`, following the execution model.
It carries cumulative CI and stays draft until Step 14 evidences every gate.
Step PRs target `m2`; the milestone PR alone eventually targets `main`.

## Fixed implementation decisions

- **Reuse M1's frontend.** Consume its source AST and generic VIBON tree; retain
  CST origins for diagnostics and formatting. There is no second parser in the
  resolver, interpreter, CLI, or corpus adapter.
- **Add architecture nodes as needed.** Use separate `vibra-resolve`,
  `vibra-types`, `vibra-ir`, `vibra-interp`, `vibra-workspace`, and `vibra-cli`
  crates. Add each when its first real slice needs it. Update the architecture
  test in the same PR; do not create empty crates for all future phases.
- **One checked execution input.** Only successfully checked typed IR enters
  the interpreter. Neither execution nor schema rendering resolves source
  names. Keep immutable values, source origins, and deterministic traversal.
- **One snapshot service.** Workspace owns filesystem acquisition and phase
  orchestration. Semantic crates accept explicit inputs and never search disk,
  consult environment variables, invoke a shell, or download dependencies.
- **Closed coverage.** Maintain a supported-surface table, including exclusions,
  as each slice lands. M1 still parses later-v1 syntax; M2 does not execute it
  or reinterpret it. Availability is distinct from malformed syntax.
- **No backend shortcuts.** No LLVM, JIT, Wasm, host registry execution, hidden
  arithmetic operator, ambient stdlib prelude, or archived implementation is
  needed for this milestone. Pure intrinsics have reviewed exact semantics.

These are implementation constraints, not new language rules. Observable
decisions are closed in Step 1 and the owning specification before dependent
code. In particular, the signed offline bootstrap artifact, its module byte
digests, and the assertion/trap result envelopes are part of that contract; a
path named `std` cannot confer external-declaration authority.

## Steps

| Step | One-PR slice and guide | Requires | Status | PR / merge evidence |
| --- | --- | --- | --- | --- |
| 1 | [Freeze M2 contracts and corpus observations](01-contracts.md) — specification/infrastructure prerequisite | M1 merge and exit evidence | in progress | — |
| 2 | [Decode typed project data](02-project.md) | 1 | not started | — |
| 3 | [Build the confined source graph](03-source-graph.md) | 2 | not started | — |
| 4 | [Resolve module declarations and references](04-resolution.md) | 3 | not started | — |
| 5 | [Execute typed primitive functions](05-primitives.md) | 4 | not started | — |
| 6 | [Execute constants, bindings, and control flow](06-bindings.md) | 5 | not started | — |
| 7 | [Execute function values and labelled calls](07-functions.md) | 6 | not started | — |
| 8 | [Validate compiler externals and bootstrap pure stdlib](08-externals.md) | 7 | not started | — |
| 9 | [Guarantee tail calls](09-tail-calls.md) | 8 | not started | — |
| 10 | [Expose semantic position facts](10-queries.md) | 9 | not started | — |
| 11 | [Ship project init and safe formatting](11-init-fmt.md) | 10 | not started | — |
| 12 | [Ship check and interpreter run](12-check-run.md) | 11 | not started | — |
| 13 | [Ship pure tests and assertions](13-tests.md) | 12 | not started | — |
| 14 | [Audit the demo and exit gate](14-evidence.md) — evidence step | 13 | not started | — |

Step 1 is explicitly a specification and test-infrastructure prerequisite, not
a language-feature completion claim. Steps 2–13 each deliver their complete
claimed slice through a real library handler and independent corpus; the CLI
steps additionally exercise real processes. Step 14 adds reproducible evidence.

## Deliverable and gate coverage

| Roadmap obligation | Owning steps / required evidence |
| --- | --- |
| `@project.v1`, discovery, schema-selected atom roles | 1–3, 11; decode without filesystem and discovery fixtures |
| Unit-rooted import/entry paths, disjoint roots, leaf-or-directory modules | 3–4, 12; real temporary trees and cross-module corpus |
| Primitive types, numeric ranges, `char`, `def`, `defn`, direct patterns | 5–6; structural typed IR and value observations |
| `lambda`, `fn`, first-class paths, general function application | 7; captures, signatures, callee-once and binding order |
| No shadowing, visibility, repeatable identity-free discards | 4, 6–7, 10; scope and query observations |
| Pure `@compiler` externals and stdlib | 1, 8, 13; provenance, closed inventory, assertion outcomes |
| Unknown providers / Wasm FFI / retired control forms rejected | 1, 4–5, 8, 12; distinct syntax/availability/registry diagnostics |
| One typed IR and reference interpreter, no adapter dependency | 5–9, 14; architecture tests and interpreter corpus |
| Mandatory tail calls | 9, 14; source workload plus activation-depth evidence |
| Type-aware position metadata, visible names, primitive expectations | 10; recovery, Unicode, identities, schema consumer tests |
| `project init`, `fmt`, `check`, `run`, `test` | 11–13; actual binary, JSON, exits, writes, clean demo |
| Static/interpreter profiles for implemented subset | Every behavior step, 14; explicit scope and zero failed/unavailable in gate corpus |
| Pure execution has no host events or ambient observations | 5, 8–9, 12–14; empty audit traces and isolated-input tests |
| Unsupported later-v1 forms have explicit availability diagnostics | 1 and every widening step; availability inventory swept by 14 |
| Clean offline multi-module demo and tests | 14; repeat from exact checkout, commands and results recorded |

## Scope boundaries to freeze before implementation

M3 owns complete nominal data, collection operations and variadics, generics,
interfaces/methods, exhaustive patterns, `match`, `option`/`result`, `try`, and
the complete widening/conversion system. M4 owns nonempty effects and executable
`@host` operations. M5 owns dependency sync, vendor/lock validation, and ordinary
pinned stdlib dependency delivery. M6 owns full query/edit/MCP coverage; M7 owns
Wasm and release packaging.

Step 1 must make the overlaps explicit in the roadmap: the exact primitive
and `fn` subset, treatment of `as` and atom singleton widening, nonempty effect
rows, variadic signatures, the `result void e` entry alternative, and offline
stdlib bootstrap. These are pending contracts, not permission to silently
relax the language or to implement later milestones. Valid deferred syntax
keeps its M1 structure and receives the agreed availability result.

The `@tool.unavailable` diagnostic is the canonical result for a valid v1
surface outside the selected M2 implementation profile. It is separate from a
runner capability status and from malformed syntax.

## Exit evidence

Not run for M2. Planning and a passing M1 baseline do not complete any M2 gate.
Step 14 records tested commits, case counts by profile, demo artifacts, tail
depth, pure-event evidence, platform CI, and the final milestone PR state here.
