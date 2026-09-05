# Milestone 1 validation and handoff

Run commands from the repository root with the pinned `rust-toolchain.toml`.
These commands exist at baseline `50aa40a`; proposed future tests and fuzz
targets must be added before anyone records them as executed.

## Baseline and focused iteration

```powershell
git status --short
git rev-parse HEAD
cargo fetch --locked
cargo test --locked --offline -p vibra-syntax -p vibra-fmt
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
```

Fetch is the dependency preparation step; validation then runs offline. If the
toolchain or dependency cache is unavailable, record the blocked command and
cause rather than changing the lockfile or dropping offline validation.
For a focused named host test, append its actual name to `cargo test`; verify
that the output ran the intended test instead of accepting zero matched tests.

## Before a step is ready to merge

The following mirrors the current CI checks. Set `RUSTFLAGS=-D warnings` for
build/test commands and `RUSTDOCFLAGS=-D warnings` for documentation, as CI does.
PowerShell assignments are shown here; other shells use their native syntax.
Check each command's exit status before proceeding.

```powershell
$env:RUSTFLAGS = '-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-targets --all-features
$env:RUSTDOCFLAGS = '-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
git diff --check
```

The corpus must be nonempty and report zero failed and zero unavailable cases.
`cargo test` alone is not the corpus gate. Current CI runs host checks on Linux,
Windows, and macOS and the independent reader corpus on Linux. Record local
platform evidence separately from CI platform evidence.

## Writing an oracle

Start from an existing `case.toml` under `conformance/cases/`. Use its actual
schema: `rule`, `profile`, `[inputs]`, `[expect]`, and ordered
`[[expect.diagnostics]]` with `[expect.diagnostics.span]` start/end byte offsets.
The directory and manifest IDs must match. Use a registered rule prefix plus a
descriptive kebab-case suffix. Prefer one input document per diagnostic case
so offsets are unambiguous.

Author acceptance, diagnostics, and formatted bytes from the specification.
Review any generated snapshot manually. Assert code, level, and half-open span;
do not couple to incidental message punctuation. Include a Unicode prefix in
span tests. Omitted optional expectations mean not asserted, not empty output.
If adding new observation fields, prove the runner fails on a deliberately
wrong expectation and that the real handler produces the observed value.

For every accepted formatter case, assert successful reparse, structural/value
equivalence at the implemented layer, and `format(format(input)) == format(input)`.
Idempotence alone could pass after silently deleting content. For recovered
input, assert `format(input) == input` byte-for-byte. Test comments and 88-column
boundaries at multiple nesting depths, including a closing delimiter's column.

## Handoff record

Include the following in the step PR and summarize it in the step plan:

```text
Step and claimed behavior:
Base commit / tested head:
Specification clauses and case IDs:
Host commands / exit results / test counts:
Corpus command / passed / failed / unavailable:
Formatter equivalence and recovery evidence:
Diagnostic and schema changes (or why none):
Deferred semantics and unresolved work:
CI URL and head checked:
PR URL / actual merge commit (only after merge):
```

A documentation-only refinement checks links, anchors, file/API references,
diff whitespace, and consistency with the normative gate. It does not claim a
language step or fuzz gate completed. Existing CI may still run all checks.
