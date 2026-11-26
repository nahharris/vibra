# Step 28: fix-edits

Kind: evidence. The fixes the review of step 27 found, each seen failing
first, with its host test or case; the rulings on the rest.
Depends on: 27.
Crates: `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`,
`conformance/cases`.

## Prerequisites

- Read: `docs/roadmap/v0.0.2/m3/evidence/review-edits.md` (written by
  step 27) and the integrator's rulings on it; the guides of steps 07, 11,
  14, 19, 20, 21, 24.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fs-scope/src/lib.rs:248` | `pub fn replace_contents` | the write primitive |

The files owned below are those the findings can touch.

## Files owned

- `crates/vibra-workspace/src/edit/plan.rs`
- `crates/vibra-workspace/src/edit/replace.rs`
- `crates/vibra-workspace/src/edit/apply.rs`
- `crates/vibra-workspace/src/edit/journal.rs`
- `crates/vibra-workspace/src/edit/rename.rs`
- `crates/vibra-workspace/src/edit/imports.rs`
- `crates/vibra-workspace/src/edit/fix.rs`
- `crates/vibra-workspace/src/edit/lint.rs`
- `crates/vibra-workspace/tests/it/edit_plan.rs`
- `crates/vibra-workspace/tests/it/edit_replace.rs`
- `crates/vibra-workspace/tests/it/edit_apply.rs`
- `crates/vibra-workspace/tests/it/edit_rename.rs`
- `crates/vibra-workspace/tests/it/edit_imports.rs`
- `crates/vibra-workspace/tests/it/edit_fix.rs`
- `crates/vibra-workspace/tests/it/lint.rs`
- `crates/vibra-cli/src/edit.rs`
- `crates/vibra-cli/src/lint.rs`
- `crates/vibra-cli/tests/it/edit_replace.rs`
- `crates/vibra-cli/tests/it/edit_apply.rs`
- `crates/vibra-cli/tests/it/edit_rename.rs`
- `crates/vibra-cli/tests/it/edit_imports.rs`
- `crates/vibra-cli/tests/it/edit_fix.rs`
- `crates/vibra-cli/tests/it/lint.rs`
- `crates/vibra-schema/src/edit_plan.rs`
- `crates/vibra-schema/schemas/v2/edit-plan.json`
- `crates/vibra-schema/src/lint_payload.rs`
- `crates/vibra-conformance/src/edit.rs`
- `crates/vibra-conformance/tests/it/engine_writes.rs`
- `docs/roadmap/v0.0.2/m3/evidence/fix-edits.md`
- `conformance/rules/m3-28.tsv`
- the case directories of the fixes

## Ordered tasks

1. For each finding ruled a defect: a host test or case written from the
   chapter that fails on the reviewed commit, then the least fix.
2. For each finding ruled a limit or accepted: a sentence in
   `evidence/fix-edits.md`.
3. A finding whose fix needs a file outside this list stops with the file
   and owner named.

## Test matrix

Written by the step from the findings; ids `V2-TOOL-fix-edit-*` and
`ATK-fix-edit-*`.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| each defect | negative | the finding's probe | the chapter's behaviour; the test fails on the reviewed commit | the finding's sentence |
| no regression | positive | the suites of the seven steps | unchanged | execution model |

## Diagnostics and schema changes

None unless a finding names one.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features -- --nocapture
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

Unchanged.

## Excluded

Findings against identity, queries, or evaluation.

## Completion evidence

- [ ] `evidence/fix-edits.md` maps every finding to a fix, a ruling, or a
      stop.
- [ ] Each fix's test was seen failing on the reviewed commit.
- [ ] The validation commands exit 0.
