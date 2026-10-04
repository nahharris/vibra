# Step 11 — tests and traps in the Wasm backend

Prerequisite: Step 10 merged. Stage 4A behavior step, WebAssembly backend and
harness. It claims the Stage 4A half of the exit clause that success, typed host
error, propagation, and trap have parity: here the pure cases (success,
propagation, trap), with host errors joining in Stage 4B.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Traps**, **WebAssembly boundary** (the
  status protocol and the failure accessors), **Test assertions**, **Canonical
  value encoding**.
- [Projects](../../spec/04-programs-and-packages.md): **Tests**, **M2 assertion
  contract**, **M3 assertion contract**.
- [Tooling](../../spec/05-tooling.md): the `test` payload and result precedence.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): the
  `@test-run.v1` observation.
- [M4 ledger](decision-ledger.md) rows D8.1–D8.3, D13.2.

## Scope

The `workspace-test` observation in both backends, with every trap code and its
origin: each selected test in a fresh instance through `vibra_v1_test`; the
three assertion members and the canonical equality of `assert.equal`; the
outcomes `@test.passed`, `@test.assertion-failed`, and `@test.trap`; and the
traps `@runtime.unobservable-function` and `@runtime.invalid-host-value`.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-workspace/src/test_runner.rs` | The interpreter's runner and result shapes, which the module path must reproduce |
| `crates/vibra-wasm` | Generated canonical equality per type (NaN equal to NaN, zeros by sign, a function is the trap), the assertion failure record, and the origin table builder |
| `crates/vibra-wasm-run` | Fresh instance per test, the status reader, and mapping an origin ordinal back to a span and operand type |
| `crates/vibra-conformance/src/workspace_semantic.rs` and the `workspace-test` handler | Compares `@test-run.v1` snapshots against both backends |

## Ordered tasks

1. Assertion lowering: on a false `assert.true`, `assert.false`, or
   `assert.equal`, record the assertion, its origin ordinal, and the two operand
   values (scalar bits or IDs), set status `3`, and stop that test.
2. Canonical equality in generated code, and the host-side expected and actual
   strings built by the same encoder as results.
3. Trap recording: write the code and origin before `unreachable`; map every
   other engine trap to the toolchain defect; map memory exhaustion to the
   host event.
4. The origin table: a deterministic list of source spans with the operand type
   for an assertion call, produced beside the module and consumed by the runner.
5. The `workspace-test` handler runs each test in a fresh instance and builds
   the same `@test-run.v1` record as the interpreter, including audit traces
   (empty in Stage 4A).
6. Parity cases for success, propagation (`try`), assertion failure, the two
   traps, and the host event; move all remaining `workspace-test` rows.

Invariants preserved: a false assertion stops only its test and is not a trap;
a trap stops only its test; each test starts with fresh value state; the
runner never recasts a trap as a host event.

## Test matrix

- Positive: each assertion passing and failing; a test that traps with
  `@runtime.unobservable-function` at the assertion call; the entry result that
  holds a function, unlocated; isolation between two tests.
- Negative: a forced engine trap with no record is reported as
  `@runtime.invalid-checked-program` and fails every case that reaches it; an
  accessor given a released ID is `@runtime.invalid-host-value`.
- Recovery: after a failed test the next test runs normally.
- Boundary: `assert.equal` on NaN and negative zero, on values nested 5,000 deep,
  on a union and a dict, and on a value hiding a function inside `any`.
- Formatter: no change.

## Diagnostic and schema changes

None to the registry. The command-result and test schemas do not yet list
`@runtime.invalid-host-value` as a trap code, because no checked program reaches
it before Stage 4B ([ledger D13.2](decision-ledger.md)).

## Validation

The [Step 4–12 row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step). Record corpus counts
per backend; at the end of this step the Wasm backend has no `not lowered` row.

## Excluded

Effectful tests, recorded responses, and provider failures (Step 13 onward);
source maps (M7); any new assertion member.

## Completion evidence

Every `workspace-test` and `workspace-run` case is `matched`; each registered
trap code has a case with its origin in both backends.
