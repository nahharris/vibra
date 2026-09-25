# Step 1 — freeze Stage 3A contracts

Prerequisite: `m3` at or after the bootstrap commit; M2 integrated into `main`.
This is a specification/infrastructure prerequisite. It does not implement or
claim any language feature. No Stage 3A behavior step begins until every
decision below has canonical prose, registry/schema evidence, and review.

## Read before editing

- [Charter](../../spec/00-charter.md): all of it; the decision order governs
  every tie below.
- [Source](../../spec/01-source-language.md): **Labels and applications**,
  **Declarations**, **External definitions**, **Types, interfaces, and
  methods**, **Functions and expressions**, **Type ascription and narrowing**.
- [Types](../../spec/02-type-system.md): all sections except **Iteration**,
  **Conversion**, and the interface-dispatch rules in **Interfaces and
  methods**, which Step 10 owns.
- [Projects](../../spec/04-programs-and-packages.md): **M2 bootstrap trust
  input**, **Tests**, **M2 assertion contract**.
- [Runtime](../../spec/06-runtime.md): **Evaluation**, **M2 compiler intrinsic
  profile**, **External providers**, **Traps**, **Determinism and
  observability**.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): **Diagnostics
  are a language surface**, **M2 unavailable-form spans**, **Conformance
  corpus**, **Conformance profiles**.
- M2's [decision ledger](../milestone-2/decision-ledger.md),
  [supported surface](../milestone-2/supported-surface.md), and
  [availability audit](../milestone-2/availability-audit.md).

## Decision checklist

The suggestions below are review starting points, not normative rules. Each row
is closed by a link to its reviewed canonical section, recorded in an M3
decision ledger (`decision-ledger.md` in this directory) with one disposition
and one proof owner per atomic row.

| ID | Close before | Required decision and evidence |
| --- | --- | --- |
| D1 | All 3A steps | Stage scope (gap G1). Decide whether `any`-bounded parametric generics move into Stage 3A. If they do, amend the Stage 3A and 3B lists in `v1.md` in this PR and state exactly which generic forms stay `@tool.unavailable` until Stage 3B (interface bounds, `defint`, `impl`, `any` and interfaces in type position). |
| D2 | 2–8 | M3 availability boundary: one M3 surface inventory giving every AST variant and every M2 ledger row deferred to M3 an owning step, plus the M4-owned rows unchanged. Add an inventory test that fails on an undisposed variant, modelled on `crates/vibra-conformance/tests/m2_contract_inventory.rs`. |
| D3 | 4, 8 | Map-key admissibility (gap G2): declare `equatable`, `ordered`, and `hashable` in the type chapter (their members may be written now and implemented in 3B), a closed builtin-conformance table for primitive key types and whether tuples of admissible keys qualify, and the canonical key order used by the runtime and later by `iter`. |
| D4 | 8 | Closed M3 library contract (gap G3): module map (`@std.option`, `@std.result`, `@std.integer`, `@std.text`, `@std.bytes`, `@std.array`, `@std.map`, or another reviewed split), every `@compiler` symbol with exact signature and total semantics, the checked-arithmetic error type(s) replacing the undeclared `overflow`, and the equality/comparison functions Stage 3A uses before interfaces exist. Extend the runtime chapter's intrinsic profile the same way M2 did for text. |
| D5 | 8 | Trust input for the extended library (gap G4): artifact path and package version for M3, whether the M2 artifact is retained or replaced, the review rule for re-signing, and who holds the toolchain private key. Needs a maintainer decision; an implementer cannot sign. |
| D6 | 2–7 | Diagnostic registry additions (gap G5) for Stage 3A: non-exhaustive `match`, unreachable arm, unhandled fallible value, `try` container/error mismatch, general ambiguous inference, infinite-size type, missing/duplicate/unknown constructor field, odd `map.of` arity, empty collection with no expected type, and invalid map key. Fix each code's level, primary span, and related spans. Reuse an existing code only when its condition matches. |
| D7 | 2–8 | Canonical observations (gap G6): VIBON rendering of record, enum, newtype, union, tuple, array, map, and atom values in typed and execution snapshots and in assertion `expected`/`actual`; whether the assertion registry gains monomorphic members for these values in 3A or waits for a generic `equatable` assertion in 3B. |
| D8 | 5 | Pattern details the chapters leave implicit: how atom patterns are "statically closed" for exhaustiveness; whether literal patterns cover every primitive; the witness a non-exhaustive diagnostic reports; how duplicate or unreachable arms relate to earlier arms. |
| D9 | 7 | `try` and unhandled values: the exact set of ignored positions (non-final `do` elements, discarded `let` bodies, test bodies), `try` inside lambdas, and `try` over `option` vs `result` in one function. |
| D10 | All | Editorial repairs (gap G8): split the two joined lines in **Model** and replace or declare the `storable` bound in the **Generics** example. Keep the `iter` table for Step 10. |

## Ordered work

1. Build a clause-to-decision table from the sections above, separating rules
   the specification already decides from missing contracts. Do not reopen
   settled rules because an implementation shortcut would be easier.
2. Close D1–D10 in their owning chapters. Following
   [`AGENTS.md`](../../../AGENTS.md), update every affected chapter, the
   diagnostic registry, the conformance rule table, and `v1.md` in this one PR.
3. Update the diagnostic registry crate and schema producer/consumer tests for
   the reviewed codes. Add the M3 inventory and its test.
4. Extend neutral conformance infrastructure only where the new observations
   require it, and test it with synthetic observations. Add no executable
   corpus case for a handler that does not exist yet.
5. Write the guides for Steps 2–9 using the
   [required guide contents](../execution.md#required-contents-of-an-implementation-guide),
   and an M3 `validation.md` that updates the M2 commands for `m3`.
6. Run the M2 [pre-merge checks](../milestone-2/validation.md#before-merging-each-step)
   and confirm the corpus still reports 205 passed, 0 failed, 0 unavailable.

## Done

Every atomic ledger row links to reviewed canonical prose and has registry or
schema evidence; the inventory test passes; the guides for Steps 2–9 exist; the
M2 corpus and host suites still pass. A row left open keeps this step
`in progress`. D5 in particular cannot be closed by an implementer alone.
