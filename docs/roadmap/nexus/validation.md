# Nexus validation

How a work package proves itself, what must stay true whatever a package
does, and which evidence closes each roadmap clause.

## Standing rules

These hold for every package and are not restated in the plans. They restate,
for Nexus, rules of [AGENTS.md](../../../AGENTS.md) and the
[execution model](../execution.md).

1. **The interpreter is the oracle and expectations are authored from the
   specification.** An expectation is never regenerated from the stepper's or
   a native executable's output. A disagreement is a defect of the newer
   executor until the specification says otherwise.
2. **No partial programs.** A lowering, the stepper, and the code generator
   each reject a program they cannot handle in full, by name
   (`NotLowered`, `Unsupported`, `EmitError`). None runs or emits part of one.
3. **Pending is not passing.** A `pending` cell of the parity inventory, a
   test recorded as not run, and an unavailable case each count as a gap. No
   package removes a failing case, downgrades its profile, or accepts a
   snapshot wholesale to get green.
4. **Every comparison runs twice:** unbounded, and with quota 1.
5. **Extending the execution IR is four parts in one change:** the
   instruction, its verification rule and tests, the stepper, the generator.
6. **`unsafe` lives in `vibra-nexus-bridge` only.**
7. **Nothing is advertised early.** No CLI command, schema field, or profile
   mentions native execution before X3-23, or actors before X4-S1 is approved.
8. **A design gap stops the package.** If a validation line cannot hold, or a
   ledger row turns out wrong, the agent reports the line and the reason. The
   specification or the ledger is corrected first.
9. **A reviewer checks validation lines, not the diff's size.** Each line is
   a named test, a command, or a count.

## Before merging a package

From the repository root, with the pinned toolchain. Check every exit status:
PowerShell does not stop on a failing native command by itself.

```powershell
$env:RUSTFLAGS = '-D warnings'
$env:RUSTDOCFLAGS = '-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-targets --all-features
cargo doc --locked --offline --workspace --no-deps --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline -p vibra-conformance --test it -- architecture_boundary:: diagnostic_registry:: evidence:: ambient_reads::
git diff --check
```

Focused runs while working:

```powershell
cargo test --locked --offline -p vibra-exec --test it
cargo test --locked --offline -p vibra-nexus
cargo test --locked --offline -p vibra-nexus-eval --test it
```

## Baseline

Recorded on `worktree-nexus-design` with the foundation applied, on Windows
x64 (`rustc 1.96.1`), 2026-11-11. A package that changes none of these states
so; one that does states the new numbers and why.

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | Pass |
| `cargo clippy … --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test … --workspace --all-targets --all-features` | **One failure, inherited from `main`**: `evidence::spec_examples_are_inventoried_and_losslessly_exercised` (397 tests in `vibra-conformance --test it`: 396 passed, 1 failed, 2 ignored). Every other test target passes, including the 24 foundation tests |
| `cargo doc … -p vibra-exec -p vibra-nexus -p vibra-nexus-eval` | Pass. The whole-workspace doc build was not run |
| Conformance corpus | 595 passed, 0 failed, 0 unavailable: 89 reader, 305 static, 181 interpreter, 20 tooling. Interpreter backend 173 passed; Wasm backend 173 matched, 0 failed |
| `architecture_boundary::`, `ambient_reads::` | Pass, with the three new crates in the table |

The inherited failure is not caused by Nexus work: the syntax-example
inventory `docs/roadmap/v0.0.1/m1/syntax-examples.tsv` is stale against
`docs/spec/04-programs-and-packages.md`, and CI on `main` at `c81a2836` fails
the same way. Until it is repaired on `main`, a package reports this one test
as the known failure and any other failure as its own.

## Foundation evidence

What the three foundation crates already prove, and what each proof does
**not** cover. All of it is at the execution-IR level, through the reference
stepper. None of it is a native executable.

