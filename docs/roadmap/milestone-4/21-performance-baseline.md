# Step 21 — performance baseline

Prerequisite: Step 20 merged. Evidence step: it measures and records and claims no
language behavior. Its tool and its host test are infrastructure for the evidence; it
changes no observable behavior and no specification. M7 compares its optimizations
against what this step records.

## Read before editing

- [README](README.md#performance-baseline): the definition of the baseline (the named
  programs, the exact metrics, and the informational medians).
- [Ledger](decision-ledger.md) rows D29.1, D14.2.
- [Runtime](../../spec/06-runtime.md): **Reclamation** (`vibra_v1_live_size`),
  **Determinism and observability**.
- [Validation](validation.md).

## Scope

1. A measuring tool (proposed as a `vibra-conformance` binary) that runs the ten
   programs of the README list in the interpreter and in the Wasm backend and writes
   `docs/roadmap/milestone-4/performance-baseline.vibon`, an `@perf-baseline.v1`
   document.
2. The exact metrics for each program and backend: module bytes (Wasm), the allocation
   count, the peak live arena size, and fuel (Wasmtime fuel, with metering enabled
   in the measuring runner only; the interpreter's reduction count).
3. The informational wall-time medians over a stated number of runs.
4. A host test that regenerates the exact metrics and fails on any difference from
   the checked-in file; the medians never participate.
5. The investigation of ledger D14.2 (the seven-minute debug-build test over a value
   nested 5,000 deep), recorded with its cause before the number is recorded.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-conformance/src/wasm.rs`, `runner.rs` | Running a program in both backends |
| `crates/vibra-wasm-run/src/instance.rs` | The engine configuration; the place fuel is enabled for measurement |
| `crates/vibra-interp/src/lib.rs` | A reduction counter behind a measuring hook that is a no-op otherwise |
| `crates/vibra-conformance/tests/stage_4a_demo_m4_step12.rs` | The deep walk's setup |
| `examples/` | The M3 demo programs and the effectful walk |

## Ordered tasks

1. Choose and name the ten programs, fixing their sizes; put each where the corpus
   or the examples already hold it, so that the baseline measures programs that CI
   already runs.
2. Add the measuring hooks: allocation counting at the arena allocator and the
   interpreter's allocation points, a peak live-size record, the fuel setting, and
   the reduction counter. A hook adds no observable behavior and no cost when off.
3. Write the tool and the `@perf-baseline.v1` writer, canonical VIBON with the
   median fields marked informational.
4. Investigate the nested-value cost, fix or record it, and then record the baseline.
5. Add the host test and check that a deliberately perturbed program changes it.
6. Record the environment of the medians (machine, Rust and Wasmtime versions) in
   the document.

Invariants preserved: metrics are deterministic (two runs produce the same exact
numbers); the document is generated, never edited by hand; the tool changes nothing
the corpus observes.

## Test matrix

- Positive: two consecutive runs give identical exact metrics; the checked-in file
  equals a regeneration.
- Negative: a program change moves a metric and the test fails with the program's
  name and the metric; a missing program fails.
- Recovery: the tool fails without writing when any program traps.
- Boundary: the largest program (the tail loop of 10^6) completes under the runner's
  memory limit; the 5,000-deep value completes in a debug build in a bounded time.
- Formatter: the document is canonical VIBON; `fmt` leaves it unchanged.

## Diagnostic and schema changes

None. The baseline is a VIBON data document with a typed decoder in the tool.

## Validation

The [pre-merge list](validation.md#before-merging-each-step) and the new host test,
plus a run of the tool whose output is compared with the committed file.

## Excluded

Any optimization, a threshold that fails a build on wall time, a comparison across
machines.

## Completion evidence

The committed `performance-baseline.vibon`, the cause of the nested-value cost, and
the passing host test, recorded in the README.

## As built

Landed on `claude/m4-step-21-performance-baseline`, conditional on its pull request merging.
The prose is in [the note](performance-baseline.md); the data is in `performance-baseline.vibon`.

- The tool is the `perf-baseline` binary of `vibra-conformance` (`--write`, `--check`,
  `--runs`), the library module `baseline`, and the host test `performance_baseline_m4_step21`
  (11 tests: the checked-in exact metrics equal a regeneration, two measurements agree, a changed
  program and a moved metric are named, a missing or extra program fails, a failing program is an
  error naming it, the document is canonical VIBON and round-trips, the 5,000-deep value runs in
  bounded debug time, and a measured module computes what the plain one does).
- Hooks. The interpreter counts allocations and reductions beside its existing activation and
  tail-transfer counters (`Execution::allocation_count`, `reduction_count`; the same on
  `TestExecution`, which also exposes `peak_memory_bytes`); they change no run. The Wasm
  allocator counts only in a module from `emit_measured` or `emit_tests_measured`, in two state
  fields at 112 and 120 that no other module writes, so `emit` is byte-identical to before. Fuel
  is metered only by a runner from `Runner::with_fuel`; the conformance runner's configuration is
  unchanged.
- Exact metrics: module bytes, fuel, allocations, peak live bytes (Wasm); allocations, peak bytes,
  reductions (interpreter). Recorded only: medians over 11 runs and the machine. None is
  per-platform; see the note for why and for the one residual risk.
- D14.2 reproduced and found already fixed by Step 3 (linear now); nothing to fix.
- Known costs recorded for M7: the quadratic `array.fold` body (D10.5, not rewritten), a result
  allocation per checked arithmetic call, and the type-directed canonical equality (D8.4).

### Left for Step 22

The M4 demo and exit sweep. Step 22 re-runs the tool's `--check` on the exit head.
