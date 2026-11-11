# v0.0.2 M1 validation and handoff

Status: proposed validation policy. Commands below name current workspace
entry points; new focused store tests are added once Step 1 freezes their
placement and contracts. No test result is recorded by writing this document.

Run from the repository root with `rust-toolchain.toml`. Inspect each native
command's `$LASTEXITCODE` immediately in PowerShell; a later successful command
does not clear an earlier failure. Restore any caller environment settings
after validation. Fetch dependencies only when needed; offline failure is
neither a passing test nor evidence that the implementation is broken.

## Planning documents

For this planning-only change, review scope and the gate mapping, validate
local Markdown link targets/anchors, and run:

```powershell
git diff --check
git status --short
```

No Rust suite is needed for a draft that changes only planning prose.
Specification, registry, schema, or implementation changes use the applicable
checks below, with both independent suites required for milestone completion.

## Establish an execution baseline

Before creating `v0.0.2/m1`, refresh `origin/main`, verify the preceding
milestone merge and exit evidence, and preserve unrelated work. Record the
exact base and toolchain. When dependencies are not already available, run
`cargo fetch --locked` before offline checks.

```powershell
git status --short --branch
git rev-parse HEAD
cargo test --locked --offline --workspace --all-targets --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
```

Record corpus passed/failed/unavailable counts per profile and the
interpreter/Wasm parity counts. No historical number is a current baseline.

## Before merging a behavior step

Run the step's new focused host tests and reviewed independent corpus cases
first. Confirm that they fail for the missing behavior before implementing
it. Then run affected neighboring tests and the existing CI checks:

```powershell
$env:RUSTFLAGS = '-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-targets --all-features
$env:RUSTDOCFLAGS = '-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo run --locked --offline -p vibra-conformance --bin m1-fuzz -- --profile ci-smoke
git diff --check
```

The `m1-fuzz` executable belongs to the original reader milestone; its name
does not make it codebase fuzz coverage. Add store-specific malformed-record
and fault-injection coverage through the approved M1 test design.

Both suites must pass with zero failed and zero unavailable cases. Existing
executable cases retain interpreter/Wasm parity and no newly unlowered rows.
All successful and failed M1 JSON payloads validate against their schemas.
Do not accept changed snapshots without checking their normative rule.

## Handoff

Record the base/tested commit, actual commands and exit codes, corpus and
backend counts, new rule/case/diagnostic/schema coverage, operating system
and filesystem, step PR and merge, and unresolved scope. For measurements,
record workload and repetitions as well as latency and size. Keep supported
commands and local filesystem claims consistent with exercised coverage.

At Step 10, attach evidence for every row of the
[gate mapping](execution.md#gate-coverage), including reopen, two-publisher
race, interruption, export conflict, corruption/version rejection, index
rebuild, and independent restore. Green CI must refer to the final PR head.
