# Step 23: tests-and-assertions

Kind: vertical. Slice 8 completed: `test` declarations without grants in
the `@tests` unit, `@std.assert`, test discovery and selection, the test
runner with a fresh state per test, structured assertion failures, the
`@testRun.v2` observation, and the `workspaceTest` handler. Module values
were step 15; their per-run initialization is re-verified under the runner.

## Prerequisites

- Step 17 landed (and 09 for the `@tests` unit).
- Read: [programs chapter](../../../../spec/04-programs-and-packages.md) "Tests"
  ("Test worlds and grants" only for the rule that a test without `takes` has no
  grant; `TestWorld` is M2), "Assertions"; [source
  chapter](../../../../spec/01-source-language.md) "`test`"; [runtime
  chapter](../../../../spec/06-runtime.md) "Test assertions", "Module-value
  initialization"; [tooling chapter](../../../../spec/05-tooling.md) "Tests and
  selection", `TestItem`; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md) the `@testRun.v2`
  row and the test observation paragraph.
- Open questions: Q11 (whether `check` covers `@tests`) is answered before
  step 25, not here.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-workspace-v1/src/test_runner.rs:699` (`run_tests`) | discovery order, selector, fresh state per test, status precedence |
| `crates/vibra-interp/src/machine.rs` (`run_test`, step 13) | the assertion path |
| `crates/vibra-conformance-v1/src/{test_run.rs,workspace_semantic.rs}` | the `@testRun` rendering and handler |

Proposed: `stdlib/src/std/assert.vib`; `vibra-workspace::test_runner`;
`vibra-conformance::workspace_test`; `vibra-types::tests` (test
declaration checks).

## Ordered tasks

1. `@std.assert`: `true`, `false`, `equal(T)` as `external @compiler`
   members of a module with manifest role `@testRegistry`; usable only from
   `@tests` (`@module.assertionOutsideTests` at a reference from a target).
2. Test declarations: only in `@tests` (`@module.testOutsideTests`); a
   module declaring a test imports `@std.assert`
   (`@module.missingRequiredImport` at its first test's name); names unique
   per module by exact scalar sequence (`@name.redeclaration`); body of
   type `Void`; `return` or `try` directly in the body is
   `@type.invalidReturn` / `@type.invalidTry`; `takes` in M1 is
   `@tool.unavailable` over the test (M2); `performs` likewise.
3. `assert.equal` operands: a type containing a callable is
   `@type.functionNotEquatable`; comparison by canonical encoding.
4. Runner: discovery in ascending module identity by UTF-8 bytes then
   declaration order; a selector `@tests.<module>::"name"`; every selected
   test checked before any runs; an ordinary error makes every item
   `@test.invalid` and none runs; an unsupported construct in a test's
   declaration closure makes that item `@test.unavailable` with
   `@tool.unavailable` at the form; each test runs with fresh module state
   and an empty audit trace; a false assertion records `@test.assertionFailed`
   with assertion name, expected and actual encodings, and span, and stops
   that test; a trap ends only its test; `selected`, `passed`, `failed`
   counts; suite precedence trap, exhaustion, unavailable, failed, ok.
5. `@testRun.v2`: `format`, `result`, `tests` with `name`, `result`,
   optional `failure`/`trap`, `auditTrace` (empty in M1).
6. Cases: confined trees with `src/` and `tests/`.

## Test matrix

| Case id | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-PROJECT-test-passes` | positive, workspaceTest | `tests/math.vib`: `use @std.assert enduse\nuse @app.math enduse\ntest "adds" do assert.equal(3i32 math.add(1i32 2i32)) endtest` | `@testRun.v2` with one `@test.passed`; result `@command.ok` |
| `V2-PROJECT-test-fails` | negative, workspaceTest | `assert.equal(4i32 math.add(1i32 2i32))` | `@test.assertionFailed` with `assertion: @std.assert.equal`, `expected: 4i32`, `actual: 3i32`, the call's span; result `@command.testFailed` |
| `V2-PROJECT-test-stops-at-failure` | positive, workspaceTest | two assertions, the first false, the second would trap (unobservable value) | one failure recorded; the second never runs (its effect is absent) |
| `V2-PROJECT-test-order` | positive, workspaceTest | `tests/b.vib`, `tests/a.vib`, two tests each | items in order `a` then `b`, declaration order within |
| `V2-PROJECT-test-fresh-state` | positive, workspaceTest | a module value with an initializer whose evaluation is observable through a counting intrinsic is not pure; so: two tests each asserting a module value equals its constant, with the host test asserting the initializer ran twice | both pass; host assertion |
| `V2-PROJECT-test-missing-assert-import` | negative | a test module without `use @std.assert enduse` | `@module.missingRequiredImport` at the first test's name; every item `@test.invalid` |
| `V2-PROJECT-test-outside-tests` | negative | `src/app/main.vib` containing a `test` | `@module.testOutsideTests` |
| `V2-PROJECT-assert-outside-tests` | negative | `src/app/main.vib` calling `assert.true(true)` | `@module.assertionOutsideTests` |
| `V2-PROJECT-test-duplicate-name` | negative | two tests `"adds"` in one module | `@name.redeclaration` |
| `V2-PROJECT-test-return` | negative | `test "x" do return void endtest` | `@type.invalidReturn` |
| `V2-PROJECT-test-try` | negative | `test "x" do let v be try f() endlet endtest` | `@type.invalidTry` |
| `V2-PROJECT-test-return-in-anon-fn` | positive | a test whose body defines an anonymous `fn` with `return` | accepted |
| `V2-PROJECT-test-invalid-selection` | negative, workspaceTest | one test with a type error, one valid | both `@test.invalid`; none runs |
| `V2-PROJECT-test-unavailable-item` | boundary, workspaceTest | one test using `takes {world: TestWorld}`, one plain | the first `@test.unavailable`, the second runs and passes |
| `V2-PROJECT-test-function-not-equatable` | negative | `assert.equal(inc inc)` | `@type.functionNotEquatable` |
| `V2-PROJECT-test-unobservable` | negative, workspaceTest | `assert.equal(a b)` where both are `Any` holding closures | `@test.assertionFailed` with `kind: observation` and `@tool.unsupportedValue`; not a trap |
| `V2-PROJECT-test-empty-suite` | positive, workspaceTest | a project with no `tests/` | result `@command.ok`, no items |
| `V2-PROJECT-test-trap-isolated` | boundary, workspaceTest | a test that exhausts memory and one that passes (the exhausting one runs first by order) | the first ends with the host event, the second still passes; the suite result follows precedence |
| `V2-PROJECT-test-selector` | positive | selector `@tests.math::"adds"` with two tests present (host test on the runner) | one item selected |

## Diagnostics and schema changes

Codes: `@module.assertionOutsideTests`, `@module.testOutsideTests`,
`@module.missingRequiredImport`, `@name.redeclaration`,
`@type.invalidReturn`, `@type.invalidTry`, `@type.functionNotEquatable`,
`@tool.unavailable`, `@tool.unsupportedValue`. Schema: `@testRun.v2`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-types -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: all cases pass; the fresh-state host test asserts two initializations.

## Excluded

`TestWorld`, grants, host worlds, audit traces with events (M2); the CLI
`test` command (26).

## Completion evidence

- [ ] `stdlib/manifest.vibon` lists `@std.assert` with role `@testRegistry` and
      `assertions: ["assert.true" "assert.false" "assert.equal"]`.
- [ ] Every `@test.*` result value has a case.
- [ ] Validation commands exit 0.
