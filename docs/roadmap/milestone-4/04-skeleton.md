# Step 4 — Wasm backend skeleton and differential harness

Prerequisite: Step 3 merged. Infrastructure step: it adds crates, a dependency
set, the runner contract, the parity inventory, and a CI job, and claims no
language behavior. It lowers no source form beyond the empty entry its harness
needs to prove the pipeline end to end.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **WebAssembly boundary** (the module
  contract, the feature baseline, the export table), **Traps**,
  **Determinism and observability**.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): **Conformance
  corpus**, **Conformance profiles** and **Differential execution**,
  **Required implementation suites**.
- [Roadmap architecture boundary](../v1.md#architecture-boundary) and
  `crates/vibra-conformance/tests/architecture_boundary.rs`.
- [README](README.md#fixed-implementation-decisions): the engine and encoder
  decision and the dependency evidence; [M4 ledger](decision-ledger.md) rows
  D4.1–D4.4, D5.1–D5.3, D6.1–D6.4, D7.1, D11.2.

## Scope

1. The emitter crate `vibra-wasm` (proposed name): lowers a checked program to
   module bytes and an origin table. It depends only on `vibra-ir`,
   `vibra-diagnostics`, and `wasm-encoder`.
2. The runner crate `vibra-wasm-run` (proposed): the only crate that depends on
   Wasmtime, and the one that supplies the `vibra_native_v1` imports from the
   native-code crate `vibra-native` (proposed, Step 8c), which the emitter never
   depends on. It validates a module with `wasmparser` under exactly the baseline
   feature set, instantiates it, calls the exports, and reads the statuses.
3. The harness in `vibra-conformance`: the Wasm execution handler, the
   per-backend report lines, the memory limit, and the parity inventory with its
   test.
4. A CI job for the engine on the three platforms.

## Entry points

| File | Use |
| --- | --- |
| `Cargo.toml` `[workspace.dependencies]`, `Cargo.lock` | Add `wasm-encoder`, `wasmparser`, and `wasmtime` with `default-features = false` and the smallest feature set that compiles with Cranelift. Use the latest Wasmtime, `wasm-encoder`, and `wasmparser` (decided by Hannah, 2026-10-02), and raise `rust-toolchain.toml` in this same PR to the Rust version they require (1.96 for Wasmtime 49.0.2) |
| `crates/vibra-conformance/tests/architecture_boundary.rs`: `ARCHITECTURE` | One row per new crate. `vibra-conformance` gains the runner crate; the emitter row lists no engine crate and not the native-code crate, and the test forbids every other crate from depending on the runner |
| `crates/vibra-conformance/src/bin/conformance.rs`, `runner.rs`, `profile.rs` | Handler registration, `CaseReport`, and the summary lines |
| `crates/vibra-conformance/src/corpus.rs` | Where the parity inventory file is discovered |
| `.github/workflows/ci.yml` | The new job |

## Ordered tasks

1. Add the dependencies behind `cargo fetch --locked`, record versions,
   licences, and added build time, and prove `--offline` builds. Stop and
   report if any CI platform fails to build the engine.
2. Add the crates and the architecture rows. Emit the smallest conforming
   module: one memory exported as `vibra_v1_memory`, only the imports of the pure `vibra_native_v1` module that the program uses (none for the empty entry), the exports of the
   [boundary table](../../spec/06-runtime.md#webassembly-boundary) that a
   program with a `void` entry needs, and no custom section. Every other
   form returns a typed `NotLowered` error naming the form, never a wrong module.
3. Determinism: a host test that emits one checked program twice, and in two
   processes, and compares bytes ([ledger D4.4](decision-ledger.md)).
4. Validation: a host test that validates the module with exactly the baseline
   features and fails if the module uses another (tail-call, GC, SIMD, threads,
   reference types, memory64).
5. Runner crate: engine configuration with Cranelift only, NaN canonicalization
   on, every optional proposal off, fuel and epoch unused, and a resource
   limiter that applies the runner's memory limit and reports growth failure as
   the host event. Interpret the statuses by the boundary's protocol; a stop with
   status `0` is the toolchain defect.
6. Harness: run each executable case in the interpreter and, when it lowers, in
   Wasm, against the one expectation; report the two backend lines and the
   `not lowered` count; keep the by-profile lines and the total unchanged
   otherwise.
7. Parity inventory: a checked-in table `conformance/parity.tsv` with one row
   per executable case (100 at `388dfe1`, the head Step 4 landed on), `matched` or `not-lowered` with an
   owning step, and the host test of
   [Differential execution](../../spec/07-diagnostics-and-conformance.md#differential-execution).
   Initialize every row to `not-lowered` with the step that owns the form the
   case uses, except the cases the skeleton genuinely matches.
8. A host test over the typed IR's canonical form that no index, offset, or
   instance identity appears in it ([ledger D2.2](decision-ledger.md)).
9. CI job: build and test the two crates and run the harness on Linux, Windows,
   and macOS.

Invariants preserved: the interpreter handlers and every existing expectation
are unchanged; no case gains a Wasm expectation; `run` and `test` gain no
option and `build` stays `@tool.unavailable`.

## Test matrix

- Positive: the empty-entry program runs in both backends and matches;
  emission is byte-identical across runs; the module validates under the
  baseline; the report prints both backend lines.
- Negative: a module with an import outside `vibra_native_v1`, a custom section, an exported memory under another name, or a
  non-baseline instruction is rejected by the validation test; a case whose row
  is missing, whose row names no case, or whose `not-lowered` row names no step
  fails the inventory test; a `matched` case on which Wasm disagrees fails; a
  forced Wasm disagreement fails the case and names the backend.
- Recovery: a `not lowered` case still passes on the interpreter side and does
  not fail the run.
- Boundary: the memory limit exactly at, and one page under, the empty module's
  need; a stop with no recorded status maps to `@runtime.invalid-checked-program`.
- Formatter: no change.

## Diagnostic and schema changes

None: the codes and statuses were closed in Step 1. The runner's report lines
are human output of the internal runner and are not a machine schema.

## Validation

The [Step 4 row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step), plus the parity
inventory test and the determinism test. Record corpus counts by profile and by
backend, and the dependency evidence of
[Dependency steps](validation.md#dependency-steps).

## Excluded

Lowering of any source form (Steps 5a onward); a backend option on `run` or
`test`; `vibra build`; custom sections and source maps (M7); optimization.

## Completion evidence

Both crates and the CI job exist; the inventory test and determinism test pass;
the harness report shows the interpreter backend at 100 of 100 and the Wasm
backend with its matched and not-lowered counts; the dependency evidence is in
the PR; the README row for Step 4 records the engine version.

## As built

The step landed as proposed, with these choices, which the later steps build on.

- **Crates.** `vibra-wasm` (the emitter) depends on `vibra-ir` and `wasm-encoder`
  only; its tests use `vibra-diagnostics`. `vibra-wasm-run` depends on `vibra-ir`,
  `wasmparser`, and Wasmtime (Cranelift only, with no optional engine feature),
  and no other crate depends on it but `vibra-conformance`. The export names, the
  native import module, and the status, trap, and failure codes are written once
  in `vibra_ir::boundary`, which both crates read, so neither depends on the other.
- **The module.** One 32-bit memory of one page, exported as `vibra_v1_memory`;
  no import and no custom section; and the exports `vibra_v1_entry`,
  `vibra_v1_status`, `vibra_v1_trap_code`, `vibra_v1_origin`, `vibra_v1_result`, and
  `vibra_v1_live_size`, which are the accessors of the boundary table that depend
  on no value kind and no test. The specification's table is the contract, and a
  Stage 4A module exports all of it by the end of the stage: Step 5a adds the
  accessors of values (`vibra_v1_release`, `vibra_v1_variant`, `vibra_v1_length`,
  `vibra_v1_read_i32`, `vibra_v1_read_i64`, `vibra_v1_read_f32`,
  `vibra_v1_read_f64`, and `vibra_v1_read_id`) and Step 11 adds `vibra_v1_test` and
  the failure exports (`vibra_v1_failure`, `vibra_v1_failure_expected`, and
  `vibra_v1_failure_actual`). The runner's validation admits any subset of the table
  and the whole of it, and rejects every export outside it; the runner already
  reads every status.
- **What lowers.** A function with no parameter, the result `void`, and a body
  that is the `void` literal alone or in a sequence, which is what `(do)` checks
  to. A body with no effect is lowered by lowering nothing. Every other node is
  named: `NotLowered` lists each distinct form with the first origin that uses it,
  and a registry row by its symbol and a call by its kind, because their lowering
  differs by step. Type definitions lower nothing until a value is built of one.
- **The harness.** The `interpreter-v1` handlers hand the checked program to
  `vibra-conformance`'s Wasm module, which emits, validates, runs under the one
  memory limit, and reports a `WasmObservation`; the runner compares it with the
  case's one expectation and reports both backends. An executable case is one
  whose expectation is accepted, so a rejected case reaches no backend, has no
  parity row, and is in neither backend line ([ledger D6.5](decision-ledger.md)). A
  `workspace-test` case that is accepted is not lowered, by `test-module`, until
  Step 11. The `wasm` field of the case manifest, which the specification says no
  case carries, is gone, and a manifest that writes it is a decoding error.
- **The memory limit.** `INSTANCE_MEMORY_LIMIT_BYTES`, 64 MiB, replaces
  `INSTANCE_MEMORY_BUDGET`. The interpreter's budget and the Wasm runner's limit
  both derive from it, and a refused growth is the host event in both.
- **The parity inventory.** `conformance/parity.tsv` has a row for each of the 100
  executable cases, all not lowered, each with its owning step: 5b 12, 6 17, 7 4,
  8a 6, 8b 13, 8c 2, 9 28, and 11 18. The owning step is the latest step among
  the forms the case needs. Step 2b made `true` and `false` module values of every
  checked program, and a module value needs the arena, so no checked source
  program lowers before Step 5b, and the empty entry enters the harness as a
  hand-built program (`wasm_skeleton_m4_step4`, `differential_m4_step4`).
- **CI.** The `check` job builds and tests the workspace, Wasmtime included, on the
  three platforms, and the `conformance` job runs the corpus in both backends on
  the same three after `check`, sharing its build cache per platform, so the
  engine compiles once per platform and not twice.
