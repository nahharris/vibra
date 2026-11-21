# Step 35: fix-drivers

Kind: evidence. It closes the defects gate 34 found in startup, the run,
the command, and the real drivers, each with the test that would have
caught it.
Depends on: 34.
Crates: `vibra-fs-scope`, `vibra-host`, `vibra-workspace`, `vibra-cli`.

## Prerequisites

- The review of step 34, `evidence/review-drivers.md`, every finding
  classified.
- Read: the chapter sentence each defect cites.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fs-scope/src/windows.rs:55` | `pub struct ScopedDir` | adapter-side fixes |
| `crates/vibra-cli/src/lib.rs:47` | `pub const fn exit_code` | command-side fixes |

Driver-side fixes enter through `Driver::call` and `Driver::acquire`.

## Files owned

- `crates/vibra-fs-scope/src/`
- `crates/vibra-fs-scope/tests/`
- `crates/vibra-host/src/startup.rs`
- `crates/vibra-host/src/system.rs`
- `crates/vibra-host/src/system/`
- `crates/vibra-host/tests/it/startup.rs`
- `crates/vibra-host/tests/it/system.rs`
- `crates/vibra-host/tests/it/fs.rs`
- `crates/vibra-workspace/src/run.rs`
- `crates/vibra-workspace/src/entry.rs`
- `crates/vibra-workspace/src/binding.rs`
- `crates/vibra-workspace/tests/it/run.rs`
- `crates/vibra-workspace/tests/it/attacks_run.rs`
- `crates/vibra-cli/src/run.rs`
- `crates/vibra-cli/tests/it/run_command.rs`
- `conformance/rules/m2-35.tsv`
- `docs/roadmap/v0.0.2/m2/evidence/fix-drivers.md`

## Ordered tasks

1. For each defect, first add the failing test or case and record that it
   fails. Invariant: no fix without a test that failed before it.
2. Fix the defect in the file that owns the rule. A fix that needs a file
   outside the list is returned to the planner.
3. An accepted limit is written into the evidence in the chapter's words
   and into nothing else.
4. Rerun every probe of step 34 and the dual run of step 33.

## Test matrix

One row per defect of the review, added when the review is merged. The
fixed rows are:

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every failed probe of step 34 | as the probe | the probe's source, as a host test | the probe's expected result | the probe's sentence |
| the dual run | positive | every row of step 33 | agreement, or `NOT RUN` with a reason | architecture, section 3 |
| no regression | positive | the corpus and the attack suites of steps 13, 31, 33 | pass | each row's sentence |

## Diagnostics and schema changes

None beyond what the fixed rules name.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-fs-scope -p vibra-host -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features -- --nocapture
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; `NOT RUN` lines are counted and copied.

## Stays unavailable

Unchanged.

## Excluded

New behaviour; findings classed as questions or accepted limits.

## Completion evidence

- [ ] `evidence/fix-drivers.md` maps every defect to its test and the two
      commits, failing and passing.
- [ ] No defect of the review is open.
- [ ] The validation commands exit 0.
