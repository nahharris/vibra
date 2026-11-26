# Step 34: fix-breadth

Kind: evidence. The fixes the review of step 33 found, each seen failing
first; the rulings on the rest.
Depends on: 33.
Crates: `vibra-diagnostics`, `vibra-workspace`, `vibra-cli`, `vibra-schema`,
`vibra-conformance`, `conformance/cases`, `docs/reference`.

## Prerequisites

- Read: `docs/roadmap/v0.0.2/m3/evidence/review-breadth.md` (written by
  step 33) and the integrator's rulings; the guides of the nine steps.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-interp/src/evaluate.rs:313` | `pub fn evaluate` | the evaluator |

The files owned below are those the findings can touch.

## Files owned

- `crates/vibra-diagnostics/src/registry.rs`
- `crates/vibra-diagnostics/src/diagnostic.rs`
- `crates/vibra-workspace/src/eval.rs`
- `crates/vibra-workspace/src/query/expansions.rs`
- `crates/vibra-workspace/src/query/index.rs`
- `crates/vibra-workspace/src/edit/rename.rs`
- `crates/vibra-workspace/src/edit/imports.rs`
- `crates/vibra-workspace/src/edit/fix.rs`
- `crates/vibra-workspace/src/edit/lint.rs`
- `crates/vibra-workspace/tests/it/eval.rs`
- `crates/vibra-workspace/tests/it/query_expansions.rs`
- `crates/vibra-workspace/tests/it/index.rs`
- `crates/vibra-workspace/tests/it/edit_rename.rs`
- `crates/vibra-workspace/tests/it/edit_imports.rs`
- `crates/vibra-workspace/tests/it/edit_fix.rs`
- `crates/vibra-workspace/tests/it/lint.rs`
- `crates/vibra-cli/src/eval.rs`
- `crates/vibra-cli/src/edit/rename.rs`
- `crates/vibra-cli/src/edit/imports.rs`
- `crates/vibra-cli/src/edit/fix.rs`
- `crates/vibra-cli/src/lint.rs`
- `crates/vibra-cli/tests/it/eval.rs`
- `crates/vibra-cli/tests/it/query_expansions.rs`
- `crates/vibra-cli/tests/it/query_index.rs`
- `crates/vibra-cli/tests/it/edit_rename.rs`
- `crates/vibra-cli/tests/it/edit_imports.rs`
- `crates/vibra-cli/tests/it/edit_fix.rs`
- `crates/vibra-cli/tests/it/lint.rs`
- `crates/vibra-schema/src/eval_payload.rs`
- `crates/vibra-schema/src/index.rs`
- `crates/vibra-schema/src/lint_payload.rs`
- `crates/vibra-schema/schemas/v2/payloads/eval.json`
- `crates/vibra-schema/schemas/v2/payloads/lint.json`
- `crates/vibra-schema/schemas/v2/index.json`
- `crates/vibra-schema/schemas/v2/query/expansions.json`
- `crates/vibra-schema/schemas/v2/query/index.json`
- `crates/vibra-conformance/src/eval.rs`
- `crates/vibra-conformance/src/bin/reference/main.rs`
- `crates/vibra-conformance/src/bin/reference/data.rs`
- `crates/vibra-conformance/src/bin/reference/card.rs`
- `crates/vibra-conformance/src/bin/reference/examples.rs`
- `crates/vibra-conformance/src/bin/reference/card-template.md`
- `crates/vibra-conformance/tests/it/reference.rs`
- `crates/vibra-conformance/tests/it/reference_card.rs`
- `docs/reference/`
- `docs/roadmap/v0.0.2/m3/evidence/fix-breadth.md`
- `conformance/rules/m3-34.tsv`
- the case directories of the fixes

## Ordered tasks

1. For each finding ruled a defect: a test or case written from the
   chapter that fails on the reviewed commit, then the least fix.
2. For each finding ruled a limit or accepted: a sentence in the evidence
   file.
3. A finding whose fix needs a file outside this list stops with the file
   and owner named.

## Test matrix

Written by the step from the findings; ids `V2-TOOL-fix-eval-*`,
`V2-TOOL-fix-rename-*`, `V2-TOOL-fix-index-*`, `ATK-fix-breadth-*`.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| each defect | negative | the finding's probe | the chapter's behaviour; the test fails on the reviewed commit | the finding's sentence |
| no regression | positive | the suites of the nine steps | unchanged | execution model |

## Diagnostics and schema changes

None unless a finding names one.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-diagnostics -p vibra-syntax -p vibra-fmt -p vibra-resolve -p vibra-ir -p vibra-interp -p vibra-types -p vibra-host -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo run --locked --offline -p vibra-conformance --bin vibra-reference -- --out docs/reference
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; `git status --short docs/reference` prints nothing.

## Stays unavailable

Unchanged.

## Excluded

Findings against identity, the focused edit lane, or the focused queries.

## Completion evidence

- [ ] `evidence/fix-breadth.md` maps every finding.
- [ ] Each fix's test was seen failing on the reviewed commit.
- [ ] The validation commands exit 0.
