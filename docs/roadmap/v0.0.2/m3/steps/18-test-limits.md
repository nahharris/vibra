# Step 18: test-limits

Kind: vertical. A work limit for every test item, passed to the
interpreter's test entry, which gains it here; an operation of the test
world charged deterministically; the exhausted item reported as the chapter
reports an exhausted budget, with the other items keeping their results
and the suite following the chapter's precedence; the structured test
result checked field by field against the chapter on the release program's
tests; the `approvals` field of the test payload rendered.
Depends on: 01; fix package E (`m2/fix-e`) merged, for the spelling
convention of limit names (Q27).
Crates: `vibra-interp`, `vibra-workspace`, `vibra-cli`, `vibra-schema`,
`conformance/cases`.

## Prerequisites

- Read: [05 Tests and selection](../../../../spec/05-tooling.md#tests-and-selection)
  (the item results and the suite precedence); [05 Command payloads](../../../../spec/05-tooling.md#command-payloads) (`TestItem`);
  [05 Program arguments, execution and evaluation](../../../../spec/05-tooling.md#program-arguments-execution-and-evaluation) ("Limits");
  [06 Activations and memory](../../../../spec/06-runtime.md#activations-and-memory);
  [07 Persistent observations](../../../../spec/07-diagnostics-and-conformance.md#persistent-observations)
  (`@testRun.v2`); [architecture](../architecture.md) row I16;
  [questions](../spec-questions.md) Q27, Q29, Q40.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-interp/src/evaluate.rs:243` | `pub fn run_test` | takes a memory budget alone; gains `Limits` |
| `crates/vibra-interp/src/evaluate.rs:284` | `let mut machine = Machine::new(set, Mode::Test, budget.bytes, u64::MAX);` | the unbounded reduction count the step replaces |
| `crates/vibra-interp/src/lib.rs:71` | `pub struct Limits` | the interpreter's work limit |
| `crates/vibra-workspace/src/test_runner.rs:454` | `pub fn run_tests_in` | gains a reduction budget beside the memory budget |
| `crates/vibra-workspace/src/test_runner.rs:148` | `pub enum TestOutcome` | the exhausted outcome widens |
| `crates/vibra-cli/src/test.rs:47` | `pub const MEMORY_BUDGET` | the budgets of the command |
| `crates/vibra-schema/src/test_payload.rs:130` | `pub struct TestItem` | the structured result |

Proposed: `test_runner::WORK_BUDGET`, `TestOutcome::Exhausted` carrying
which budget; `run_test(program, world, host, limits)`.

## Files owned

- `crates/vibra-interp/src/evaluate.rs`
- `crates/vibra-interp/tests/it/test_limits.rs`
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
- `conformance/rules/m3-18.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `vibra-interp`: `run_test` and `run_test_probing` take `Limits` (the
   reduction limit and the memory budget) and build the test machine with
   that reduction count in place of the unbounded one; a reached reduction
   limit ends the body as `Evaluation::LimitReached(Limit::Reductions)`
   does for an evaluation, reported through the existing `RuntimeError`
   for an exhausted budget. The hosts of the test world charge a stated
   amount per operation: 1,024 reductions for the call and one per byte of
   the data it moves (Q27), through the machine's counter and no clock.
   This is the one change M3 makes in `vibra-interp`; `evaluate.rs` is the
   one file it touches there, and fix E's walk of sequences is in place
   before it. Invariant: every other test of `vibra-interp` passes
   unchanged with `Limits { reductions: u64::MAX, .. }`.
2. `run_tests_in` takes a `LimitProfile`-derived reduction budget per
   item, 100,000,000 under `m1Default`, and passes it with the memory
   budget.
3. An item that reaches it is `@test.exhausted` with the diagnostic
   `@tool.limitExceeded` (no location) attributed to the item; the suite's
   precedence is unchanged (trap, operational exhaustion, unavailable, test
   failed, ok); `limits.reached` names `testReductions`; `complete: false`;
   the other items run and keep their results; the persisted `@testRun.v2`
   records the item.
4. `TestItem` and `TestItem.failure` compared field by field with the
   chapter's table on the release program's tests: `name`, `result`,
   `failure` with `kind`, `assertion`, `expected`, `actual`,
   `primarySpan`, `observationError`; `trap`; `auditTrace`;
   `diagnostics`; `status`. Any field the chapter has and the payload
   lacks is added here; any the payload has and the chapter lacks is a
   finding. The payload's `approvals` list (Q40) is rendered from the
   `TestRun`'s list the hook of step 01 fills; its exhausted form keeps
   the three counts and empties `tests` and `approvals`.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| a test item ends within its work limit | a long pure loop; a long run of test-world operations; against: a test within the limit |
| the suite's precedence | an exhausted item alone; an exhausted item after a failed one; against: all passed |

## Test matrix

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-PROJECT-test-reductions-limit` | negative, `workspaceTest` | a test whose body awaits a tail-recursive `proc` without end | the item `@test.exhausted`; the suite `@command.operationalFailure`; the observation records the limit as unknown with the profile | 07: "A tool limit is expected as unknown, with the limit profile" |
| `V2-PROJECT-test-reductions-other-items-run` | positive, `workspaceTest` | the same suite with a passing test after it | the second item `@test.passed` | 06: "The run MUST NOT report the tests that did not run as passed"; 05: "the other items keep their results" |
| `V2-PROJECT-test-exhausted-after-failed` | negative, `workspaceTest` | a suite whose first item fails an assertion and whose second exhausts its work | the first `@test.assertionFailed`, the second `@test.exhausted`; the suite `@command.operationalFailure` | 05: "the suite result follows this precedence: trap, operational exhaustion, unavailable, test failed, ok" |
| `V2-PROJECT-test-world-operations-charged` | negative, `workspaceTest` | a test that writes 200,000 small files through `TestWorld.directory`'s `writeText` grant in a loop | the item `@test.exhausted` with `testReductions`; the host test shows the count charged per operation and per byte | 05: "Every executing command runs under published, versioned limits on work"; Q27 |
| `V2-PROJECT-test-reductions-within` | positive, `workspaceTest` | a test that runs 1,000,000 reductions | `@test.passed` | Q27 |
| JSON: exhausted item | positive | the first row through `vibra --format json test` | `tests[0].result: "@test.exhausted"`, `diagnostics: ["@tool.limitExceeded"]`, `status: "unknown"`; `limits.reached: ["testReductions"]`, `complete: false` | 05 `TestItem`: "`status`, that of the command result the item's result corresponds to" |
| structured failure | positive | the release program's failing-test fixture (`gate.rs`) | `failure {kind: "assertion", assertion, expected, actual, primarySpan, observationError: null}` | 05 `TestItem.failure` |
| observation failure | positive | `assert.equal` over an `Any` holding a function | `failure.kind: "observation"`, `observationError.code: "@tool.unsupportedValue"` | 05 `ObservationError` |
| trace per item | positive | a test with two host calls | `auditTrace` with two `@host` events of phase `@entry` in completion order | 05 `TestItem`: "the test's events in completion order" |
| selector run | positive | `test @tests.main::"…"` of the gate program | one item | 05 Tests and selection |
| approvals rendered | positive | `test` on a tree whose `TestRun` list the hook filled (a stub list in this step's host test) | the payload's `approvals` holds the list's entries with `target`, `identity`, `selection` | Q40 |
| memory exhaustion unchanged | positive | `V2-RUNTIME-memory-exhausted`'s shape under `test` | `@test.exhausted`, `limits.reached: ["memory"]` | 06 Activations and memory |
| both limits named | positive | a suite with one item exhausting memory and one exhausting work | `limits.reached: ["memory", "testReductions"]` in order of reaching | 05: "`reached` lists the names of the limits reached, in order" |
| interpreter unchanged otherwise | positive | the suites of `vibra-interp` with an unbounded `Limits` | pass unchanged | architecture, section 1 |

## Diagnostics and schema changes

Codes: `@tool.limitExceeded`, `@runtime.memoryExhausted`. Schema: the
`test` payload gains `approvals` (Q40) and any field of the chapter's
table that was missing. The limit name `testReductions` and the widened
meaning of `@test.exhausted` are Q27's, to be written into 05 by the spec
batch that lands the ruling, in the spelling convention fix E set.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-interp -p vibra-types -p vibra-lower -p vibra-exec -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; every `V2-PROJECT-test-*` and `V2-GRANT-test-*` case of
M2 passes unchanged; every crate that depends on `vibra-interp` is in the
test line.

## Stays unavailable

A per-test time limit (Q29).

## Excluded

The interpreter's counting of reductions (exists); eval's limits (11);
every other file of `vibra-interp`.

## Completion evidence

- [ ] `conformance/rules/m3-18.tsv` has one `active` row per case id.
- [ ] The report lists each field of 05's `TestItem` table beside the
      payload's field.
- [ ] `git diff --stat crates/vibra-interp` names `src/evaluate.rs` and
      `tests/it/test_limits.rs` and no other file.
- [ ] The validation commands exit 0.