| Test | Evidences | Clause | Still owed |
| --- | --- | --- | --- |
| `vibra-exec::it::v1_…` to `v6_…` (9 tests) | Each verification rule accepts a valid program and rejects its violation | G3.M1.3, G3.M1.11 (invalid IR) | The rules for instructions added after the foundation |
| `vibra-nexus-eval::it::the_demo_tail_calls_indirectly_performs_one_host_operation_and_traps_with_its_origin` | Indirect tail transfers between two functions, one audit event, a trap with origin ordinal 2 | G3.M1.8 | The same from Vibra source, natively, compared with the interpreter (X3-09) |
| `…::yielding_at_every_checkpoint_changes_no_observation` | Quota 1, 2, and 7 give the same outcome, result, and audit trace as unbounded, with exactly one host event | G3.M1.9 | The corpus (X3-21) |
| `…::tail_transfers_keep_the_frame_count_and_the_live_size_bounded` | 10,000 and 100,000 transfers both peak at 2 frames, end at live size 0, and 100,000 run in 1 MiB | G3.M1.10 | Native measurement (X3-09) |
| `…::non_tail_recursion_is_bounded_by_memory_and_uses_no_native_stack` | 100,000 nested activations on a 256 KiB thread stack | G3.M2.4 | Native (X3-21) |
| `…::exhausting_the_limit_is_the_host_event_and_a_larger_limit_recovers` | The limit ends a run as `MemoryExhausted`, not a trap | "Activations and memory" | Exit code 3 through the launcher (X3-05) |
| `…::a_module_value_is_initialized_once_however_often_it_is_read` | One initializer run, one host event, across two reads and across yields | "Module-value initialization" | Source-level cases (X3-17) |
| `…::a_function_value_called_with_another_arity_is_a_toolchain_defect` | `@runtime.invalid-checked-program`, no origin | "Traps" | — |
| `…::invalid_execution_ir_is_rejected_before_it_runs`, `…::an_incompatible_runtime_identity_and_an_unknown_symbol_are_refused` | Verification and admission refuse before any step | G3.M1.11 | The launcher's refusal of a linked object (X3-05) |
| `vibra-nexus::store::tests` (6 tests) | Count balance, stale handle is a fault, free-list reuse, limit as host event, release of a value 100,000 deep on a 128 KiB stack, wide release | "Reclamation" | The M4 arena constants (X3-02) |

## Gate map

Each roadmap clause, the package that closes it, and the form its evidence
takes. A clause is closed when the evidence is recorded in the milestone's
README, not when the package merges.

| Clause | Closed by | Evidence form |
| --- | --- | --- |
| G3.R4, G3.M1.1 | X3-S1 | Approved specification PR |
| G3.M1.2, G3.M1.13 | X3-S2, X3-07, X3-08 | Feasibility record; a linked executable per target |
| G3.M1.3 | X3-01, X3-03 | Verifier tests; `every_lowered_program_verifies` |
| G3.M1.4 | X3-05, X3-06, X3-07 | Object symbol and relocation tests; a launched `.exe` |
| G3.M1.5, G3.M1.6 | Foundation, X3-05 | Foundation tests; `the_bridge_drives_the_demo` |
| G3.M1.7 | X3-09 | Design §11 reviewed against the as-built ABI |
| G3.M1.8–G3.M1.12 | X3-09 | The X3-09 table |
| G3.M2.1, G3.M2.2 | X3-10…X3-19 | Inventory rows `matched` per executor, twice |
| G3.M2.3 | X3-20 | `smoke_<target>_<aspect>` tests |
| G3.M2.4–G3.M2.6 | X3-21 | Named tests and one case per diagnostic |
| G3.M2.7–G3.M2.10, G3.R6 | X3-22 | Zero pending cells; baseline; two audits |
| G3.M3.1, G3.M3.2, G3.R7 | X3-23 | Schema-validated build results; identity tests; dependency test |
| G3.M3.3 | X3-24 | Byte-equal builds per target |
| G3.M3.4 | X3-25 | Clean-machine install job |
| G3.M3.5, G3.R5 | X3-26 | Empty Wasm search; both suites |
| G3.M3.6, G3.M3.7 | X3-27 | Audit record |
| G3.R8 | X3-09, X4-05 | Design §11; the actor transfers reuse it |
| G4.1, G4.7–G4.10 | X4-S1 | Approved specification PR |
| G4.2 | X4-01, X4-03 | Kernel tests; their source-level cases |
| G4.3, G4.9 | X4-03, X4-04 | Origin checks; the causal-path query |
| G4.4 | X4-05, X4-06 | Kernel tests on the Nexus executor; replay inventory |
| G4.5, G4.6 | X4-07 | Ownership stress test; placement test |
| G4.G1–G4.G4, G4.G6 | X4-08 | The X4-08 table |
| G4.G5 | X4-09 | Overlap intervals with hardware recorded |

## Reporting

A package's handoff records, as the execution model requires: base and tested
commit; each validation line with pass, fail, or not run; the exact commands
and exit statuses; corpus counts and the inventory counts per column;
remaining scope; and PR state. A command not run is recorded as not run. A
dependency download failure is not a passing test.
