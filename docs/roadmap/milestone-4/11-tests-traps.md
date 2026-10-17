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

## As built

Landed as `landed`, conditional on its pull request merging; the base is Step 10 (PR #368, merge `20db8c6`). The step is one pull request; the seam for review is the crates below.

- **Test module** (`vibra-wasm`: `emit_tests`, `runtime::test_export`; ledger D8.5). A test module exports `vibra_v1_test` in place of `vibra_v1_entry`, with the same accessors plus the three failure exports (state fields `failure`, `expected`, `actual`, addresses 92, 96, 104). Every test of a workspace shares one module set, so the module is made from any one test program and the list of entry indices in canonical discovery order. `workspace-test` runs each test of that list in a fresh instance (`Runner::run_test`) and writes the rows with the interpreter's own writer (`test_run.rs` of the conformance crate), an empty `@audit-trace.v1` for each.
- **Assertions** (`lower/assertion.rs`). A call of one of the closed assertion functions is lowered where it is made. A false one records status `3`, the assertion, the origin ordinal, and for `assert.equal` the two operands (scalar bits, or the IDs of values, or an interface value for an operand of a generic type), and stops that test only.
- **Canonical equality** (`canon.rs`, ledger D8.4). One iterative walk over the arena compares two values, or scans one for a function, with a stack in a block of the arena. NaN is replaced by the quiet NaN, zeros differ by sign, a dict compares its entries in key order, an interface value compares its descriptor and its value. A value that can hold a function is scanned first, so a function in either operand is `@runtime.unobservable-function` at the assertion call.
- **Origin table** (`OriginTable`, ledger D8.2). Ordinals are assigned by the lowering in the order it first needs a row, one per assertion call, with the operand type of an `assert.equal`; the host maps an ordinal to the source span and reads the operands by the type. The table is not in the module. Both backends report the same code and the same origin for each registered trap: `trap_origins_m4_step11` holds `@runtime.invalid-checked-program`, `@runtime.unobservable-function` (at the assertion call in a test, none for the entry result), and `@runtime.invalid-host-value` to it.
- **The three forms of Step 9** (ledger D9.9): a result converted through an array, a dict, a record, or a union; a bare generic parameter that receives an interface value, which needs no opening; and the `void` item of a builtin `option` iterator. Each has a corpus case matched in both backends. The interpreter did not yield the item of a `some` of `void`; it does now, as `02-type-system.md` says.
- **No `NotLowered` form.** `Form` has three kinds, none a construct a checked program contains (`lowers_every_form_m4_step11`: every registry row has a lowering, every program of the corpus lowers, and the corpus reports no case as not lowered).
- **Parity.** All 18 rows owned by Step 11 are `matched`, and the three new cases are added: 128 of 128 executable cases matched. A host event in a test (`memory_exhaustion_in_a_test_is_the_host_event_and_not_a_trap`) is held by a host test; the corpus manifest allows `expect.host_event` on `interpret` and `workspace-run` only, so there is no `workspace-test` case for it.
