# Milestone 4 validation and handoff

Run from the repository root with the pinned `rust-toolchain.toml`, which has
been Rust 1.96.1 since Step 4 (Wasmtime 49 requires 1.96; `rustup` installs the
pin on first use, and that needs the network once). Check every exit status;
PowerShell does not stop on a failing native command by itself. A command not
run is reported as not run, and a dependency-download failure is not a passing
test. These commands replace the
[M3 validation](../milestone-3/validation.md) for work based on `origin/m4`.

## Establish the baseline

```powershell
git status --short --branch
git fetch origin m4
git merge-base --is-ancestor f4bf87b origin/m4
cargo fetch --locked
cargo test --locked --offline --workspace --all-targets --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
```

Branch from the fetched `origin/m4` head only after these pass, and confirm
that the predecessor step's merge is in that head. The baseline at `f4bf87b`
is 414 passed, 0 failed, 0 unavailable: 82 reader, 224 static, 94 interpreter,
and 14 tooling cases. The baseline at `8a04c08`, the head after Step 3, is 431
passed, 0 failed, 0 unavailable: 82 reader, 230 static, 104 interpreter, and 15
tooling cases. From Step 4 the runner also reports the per-backend and parity
counts of the
[differential execution](../../spec/07-diagnostics-and-conformance.md#differential-execution)
rule (two lines, `interpreter backend:` and `wasm backend:`), and later steps
record their own counts. At Step 4 the interpreter backend is 104 passed, 0
failed, 0 unavailable, and the Wasm backend is 8 matched, 0 failed, 96 not
lowered; each step reports how many rows it moved from not lowered to matched.

## Before merging each step

```powershell
$env:RUSTFLAGS = '-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-targets --all-features
$env:RUSTDOCFLAGS = '-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline -p vibra-conformance --test evidence_step11 --test m4_contract_inventory --test m3_contract_inventory --test diagnostic_registry
cargo run --locked --offline -p vibra-conformance --bin m1-fuzz -- --profile ci-smoke
git diff --check
```

From Step 4 add the parity inventory test, the module-bytes determinism test,
and the differential harness test to the list above in each step's handoff:

```powershell
cargo test --locked --offline -p vibra-conformance --test parity_inventory_m4_step4 --test wasm_skeleton_m4_step4 --test differential_m4_step4 --test typed_ir_identity_m4_step4
```

The parity inventory test names the rows to add when a case has none, so the
second of two branches that add executable cases runs it, copies the rows it
prints into `conformance/parity.tsv`, and re-runs it.

Both suites are required: host tests inspect structures, phase order, and
failure paths; the corpus asserts observations through real handlers. Keep
zero failed and zero unavailable, and report counts per profile and per
backend. A `not lowered` count may be nonzero before Step 12 and must be zero
at the Stage 4A sub-gate.

`evidence_step11` keys the specification-example inventory
(`docs/roadmap/milestone-1/syntax-examples.tsv`) by line position, so any
specification edit that moves lines needs a mechanical refresh of that file
from the test's `missing inventory row` output. Review the refreshed rows: a
changed digest or classification is a real change, not noise. The test also
fixes the number of fenced specification examples; a step that adds or removes
one updates that count deliberately.

Check every relative Markdown link and anchor a step adds resolves, including
the anchors of headings it renames.

## Focused checks

| Steps | Command |
| --- | --- |
| 1 | `cargo test --locked --offline -p vibra-diagnostics -p vibra-schema`<br>`cargo test --locked --offline -p vibra-conformance --test m4_contract_inventory --test diagnostic_registry` |
| 2 | `cargo test --locked --offline -p vibra-resolve -p vibra-types -p vibra-ir -p vibra-interp -p vibra-fmt`<br>`cargo test --locked --offline -p vibra-workspace -p vibra-conformance` |
| 3 | `cargo test --locked --offline -p vibra-interp -p vibra-ir -p vibra-cli -p vibra-workspace`<br>`cargo test --locked --offline -p vibra-conformance --test activations_m4_step3 --test tail_calls_step9` |
| 4 | `cargo test --locked --offline -p vibra-wasm -p vibra-wasm-run -p vibra-ir`<br>`cargo test --locked --offline -p vibra-conformance --test parity_inventory_m4_step4 --test wasm_skeleton_m4_step4 --test differential_m4_step4 --test typed_ir_identity_m4_step4 --test architecture_boundary --test corpus_step3 --test activations_m4_step3` |
| 5a–12 | the emitter crate `vibra-wasm` and the runner crate `vibra-wasm-run`, with `cargo test --locked --offline -p vibra-conformance` for the parity inventory and the native harness (`natives_m3_step4b`) |
| 8a–8c | `cargo test --locked --offline -p vibra-types` for the standard-library input and registry, and the registry vectors the step adds |
| 11 | `cargo test --locked --offline -p vibra-cli` for `run` and `test` output |

Add the concrete test file names each step introduces to its handoff.

## Corpus rules

Use the rule prefixes of the conformance chapter: `V1-RUNTIME` for evaluation,
the arena, activations, traps, and parity; `V1-DIAG` for the registry and
profiles; and the type, source, project, and tool prefixes for the forms a step
widens. Author expected snapshots from the specification before running either
backend. Never accept regenerated snapshots wholesale, delete a failing case,
or downgrade a profile to make a gate pass. A case has one expected result and
one expected audit trace for both backends, and a Wasm disagreement is fixed in
the backend, never in the expectation. An availability case whose form a step
implements is replaced by positive and negative cases for that form in the same
PR, not deleted on its own.

## Dependency steps

A step that adds a dependency (Step 4) records, in its handoff, the exact
version and feature set, the output of `cargo fetch --locked` and an offline
build on each CI platform, the licence, and the added build time. The workspace
rejects an unreviewed upgrade: `Cargo.lock` is committed and every command runs
`--locked --offline`. Step 4's record is the README's
[dependency evidence](README.md#dependency-evidence). The CI platforms prove
the build: the `check` job compiles the workspace, Wasmtime included, on Ubuntu,
Windows, and macOS, and the `conformance` job then runs the corpus in both
backends on the same three.

## Step handoff

Record in the PR and the README row:

- base commit, tested head, and verified merge commit;
- the behavior added and the inventory rows it moved;
- the parity inventory rows moved from `not lowered` to `matched`;
- exact commands with exit status;
- corpus counts per profile and per backend, with zero failed and zero
  unavailable;
- new diagnostics, schema, and specification changes; and
- anything left for a later step, with its owning step.
