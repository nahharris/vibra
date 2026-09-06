# Milestone 2 validation and handoff

Run from the repository root using the pinned `rust-toolchain.toml`. Check each
exit status before continuing; PowerShell does not stop on every native command
failure automatically. Commands below are present at the M1 base unless marked
as introduced by an M2 step. Do not count a command with zero matched tests as
focused validation.

## Establish the baseline

```powershell
git status --short --branch
git rev-parse HEAD
git fetch origin m2
git merge-base --is-ancestor 9c77b8642e1a7f3f4d8aab1eb0cd709bbbefebf6 origin/m2
cargo fetch --locked
cargo test --locked --offline --workspace --all-targets --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
```

Git/Cargo fetches prepare exact inputs; validation then runs offline. A missing
toolchain/cache is a blocked command, not a test pass or reason to edit the lock.
Branch only after the fetch succeeds. The ancestor check verifies M1 provenance;
also check that the chosen predecessor PR is present at the fetched M2 head.

## Before merging each step

This mirrors the existing CI checks, with the corpus expanded by M2 handlers.

```powershell
$env:RUSTFLAGS = '-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-targets --all-features
$env:RUSTDOCFLAGS = '-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline -p vibra-conformance --test evidence_step11
cargo run --locked --offline -p vibra-conformance --bin m1-fuzz -- --profile ci-smoke
git diff --check
```

Both independent suites are required. Host tests inspect structures, ownership,
phase ordering, error paths and instrumentation. The corpus asserts language
observations through real handlers; `cargo test` does not substitute for it.
Keep zero failed and zero unavailable in the committed gate corpus. Report
counts separately by profile after static/interpreter cases arrive; keep the
existing reader cases and their closest-capable-handler dispatch intact.

Do not introduce a `static-v1` or `interpreter-v1` case before its real handler
can produce the promised observation. Use synthetic host tests for Step 1's
infrastructure; merge behavior and its independently authored executable cases
together thereafter. An availability-diagnostic case is a negative admission
test, not evidence that the unavailable program ran. An unavailable execution
request remains unavailable and fails the gate.

## Focused checks as crates arrive

Run only commands for crates introduced by the selected or preceding step:

| First available | Command |
| --- | --- |
| Step 1 | `cargo test --locked --offline -p vibra-conformance -p vibra-schema -p vibra-diagnostics` |
| Step 2 | `cargo test --locked --offline -p vibra-workspace` |
| Step 4 | `cargo test --locked --offline -p vibra-resolve -p vibra-workspace` |
| Step 5 | `cargo test --locked --offline -p vibra-types -p vibra-ir -p vibra-interp` |
| Step 6 | `cargo test --locked --offline -p vibra-types -p vibra-ir -p vibra-interp`<br>`cargo test --locked --offline -p vibra-conformance --test bindings_step6` |
| Step 10 | `cargo test --locked --offline -p vibra-workspace -p vibra-schema` |
| Step 11 | `cargo test --locked --offline -p vibra-cli -p vibra-workspace -p vibra-fmt` |

Append an actual test filter only after confirming its name. Each step records
the concrete test files and case IDs it added. Proposed module/test names in a
guide must be replaced with real paths when the slice lands.

## Corpus oracle and recovery checklist

Start with `case.toml` examples in `conformance/cases/` and the current manifest
schema. Use existing rule prefixes: `V1-PROJECT`, `V1-TYPE-NAMES`,
`V1-TYPE-INFER`, `V1-TYPE-CONTROL`, `V1-SRC-EXPR`, `V1-SRC-FMT`,
`V1-RUNTIME`, `V1-TOOL`, and `V1-DIAG` as appropriate. A prose heading is not a
new manifest rule prefix. Directory and manifest IDs must match.

For every claimed behavior include:

- a positive case with independently calculated identity/type/value;
- a nearby invalid case with exact code, fixed level, source ID and byte span;
- a recovery case retaining useful following declarations and fact status;
- a boundary case (Unicode, numeric range, arity, path, scope or depth); and
- formatter equivalence/idempotence where formatting is affected, or an
  explicit reason existing syntax formatting is sufficient.

Semantic rejection of structurally valid source does not imply byte-preserving
recovery formatting: distinguish syntax recovery from type/availability errors.
For truly recovered syntax, require original bytes. For accepted formatting,
assert reparse/check/value equivalence, attached comments and width boundaries.
Never accept new snapshots wholesale to make a regression disappear.

Step 1 must extend the actual multi-file diagnostic/operation contract before
fixtures rely on it. Two documents may have identical byte offsets; do not
flatten them into a concatenated fake source. Omitted optional expectations mean
not asserted; an explicitly empty event trace must assert no events occurred.

## CLI demo commands

The binary and argument grammar do not exist at this planning baseline. Step 1
freezes the grammar; Steps 11–13 must add the exact verified init/fmt/check/run/
test invocations here and a repeatable temporary-project driver. Do not publish
an invented `--project`, target flag, or JSON stream contract in advance.

The build command after Step 11 is:

```powershell
cargo build --locked --offline -p vibra-cli --bin vibra
```

Use `target/debug/vibra.exe` on Windows and `target/debug/vibra` on Unix from
the checkout's absolute path when changing into the demo directory. The driver
must preserve unrelated directories, reject conflicts, capture actual process
exits/stdout/stderr, and fail on a mismatched outcome. It must exercise the
binary, not merely call workspace functions.

Prepare Rust dependencies and the reviewed stdlib bootstrap once, then run the
clean-checkout demo offline. Verify no `sync`/network fallback occurs. Include
multi-module import, pure tail recursion, a passing suite, an intentional
assertion failure, and type/availability rejection. External watchdogs are test
infrastructure, not language budgets or program results.

## Step handoff

```text
Step number / claimed subset:
Fetched integration base / tested head:
Predecessor PR and verified merge:
Specification clauses / closed C1-C12 decisions:
Changed files and public contracts:
Host commands / exits / actual test counts:
Corpus command / passed / failed / unavailable by profile:
Positive / negative / recovery / boundary case IDs:
Formatter / schema / no-execution-on-error evidence:
Runtime values / audit trace / tail depth where applicable:
Excluded later behavior / remaining work:
CI URL and exact checked head:
Step PR base=m2 / state / actual merge commit after merge:
```

Review the complete diff against refreshed `origin/m2`. Verify the head again
after any fix and run affected checks on that head. No claim of merge based
only on a push, open PR, or green local tests. The next implementer must be able
to reproduce the evidence without this conversation.

A planning-only refinement checks links/anchors, file/API references, table
coverage, code fences and whitespace. Its inherited M1 checks do not count as
any M2 implementation or gate. CI evidence remains separate from local evidence.
