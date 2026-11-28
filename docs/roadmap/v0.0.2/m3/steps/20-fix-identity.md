# Step 20: fix-identity

Kind: evidence. The fixes the review of step 19 found, each seen failing
first, with its case or host test; the rulings on the findings that are
not defects.
Depends on: 19.
Crates: `vibra-fmt`, `vibra-workspace`, `vibra-cli`, `conformance/cases`.

## Prerequisites

- Read: `docs/roadmap/v0.0.2/m3/evidence/review-identity.md` (written by
  step 19) and the integrator's rulings on it; the guides of steps 03, 04,
  09, 10, 14, 16.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fmt/src/lib.rs:42` | `pub fn format_document` | the text source |
| `crates/vibra-resolve/src/identity.rs:131` | `pub fn identity` | the anchor |

The files owned below are those the findings can touch.

## Files owned

- `crates/vibra-fmt/src/fragment.rs`
- `crates/vibra-fmt/tests/it/fragment.rs`
- `crates/vibra-workspace/src/identity/mod.rs`
- `crates/vibra-workspace/src/identity/claim.rs`
- `crates/vibra-workspace/src/identity/witness.rs`
- `crates/vibra-workspace/src/approval.rs`
- `crates/vibra-workspace/src/approve_write.rs`
- `crates/vibra-workspace/src/present.rs`
- `crates/vibra-workspace/tests/it/claim.rs`
- `crates/vibra-workspace/tests/it/witness.rs`
- `crates/vibra-workspace/tests/it/approval.rs`
- `crates/vibra-workspace/tests/it/approve_write.rs`
- `crates/vibra-workspace/tests/it/present.rs`
- `crates/vibra-cli/src/approve/mod.rs`
- `crates/vibra-cli/src/approve/write.rs`
- `crates/vibra-cli/tests/it/approve.rs`
- `crates/vibra-cli/tests/it/approve_write.rs`
- `crates/vibra-cli/tests/it/approval.rs`
- `docs/roadmap/v0.0.2/m3/evidence/fix-identity.md`
- `conformance/rules/m3-20.tsv`
- the case directories of the fixes

## Ordered tasks

1. For each finding ruled a defect: a case or host test written from the
   chapter that fails on the reviewed commit, then the least fix in the
   files owned.
2. For each finding ruled a limit or accepted: a sentence in
   `evidence/fix-identity.md` and, where the chapter must say so, a line
   in the report for the next spec batch.
3. A finding whose fix needs a file outside this list stops: the report
   names the file and the owner.

## Test matrix

Written by the step from the findings; each row cites the sentence of the
chapter the finding contradicted, as the M2 fix steps did. The ids are
`V2-TOOL-fix-identity-*` and `V2-GRANT-fix-approval-*`.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| each defect | negative | the finding's probe | the chapter's behaviour; the test fails on the reviewed commit | the finding's sentence |
| no regression | positive | the suites of steps 03, 04, 09, 10, 14, 16 | unchanged results | execution model |

## Diagnostics and schema changes

None unless a finding names one; then the report says which.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-fmt -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

Unchanged.

## Excluded

Findings against the edit lane, queries, or evaluation: those go to gates
21, 23, and 33.

## Completion evidence

- [ ] `evidence/fix-identity.md` maps every finding to a fix, a ruling, or
      a stop.
- [ ] Each fix's test was seen failing on the reviewed commit; the report
      says how.
- [ ] The validation commands exit 0.
