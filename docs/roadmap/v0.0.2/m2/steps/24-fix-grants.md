# Step 24: fix-grants

Kind: evidence. It closes the defects gate 23 found in the broker,
policies, the fake world, the trace, and grant values, each with the case
that would have caught it.
Depends on: 23.
Crates: `vibra-host`, `vibra-interp`, `vibra-types`, `stdlib`,
`conformance/cases`.

## Prerequisites

- The review of step 23, `evidence/review-grants.md`, every finding
  classified.
- Read: the chapter sentence each defect cites.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/test_runner.rs:611` | `fn run_one` | runner-side fixes |
| `crates/vibra-interp/src/machine.rs:1264` | `fn dispatch` | interpreter-side fixes |

Broker-side fixes enter through `Context::host` and `Context::narrow` of
step 15.

## Files owned

- `crates/vibra-host/src/policy.rs`
- `crates/vibra-host/src/path.rs`
- `crates/vibra-host/src/broker.rs`
- `crates/vibra-host/src/driver.rs`
- `crates/vibra-host/src/world.rs`
- `crates/vibra-host/src/trace.rs`
- `crates/vibra-host/src/test_world.rs`
- `crates/vibra-host/tests/it/broker.rs`
- `crates/vibra-host/tests/it/world.rs`
- `crates/vibra-host/tests/it/trace.rs`
- `crates/vibra-interp/src/host.rs`
- `crates/vibra-interp/tests/it/host.rs`
- `crates/vibra-types/src/check/effects/grants.rs`
- `crates/vibra-workspace/src/test_runner.rs`
- `conformance/rules/m2-24.tsv`
- `docs/roadmap/v0.0.2/m2/evidence/fix-grants.md`

## Ordered tasks

1. For each defect, first add the failing case or host test and record
   that it fails. Invariant: no fix without a case that failed before it.
2. Fix the defect in the file that owns the rule. A fix that needs a file
   outside the list is returned to the planner.
3. A fix that changes a trace changes the hand-written snapshots only
   after the chapter sentence that fixes the new text is quoted in the
   report.
4. Rerun every probe of step 23.

## Test matrix

One row per defect of the review, added when the review is merged. The
fixed rows are:

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every failed probe of step 23 | as the probe | the probe's source, as a case or host test | the probe's expected result | the probe's sentence |
| the counter model | positive | the random-tree test of step 15, 10,000 trees | equal | 06 Accounting |
| no regression | positive | the corpus | passes | each case's anchor |

## Diagnostics and schema changes

None beyond what the fixed rules name.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-host -p vibra-interp -p vibra-types -p vibra-workspace -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
& conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. A fix that changes the library first recomputes the
expected revisions with `revisions.ps1 -Write` (Q33).

## Stays unavailable

Unchanged.

## Excluded

New behaviour; findings classed as questions or accepted limits.

## Completion evidence

- [ ] `evidence/fix-grants.md` maps every defect to its case and the two
      commits, failing and passing.
- [ ] No defect of the review is open.
- [ ] The validation commands exit 0.
