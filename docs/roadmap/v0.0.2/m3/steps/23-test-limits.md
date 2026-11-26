# Step 23: test-limits

Kind: vertical. A work limit for every test item, reported as the chapter
reports an exhausted budget, with the other items keeping their results;
the structured test result checked field by field against the chapter on
the release program's tests.
Depends on: 01. Depends on fix package E for the spelling convention of
limit names (Q27); until it lands the step uses `testReductions` and the
report says so.
Crates: `vibra-workspace`, `vibra-cli`, `vibra-schema`, `conformance/cases`.

## Prerequisites

- Read: [05 Tests and selection](../../../../spec/05-tooling.md#tests-and-selection);
  [05 Command payloads](../../../../spec/05-tooling.md#command-payloads) (`TestItem`);
  [06 Activations and memory](../../../../spec/06-runtime.md#activations-and-memory);
  [07 Persistent observations](../../../../spec/07-diagnostics-and-conformance.md#persistent-observations)
  (`@testRun.v2`); [questions](../spec-questions.md) Q27.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/test_runner.rs:454` | `pub fn run_tests_in` | gains a reduction budget beside the memory budget |
| `crates/vibra-workspace/src/test_runner.rs:148` | `pub enum TestOutcome` | the exhausted outcome widens |
| `crates/vibra-interp/src/lib.rs:71` | `pub struct Limits` | the interpreter's work limit |
| `crates/vibra-interp/src/evaluate.rs:243` | `pub fn run_test` | the entry the runner calls; its limits |
| `crates/vibra-cli/src/test.rs:47` | `pub const MEMORY_BUDGET` | the budgets of the command |
| `crates/vibra-schema/src/test_payload.rs:130` | `pub struct TestItem` | the structured result |

Proposed: `test_runner::WORK_BUDGET`, `TestOutcome::Exhausted` carrying
which budget.

## Files owned

- `crates/vibra-workspace/src/test_runner.rs`
- `crates/vibra-workspace/src/test_runner/tests.rs`
- `crates/vibra-workspace/tests/it/limits.rs`
- `crates/vibra-workspace/tests/it/test_runner.rs`
- `crates/vibra-cli/src/test.rs`
- `crates/vibra-cli/src/test/tests.rs`
- `crates/vibra-cli/tests/it/limits.rs`
- `crates/vibra-cli/tests/it/test_command.rs`
- `crates/vibra-schema/src/test_payload.rs`
- `crates/vibra-schema/schemas/v2/payloads/test.json`
- `crates/vibra-schema/tests/it/test_payload.rs`
- `conformance/rules/m3-23.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `run_tests_in` takes a `LimitProfile`-derived reduction budget per
   item, 100,000,000 under `m1Default`; `run_test` is called with it (the
   interpreter already takes `Limits`; if `run_test` does not, the report
   stops at this task and names the `vibra-interp` change needed, which
   this step may not make).
2. An item that reaches it is `@test.exhausted` with the diagnostic
   `@tool.limitExceeded` (no location) attributed to the item; the suite's
   precedence is unchanged; `limits.reached` names `testReductions`;
   `complete: false`; the other items run and keep their results.
3. `TestItem` and `TestItem.failure` compared field by field with the
   chapter's table on the release program's tests: `name`, `result`,
   `failure` with `kind`, `assertion`, `expected`, `actual`,
   `primarySpan`, `observationError`; `trap`; `auditTrace`;
   `diagnostics`; `status`. Any field the chapter has and the payload
   lacks is added here; any the payload has and the chapter lacks is a
   finding.

## Test matrix

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-PROJECT-test-reductions-limit` | negative, `workspaceTest` | a test whose body awaits a tail-recursive `proc` without end | the item `@test.exhausted`; the suite `@command.operationalFailure`; the observation records the limit as unknown with the profile | 07: "A tool limit is expected as unknown, with the limit profile" |
| `V2-PROJECT-test-reductions-other-items-run` | positive, `workspaceTest` | the same suite with a passing test after it | the second item `@test.passed` | 06: "The run MUST NOT report the tests that did not run as passed"; 05: "the other items keep their results" |
| `V2-PROJECT-test-reductions-within` | positive, `workspaceTest` | a test that runs 1,000,000 reductions | `@test.passed` | Q27 |
| JSON: exhausted item | positive | the first row through `vibra --format json test` | `tests[0].result: "@test.exhausted"`, `diagnostics: ["@tool.limitExceeded"]`, `status: "unknown"`; `limits.reached: ["testReductions"]`, `complete: false` | 05 `TestItem`: "`status`, that of the command result the item's result corresponds to" |
| structured failure | positive | the release program's failing-test fixture (`gate.rs`) | `failure {kind: "assertion", assertion, expected, actual, primarySpan, observationError: null}` | 05 `TestItem.failure` |
| observation failure | positive | `assert.equal` over an `Any` holding a function | `failure.kind: "observation"`, `observationError.code: "@tool.unsupportedValue"` | 05 `ObservationError` |
| trace per item | positive | a test with two host calls | `auditTrace` with two `@host` events of phase `@entry` in completion order | 05 `TestItem`: "the test's events in completion order" |
| selector run | positive | `test @tests.main::"…"` of the gate program | one item | 05 Tests and selection |
| memory exhaustion unchanged | positive | `V2-RUNTIME-memory-exhausted`'s shape under `test` | `@test.exhausted`, `limits.reached: ["memory"]` | 06 Activations and memory |
| both limits named | positive | a suite with one item exhausting memory and one exhausting work | `limits.reached: ["memory", "testReductions"]` in order of reaching | 05: "`reached` lists the names of the limits reached, in order" |

## Diagnostics and schema changes

Codes: `@tool.limitExceeded`, `@runtime.memoryExhausted`. Schema: the
`test` payload, if a field of the chapter's table was missing. The limit
name `testReductions` and the widened meaning of `@test.exhausted` are
Q27's, to be written into 05 by the spec batch that lands the ruling.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; every `V2-PROJECT-test-*` and `V2-GRANT-test-*` case of
M2 passes unchanged.

## Stays unavailable

A per-test time limit (Q29).

## Excluded

The interpreter's counting (exists); eval's limits (10).

## Completion evidence

- [ ] `conformance/rules/m3-23.tsv` has one `active` row per case id.
- [ ] The report lists each field of 05's `TestItem` table beside the
      payload's field.
- [ ] The validation commands exit 0.
