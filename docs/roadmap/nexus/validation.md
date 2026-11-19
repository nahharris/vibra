# Nexus validation

How a work package proves itself, what must stay true whatever a package
does, and which package closes each gate.

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
   mentions native execution before X3-23, or concurrency before X4-S1 is
   approved.
8. **A design gap stops the package.** If a validation line cannot hold, or a
   ledger row turns out wrong, the agent reports the line and the reason. The
   specification or the ledger is corrected first.
9. **A reviewer checks validation lines, not the diff's size.** Each line is
   a named test, a command, or a count.
10. **Code in the workspace is not evidence.** The native crates were built
    before the language was redesigned (N41). A behavior counts only after
    it has run on the current corpus.
11. **No expectation is a trap caused by user data.** The language has none
    (N44). Tests that inject a defect are kept apart and say so.

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

There is no baseline for the redesigned language yet.
[v0.0.2 M0](../v0.0.2/m0/README.md) records the state of both suites before
any code changes, and X2-01 records the first baseline of the native track.
From then on, a package that changes none of the recorded numbers says so,
and one that does states the new numbers and why.

## Gate map

Each gate, the packages that close it, and the form its evidence takes. A
gate is closed when the evidence is recorded in the milestone's README, not
when the package merges.

| Gate | Closed by | Evidence form |
| --- | --- | --- |
| [v0.0.2 M4](../v0.0.2/m4/README.md): first native slice | X2-01 to X2-04 | The X2-04 table: three-way agreement, no repeated submission, refusals, the inventory |
| [v0.0.3 M1](../v0.0.3/m1/README.md): the whole language | X3-10 to X3-19, X3-28, X3-29 | Inventory rows `matched` on the stepper and Windows, twice |
| [v0.0.3 M2](../v0.0.3/m2/README.md): three platforms | X3-08, X3-20, X3-21, X3-22 | A linked executable per target; `smoke_<target>_<aspect>` tests; zero pending cells; the baseline; two audits |
| [v0.0.3 M3](../v0.0.3/m3/README.md): build and cutover | X3-23 to X3-27 | Schema-validated build results; identity tests; byte-equal builds; the clean-machine job; an empty Wasm search; the audit record |
| [v0.0.4 M2](../v0.0.4/m2/README.md): semantics and simulation | X4-S1, X4-01 to X4-04 | The approved specification change; kernel tests with their source-level cases; the causal-path query |
| [v0.0.4 M3](../v0.0.4/m3/README.md): native workers | X4-05 to X4-09 | Kernel tests on the Nexus executor; the replay inventory; the ownership and placement tests; the X4-08 table; overlap intervals with the hardware recorded |

## Reporting

A package's handoff records, as the execution model requires: base and tested
commit; each validation line with pass, fail, or not run; the exact commands
and exit statuses; corpus counts and the inventory counts per column;
remaining scope; and PR state. A command not run is recorded as not run. A
dependency download failure is not a passing test.
