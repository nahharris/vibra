# Step 12: fix-rows

Kind: evidence. It closes the defects gate 11 found in rows, plans, and
ceilings, each with the case that would have caught it.
Depends on: 11.
Crates: `vibra-ir`, `vibra-interp`, `vibra-types`, `conformance/cases`.

## Prerequisites

- The review of step 11, `evidence/review-rows.md`, with every finding
  classified.
- Read: the chapter sentence each defect cites.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-ir/src/module_set.rs:482` | `pub fn try_new` | IR-side fixes |
| `crates/vibra-types/src/check/lower.rs:868` | `pub(crate) fn check_program` | checker-side fixes enter through `effects::` |
| `crates/vibra-interp/src/machine.rs:1159` | `pub(crate) fn invoke` | interpreter-side fixes |

## Files owned

- `crates/vibra-ir/src/effects.rs`
- `crates/vibra-ir/tests/it/effects.rs`
- `crates/vibra-interp/src/plan.rs`
- `crates/vibra-interp/tests/it/plans.rs`
- `crates/vibra-types/src/check/effects/rows.rs`
- `crates/vibra-types/src/check/effects/procs.rs`
- `crates/vibra-types/src/check/effects/members.rs`
- `crates/vibra-types/src/check/effects/analysis.rs`
- `crates/vibra-types/src/check/effects/values.rs`
- `conformance/rules/m2-12.tsv`
- `docs/roadmap/v0.0.2/m2/evidence/fix-rows.md`

## Ordered tasks

1. For each defect, first add the failing case or test, with the id the
   review proposed, and record that it fails. Invariant: no fix lands
   without a case that failed before it.
2. Fix the defect in the file that owns the rule. A defect whose fix
   needs a file outside the list above is returned to the planner, not
   widened here.
3. Rerun the review's comparison of checker rows against IR rows.
4. A finding classed as a question is not fixed: its case stays out of the
   corpus until the question is answered.

## Test matrix

One row per defect of the review, added to this table when the review is
merged; each row follows the rule of the guides: one fault, real source,
its chapter sentence. The fixed rows are:

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every failed probe of step 11 | negative or positive, as the probe | the probe's source, as a corpus case or host test | the probe's expected result | the probe's sentence |
| checker rows equal IR rows | positive | every accepted `EFFECT` case | equal | architecture, risk 1 |
| no regression | positive | the corpus | passes | each case's anchor |

## Diagnostics and schema changes

None beyond what the fixed rules already name; a fix that needs a new
code is a question, not a fix.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-ir -p vibra-interp -p vibra-types -p vibra-workspace -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

Unchanged.

## Excluded

New behaviour; findings classed as questions or accepted limits.

## Completion evidence

- [ ] `evidence/fix-rows.md` maps every defect to its case, the commit
      where the case failed, and the commit where it passes.
- [ ] No defect of the review is open; each question is in the
      [questions](../spec-questions.md).
- [ ] The validation commands exit 0.
