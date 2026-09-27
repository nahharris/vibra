# Milestone 3 validation and handoff

Run from the repository root with the pinned `rust-toolchain.toml`. Check every
exit status; PowerShell does not stop on a failing native command by itself. A
command not run is reported as not run, and a dependency-download failure is
not a passing test.

## Establish the baseline

```powershell
git status --short --branch
git fetch origin m3
git merge-base --is-ancestor f25bc43e3af142b9a0391fb2748baae7747d2c7d origin/m3
cargo fetch --locked
cargo test --locked --offline --workspace --all-targets --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
```

Branch from the fetched `origin/m3` head only after these pass, and confirm that
the predecessor step's merge is in that head. The M3 baseline is 205 passed,
0 failed, 0 unavailable; later steps record their own counts.

## Before merging each step

```powershell
$env:RUSTFLAGS = '-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-targets --all-features
$env:RUSTDOCFLAGS = '-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline -p vibra-conformance --test evidence_step11
cargo test --locked --offline -p vibra-conformance --test m3_contract_inventory
cargo test --locked --offline -p vibra-conformance --test diagnostic_registry
cargo run --locked --offline -p vibra-conformance --bin m1-fuzz -- --profile ci-smoke
git diff --check
```

Both suites are required: host tests inspect structures, phase order, and
failure paths; the corpus asserts observations through real handlers. Keep
zero failed and zero unavailable, and report counts per profile.

`evidence_step11` keys the specification-example inventory
(`docs/roadmap/milestone-1/syntax-examples.tsv`) by line position, so any
specification edit that moves lines needs a mechanical refresh of that file
from the test's `missing inventory row` output. Review the refreshed rows: a
changed digest or classification is a real change, not noise. The test also
fixes the number of fenced specification examples; a step that adds or removes
one updates that count deliberately.

## Focused checks

| Steps | Command |
| --- | --- |
| 1 | `cargo test --locked --offline -p vibra-diagnostics -p vibra-schema`<br>`cargo test --locked --offline -p vibra-conformance --test m3_contract_inventory --test diagnostic_registry` |
| 2–8 | `cargo test --locked --offline -p vibra-resolve -p vibra-types -p vibra-ir -p vibra-interp -p vibra-fmt`<br>`cargo test --locked --offline -p vibra-workspace -p vibra-conformance` |
| 4, 8 | `cargo test --locked --offline -p vibra-types` for the standard-library input and registry |
| 8–9 | `cargo test --locked --offline -p vibra-cli` for `run` and `test` output |

Add the concrete test file names each step introduces to its handoff.

## Corpus rules

Use the rule prefixes of the conformance chapter: `V1-TYPE-NOMINAL`,
`V1-TYPE-GENERIC`, `V1-TYPE-INFER`, `V1-TYPE-CONTROL`, `V1-TYPE-CONVERT`,
`V1-SRC-EXPR`, `V1-SRC-CALLS`, `V1-SRC-DECL`, `V1-RUNTIME`, and `V1-DIAG`.
Author expected snapshots from the specification before running the
implementation. Never accept regenerated snapshots wholesale, delete a failing
case, or downgrade a profile to make a gate pass. An M2 availability case whose
form a step implements is replaced by positive and negative cases for that
form in the same PR, not deleted on its own.

## Step handoff

Record in the PR and the README row:

- base commit, tested head, and verified merge commit;
- the behavior added and the inventory rows it moved;
- exact commands with exit status;
- corpus counts per profile, with zero failed and zero unavailable;
- new diagnostics, schema, and specification changes; and
- anything left for a later step, with its owning step.
