# Step 30: lint

Kind: vertical. `vibra lint [TARGET] [--write]`: the check and one plan
holding every safe fix of its diagnostics, applied with `--write` when it
spans one document.
Depends on: 28.
Crates: `vibra-workspace`, `vibra-cli`, `vibra-schema`, `conformance/cases`.

## Prerequisites

- Read: [05 Command surface](../../../../spec/05-tooling.md#command-surface)
  (`lint`, the coverage of `check` and `lint`); [05 Command payloads](../../../../spec/05-tooling.md#command-payloads) (`lint`);
  [questions](../spec-questions.md) Q10, Q15, Q21, Q22.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-cli/src/check.rs:146` | `pub fn run` | the target coverage `lint` shares |
| `crates/vibra-cli/src/check.rs:52` | `pub fn outcome` | the verdict `accepted` follows |

From 28: `applicable_fixes`; from 07: the builder; from 15: `apply`.
Proposed: `edit::lint::plan_lint`; `vibra-cli::lint::run`;
`vibra-schema::lint_payload::LintPayload`.

## Files owned

- `crates/vibra-workspace/src/edit/lint.rs`
- `crates/vibra-workspace/tests/it/lint.rs`
- `crates/vibra-cli/src/lint.rs`
- `crates/vibra-cli/tests/it/lint.rs`
- `crates/vibra-schema/src/lint_payload.rs`
- `crates/vibra-schema/schemas/v2/payloads/lint.json`
- `crates/vibra-schema/tests/it/lint_payload.rs`
- `conformance/rules/m3-30.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `plan_lint(capture, target)`: the check over the targets `check` would
   cover; every diagnostic with a `@safe` fix becomes an edit of one plan,
   sorted; two fixes on one span keep the first and report the second in
   `resultingDiagnostics`; `null` with no fix.
2. The payload `{accepted, plan}`; `accepted` the check's verdict; result
   as `check`'s, with the plan beside it; a protected module's fix is left
   out of the plan and reported `@tool.actionDenied`; the exhausted form
   is `{accepted, plan: null}`.
3. `--write` applies the plan through `apply` when it spans one document;
   a plan over several documents is `@tool.unavailable` with the preview
   returned (Q15).

## Test matrix

Cases are `workspaceCheck` trees run by the CLI tests.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-lint-clean` | positive | a project with no diagnostic | `@command.ok`, `accepted: true`, `plan: null` | Q22 |
| `V2-TOOL-lint-two-fixes` | positive | a duplicate root in `src/` and a missing import in `tests/` | `accepted: false`, one plan with two edits over two documents, `@command.diagnostics` | Q22 |
| `V2-TOOL-lint-target` | positive | `lint @app` with the missing import in `tests/` | the plan omits `tests/`; `@tests` is not covered | 05: "`check @app` covers that target only, not `@tests`" |
| `V2-TOOL-lint-unsafe-left` | positive | a `@type.mismatch` and a duplicate root | the plan has one edit; the mismatch stays in the envelope | Q21 |
| `V2-TOOL-lint-protected` | negative | the missing import under an approval in force | the plan omits it; `@tool.actionDenied` in the envelope | 05: "No fix … may alter a protected … test" |
| `ATK-lint-write` | positive | `lint --write` on Windows with one fix in one document | the document written through `apply`; a following `lint` is clean | 05 |
| `ATK-lint-write-several-unavailable` | negative | `lint --write` on Windows with fixes in two documents | `@tool.unavailable`; the plan returned; nothing written | 05: "A backend that cannot write several files atomically MUST refuse before writing"; Q15 |
| JSON validates | positive | every envelope | validates | 05 |

## Diagnostics and schema changes

Codes: `@tool.actionDenied`, `@tool.unavailable`. Schema: the `lint`
payload (new).

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; `help` lists `lint` as supported.

## Stays unavailable

`lint --write` over several documents. After this step every command of
the surface but `project *`, `build`, and `mcp` is available.

## Excluded

New fixes.

## Completion evidence

- [ ] `conformance/rules/m3-30.tsv` has one `active` row per case id.
- [ ] The validation commands exit 0.
