# Step 07: procedures-run

Kind: vertical. Slice 1 completed for the empty row: a test body is a
procedure context, so a checked procedure runs under `workspaceTest` from
source to result.
Depends on: 05, 06.
Crates: `vibra-types`, `vibra-workspace`, `conformance/cases`.

## Prerequisites

- Read: [source chapter](../../../../spec/01-source-language.md#test)
  "`test`"; [effects
  chapter](../../../../spec/03-effects.md#cold-plans-and-performed-rows)
  "Cold plans and performed rows"; [programs
  chapter](../../../../spec/04-programs-and-packages.md#tests) "Tests",
  "Assertions"; [tooling
  chapter](../../../../spec/05-tooling.md#tests-and-selection) "Tests and
  selection".

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/check/testing.rs:101` | `test.view.takes().is_none()` | calls `effects::procs::test_header` (step 04) |
| `crates/vibra-workspace/src/test_runner.rs:611` | `fn run_one` | unchanged |
| `crates/vibra-workspace/src/test_runner.rs:664` | `fn execute` | calls `run_test` with no world and no host |
| `crates/vibra-conformance/src/workspace_test.rs:28` | `pub struct WorkspaceTestHandler` | unchanged |

Proposed: the body of `effects::procs::test_header`.

## Files owned

- `crates/vibra-types/src/check/effects/procs.rs`
- `crates/vibra-types/src/check/tests/procs.rs`
- `crates/vibra-workspace/src/test_runner.rs`
- `conformance/rules/m2-07.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `test_header`: a test with no `takes`, and with no `performs` or
   `performs {}`, is supported; its function is lowered with
   `BodyContext::Procedure` and the empty row. A test with `takes`, or
   with a non-empty row, stays `@tool.unavailable` over the test.
   Invariant: a test is never given a `Proc` type; it is not applicable.
2. `await` is valid in a test body under the rules of step 05, and not
   inside an anonymous `fn` written there.
3. The runner passes no world and no host context; a test that reaches a
   host body is `@test.unavailable`.

## Test matrix

`workspaceTest`, profile `interpreterV2`, a tree with `src/app/main.vib`
and `tests/run.vib`; the tests import `@std.assert` and `@app.main`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-EFFECT-await-in-test` | positive | `test "awaits" do assert.equal(4i32 await main.double(2i32)) endtest` | `@test.passed` | 03 Cold plans: "`await` is valid only in a `proc` body and in a `test` body" |
| `V2-EFFECT-await-in-test-anon-fn` | negative, `workspaceCheck` | a test whose body holds `fn returns r: I32 do return await main.double(1i32) endfn` | `@effect.invalidAwait` at the `await` | 01 `return`, `try`, `await`: "valid only inside a `proc` … or a `test`" |
| `V2-RUNTIME-plan-built-not-run` | positive | `main.spin` recurses without end; `test "builds" do let _ be main.spin() endlet assert.true(true) endtest` | `@test.passed` | 03 Cold plans: "Building a plan MUST NOT enter the body" |
| `V2-RUNTIME-plan-awaited-twice` | positive | `let plan be main.double(2i32) endlet assert.equal(8i32 I32.add(await plan await plan))` | `@test.passed` | 03 Cold plans: "each `await` is a fresh invocation of the plan" |
| `V2-RUNTIME-requires-else-on-await` | positive | `main.half` with `requires Ordered.greater(n 0i32) else 0i32`; `assert.equal(0i32 await main.half(0i32))` | `@test.passed` | 06 Callable contract execution: "False: `V` is evaluated once and returned" |
| `V2-RUNTIME-ensures-else-on-await` | positive | a `proc` whose `ensures Ordered.greaterOrEqual(r 0i32) else 0i32` replaces a negative candidate | `@test.passed` with the consequence's value | 06: "`V` is evaluated once, with the result name still bound to the candidate" |
| `V2-RUNTIME-return-await-tail` | boundary | `main.down(100000i32)`, the tail-recursive `proc` of step 05, awaited | `@test.passed` | 07 Required coverage 13: "`return await` reuse their activation for at least 100,000 transfers" |
| `V2-RUNTIME-await-unbounded` | boundary | `test "spins" do let _ be await main.spin() endlet endtest` beside a passing test | the first `@test.exhausted`, the second `@test.passed`; `hostEvent` is `@runtime.memoryExhausted` | 05 Tests and selection: "`@test.exhausted` … the item exhausted its memory" |
| `V2-EFFECT-test-row-empty-written` | positive | `test "empty row" performs {} do assert.true(true) endtest` | `@test.passed` | 03 Effect ceilings: "A row is written with `performs` on … a `test`" |
| `V2-TOOL-unavailable-test-row` | boundary | `test "row" performs {Missing} do endtest` beside `test "plain" do assert.true(true) endtest` | the first `@test.unavailable` with `@tool.unavailable` over the test, the second `@test.passed` | 05 Tests and selection: "marks `@test.unavailable` only the items whose declarations reach it" |

## Diagnostics and schema changes

Codes: `@effect.invalidAwait`, `@tool.unavailable`. No schema change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; the ten cases above pass.

## Stays unavailable

`@tool.unavailable` over a test with `takes` or a non-empty row;
`@test.unavailable` for a test that reaches a host body.

## Excluded

`TestWorld` (21); rows (08).

## Completion evidence

- [ ] `conformance/rules/m2-07.tsv` has one `active` row per new case id.
- [ ] Each `@testRun.v2` snapshot was written by hand; the report says
      which sentence fixes each result.
- [ ] The validation commands exit 0.
