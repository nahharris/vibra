# Step 26: cli-test

Kind: vertical. `vibra test [TEST]`: the test runner of step 23 behind the
command line, with selectors, the `test` payload, the suite result, and the
human report. It completes the tool surface of slice 8.

## Prerequisites

- Steps 23 and 25 landed.
- Read: [tooling chapter](../../../../spec/05-tooling.md) "Tests and selection"
  (the selector, the six numbered rules, the item results, the counts and the
  precedence), "Versioned envelope and results" (results `@command.testFailed`,
  `@command.trap`, `@command.operationalFailure`, `@command.unavailable`),
  "Command payloads" (`test`, `TestItem`, `TestItem.failure`, `TrapRecord`,
  `ObservationError`, the human report paragraph); [runtime
  chapter](../../../../spec/06-runtime.md) "Test assertions", "Traps";
  [diagnostics chapter](../../../../spec/07-diagnostics-and-conformance.md)
  "Runtime and tool failures".

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-workspace/src/test_runner.rs` (step 23) | `run_tests(snapshot, selector, limits)` and its `TestRun` |
| `crates/vibra-cli/src/{check,inventory}.rs` (step 25) | discovery, snapshot, envelope plumbing; one row turns `supported: true` |
| `crates/vibra-cli-v1/tests/it/test_command.rs` | the host-test shapes: selector errors, ordering, mixed results |
| `crates/vibra-cli-v1/src/lib.rs:32` (`MEMORY_BUDGET`) | the per-test memory budget constant and how it reaches the interpreter |

Proposed modules: `vibra-cli::{test,selector}`; the `test` payload,
`TestItem`, `TrapRecord`, and `ObservationError` in
`schemas/v2/command-result.json`.

## Ordered tasks

1. Selector: the test module's atom, `::`, and one canonical string
   literal, with no other whitespace, the module beginning `@tests.`. The
   string is decoded by the literal reader of step 03. A malformed selector
   is `@command.invalidInput`, exit 2, with no diagnostics.
2. Order of reporting: project, source-graph, and required parse errors
   first, with zero counts and no items. Without a selector every test
   module is parsed; with one, its module is. A well-formed selector that
   names no module or no test is `@command.invalidInput`.
3. Run through `run_tests`; map `TestRun` to the payload: `selected`,
   `passed`, `failed`, and `tests` with `name` (the selector text),
   `result`, `failure`, `trap`, `auditTrace` (no events in M1),
   `diagnostics` (the codes attributed to the item), `status`. The JSON
   fields are the projection of the `@testRun.v2` record of step 23, with
   `null` for an absent optional field. Invariant: a diagnostic
   appears once in the envelope and is attributed to every item it affects.
4. Suite result by precedence: trap (`@command.trap`, exit 5), operational
   exhaustion (`@command.operationalFailure`, exit 3), unavailable
   (`@command.unavailable`, exit 4), test failed (`@command.testFailed`,
   exit 1), ok. A selection invalidated by static errors is
   `@command.diagnostics`, exit 1, with every item `@test.invalid`.
   Envelope status: `passed` only when every selected item passed;
   `refuted` on a failed assertion; `unknown` after exhaustion;
   `unsupported` when an item is unavailable and none failed.
5. Memory: each test runs under the published budget; exhaustion is the
   host event `@runtime.memoryExhausted` on that item and operational
   exhaustion for the suite; it is not a trap.
6. Human report: selectors and result atoms; each item that did not pass
   with its diagnostic, failure, or trap; the three counts last.
7. Inventory: `test` becomes `supported: true` with profiles `staticV2` and
   `interpreterV2`.

## Test matrix

Host tests in `crates/vibra-cli/tests/it/test_command.rs`, each on a
confined project tree. The language behaviour is covered by the
`V2-PROJECT-test-*` cases of step 23; these cover the command.

| Test | Kind | Input | Expected |
| --- | --- | --- | --- |
| `test_all_pass` | positive | `tests/math.vib` with `test "adds" do assert.equal(3i32 math.add(1i32 2i32)) endtest`; `vibra --format json test` | exit 0; `selected: 1`, `passed: 1`, `failed: 0`; item `@tests.math::"adds"` `@test.passed` |
| `test_failure` | negative | `assert.equal(4i32 math.add(1i32 2i32))` | exit 1; `@command.testFailed`; `failure.kind: "assertion"`, `expected: "4i32"`, `actual: "3i32"`, a `primarySpan` |
| `test_selector` | positive | two tests; `vibra test '@tests.math::"adds"'` | `selected: 1` |
| `test_selector_escaped_name` | boundary | a test named `"a \"b\""` selected by its canonical literal | `selected: 1` |
| `test_selector_malformed` | negative | `@tests.math:"adds"`, `@tests.math:: "adds"`, `@app.math::"adds"`, `@tests.math::adds` | exit 2 each; no diagnostics |
| `test_selector_no_module` | negative | `@tests.none::"x"` | exit 2 |
| `test_selector_no_test` | negative | `@tests.math::"missing"` | exit 2 |
| `test_parse_error_first` | recovery | `tests/math.vib` with `test "x" do endfn` | exit 1; `@syntax.unmatchedDelimiter`; `selected: 0`; no items |
| `test_selector_parses_one_module` | boundary | a selector for `@tests.a` while `tests/b.vib` has a syntax error | the selected test runs; no diagnostic from `b` |
| `test_invalid_selection` | negative | one test with `let x: I32 be "a" endlet`, one valid | exit 1; `@command.diagnostics`; both items `@test.invalid`; `@type.mismatch` once, attributed to both |
| `test_unavailable_item` | boundary | one test with `takes {world: TestWorld}`, one plain | exit 4; the first `@test.unavailable` with `@tool.unavailable`, the second `@test.passed`; `passed: 1`, `failed: 1` |
| `test_precedence_unavailable_over_failed` | boundary | one unavailable item and one failed assertion | exit 4 |
| `test_memory_exhausted` | boundary | a test that builds a list past the budget, then one that passes | exit 3; the first item carries `@runtime.memoryExhausted`; the second `@test.passed` |
| `test_observation_error` | negative | `assert.equal` on two `Any` values holding closures | `failure.kind: "observation"`, `observationError.code: "unsupportedValue"`, `@tool.unsupportedValue`; exit 1 |
| `test_empty_suite` | positive | a project with no `tests/` directory | exit 0; zero counts |
| `test_order` | positive | `tests/b.vib` and `tests/a.vib` | items of `a` before `b`, declaration order within |
| `test_human_report` | positive | the failing project in human format | selectors and result atoms present; the counts on the last line |
| `test_json_validates` | positive | every test above in JSON format | each envelope validates |

Formatter: not applicable.

## Diagnostics and schema changes

Codes: `@syntax.unmatchedDelimiter`, `@type.mismatch`, `@tool.unavailable`,
`@tool.unsupportedValue`, `@runtime.memoryExhausted`. Schema: the `test`
payload, `TestItem`, `TestItem.failure`, `TrapRecord`, `ObservationError`
added to `urn:vibra:schema:v2:command-result`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-cli -p vibra-schema -p vibra-workspace
cargo run --locked --offline -p vibra-cli --bin vibra -- --format json help
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: `help` reports four supported commands (`help`, `fmt`, `check`,
`test`); every `test_*` host test passes.

## Excluded

`TestWorld`, grants, and audit events (M2); `run` and `eval` (M2); test
filtering other than the one selector (not in the surface).

## Completion evidence

- [ ] Each of the five item results appears in a host test; `@test.trap` is
      produced by a fault-injection hook compiled only for tests.
- [ ] The precedence function has a table test over all five suite outcomes.
- [ ] Validation commands exit 0.
