# M2 step 12 — fixes of the review of rows

The fixes of the findings of the [review of rows](review-rows.md), on the
step branch `m2/12-fix-rows`. Each case or test was committed before its
fix, in `18e37ef1` ("failing cases for the review of rows"), and seen to
fail there; the fix is in `3c7e6cb4` ("fix the witness chain, the
fallback and unused roots"), where it passes. A finding that needed no
code has a case that passes in both.

## Findings

| Finding | Class | Case or test | Fails at | Passes at |
| --- | --- | --- | --- | --- |
| F1, a witness cycle drops every related span (probe `cycleNotes`) | defect, fixed in `check/effects/analysis.rs` | `V2-EFFECT-outside-ceiling-cycle`; `ceilings::a_witness_cycle_keeps_the_chain_up_to_the_repeated_hop` | `18e37ef1`: 0 related spans | `3c7e6cb4` |
| F1, the same rule for an awaited operand with an error of its own: the chain was empty | defect, fixed in `check/effects/procs.rs` and `analysis.rs` | `ceilings::an_awaited_operand_with_an_error_starts_the_chain` | a probe on `18e37ef1`: one `@effect.outsideCeiling` with no related span | `3c7e6cb4` |
| F2, a member at a closed receiver is not indirect (probe `closedNotes`) | ruling, no code | `ceilings::a_member_at_a_closed_receiver_is_not_indirect` | passes at `18e37ef1` | `3c7e6cb4` |
| F3, a value-ending fallback gets two errors at one span (probe `fallback`) | defect, fixed in `check/expr.rs` | `V2-TYPE-CONTROL-let-else-value-statement`; `values::the_last_statement_of_a_let_fallback_is_a_statement` | `18e37ef1`: 2 diagnostics | `3c7e6cb4` |
| Unused-effect ruling of step 10 (probe `anonWarning`) | ruling, already held | `ceilings::an_anonymous_proc_with_an_error_hides_the_unused_roots_of_its_declaration`, first source | passes at `18e37ef1` | `3c7e6cb4` |
| Its variant: the anonymous `proc`'s only error is `@effect.outsideCeiling` | defect, fixed in `check/effects/analysis.rs` | the same test, second source | `18e37ef1`: `@contract.unusedEffect` reported | `3c7e6cb4` |
| F4, a plan as a module value (probe `constPlan`) | guide wrong; row of guide 11 corrected | `V2-EFFECT-module-value-plan` | passes at `18e37ef1` | `3c7e6cb4` |
| F4, a plan where a constant is required (probe `constDefault`) | the control | `V2-EFFECT-plan-default-not-constant` | passes at `18e37ef1` | `3c7e6cb4` |
| F5, the checked IR reports the first root outside a ceiling | accepted limit | none | — | — |
| F6, host operations have no source-level example | accepted limit | none | — | — |
| F7, the checker's rows against the checked IR's | closed | `ceilings::checker_rows_equal_the_checked_ir_rows_on_every_accepted_effect_case`; its list guarded by `vibra-conformance` `effect_rows::the_row_comparison_embeds_every_accepted_effect_case` | new; seen to fail under two mutations of the checker's rows | the last commit of the step |

No finding is a question: F4 was a question about the guide, and its
ruling is that the guide was wrong.

## The rules the fixes follow

- **F1.** The chain is searched breadth first as before. It ends at the
  operation member of the root when it reaches one, else at the first call
  it cannot follow. When the search meets only cycles, it ends at the first
  call of a `proc` it already followed, that call included. In
  `cycleNotes`, the chain is `loop()` in `run`, then `loop()` in `loop`,
  each `performs Log via loop`.
- **F1, a faulty operand.** An awaited operand that has an error of its own
  has no lowered node, so the call it runs is not known. It is one indirect
  hop at the operand, named by its source text, as a plan value is, and
  the chain ends there.
- **F3.** The fallback that can run off its end is not reported again at
  a span where a `@type.mismatch` was already reported: a fallback of one
  statement whose value is not `Void` or `Never` is one fault, at that
  statement. A fallback of two statements whose last has a value is two
  faults at two spans, the statement and the fallback.
- **Unused roots.** A body's unused roots are not reported when its
  declaration has an error other than that body's own
  `@effect.outsideCeiling`. An `@effect.outsideCeiling` of an anonymous
  `proc` joins the declaration that holds it, so it hides the unused roots
  of the declaration's own body and of its other anonymous `proc`s.

## Checker rows against the checked IR rows

The host test checks every case of `conformance/cases/EFFECT` whose
expected verdict is `accepted`, 37 cases, as the workspace check does. A
semantic crate reads no file, so the test embeds each case's modules with
`include_str!`, and the test `effect_rows` of `vibra-conformance` keeps
that list equal to the corpus, case by case and module by module; it was
seen to fail with one case dropped from the list. For each body the
checker analyses, the test keeps the
roots `Performed::of` computes. For each function and module value of the
checked module set and test set, it takes the union of `Facts::awaited`,
and the same for each `Proc` closure the declaration creates, directly or
through other closures. It requires, in both directions:

- the checker's row of a declaration's own body equals the IR's row of the
  function of the same name;
- an IR function the checker has no row for awaits nothing;
- each anonymous `proc` of a declaration matches a distinct `Proc` closure
  of it with the same roots;
- a `Proc` closure that matches no anonymous `proc`, which a contract
  `proc` member named as a value becomes, awaits exactly its type's row.

The test was seen to fail when the checker's rows were emptied
(`V2-EFFECT-callback-row-exact`, `app.main.emit`) and when the anonymous
`proc`s were not recorded (`V2-EFFECT-callback-row-wrapped`, `app.main.run`).
The checker's rows are read through a test-only record in
`check/effects/analysis.rs`, `observed`.

## The corpus

The corpus command exits 0: 982 passed, 0 failed, 0 unknown and 0
unavailable.
