# Step 20: edit-imports

Kind: vertical. `vibra edit imports PATH [--write]`: the `use`
declarations of one module sorted by path, `as` kept, comments kept with
their `use`, resolution unchanged.
Depends on: 14.
Crates: `vibra-workspace`, `vibra-cli`, `conformance/cases`.

## Prerequisites

- Read: [05 Transactional edit plans](../../../../spec/05-tooling.md#transactional-edit-plans)
  ("Organizing imports changes neither resolution nor operand order");
  [01 `use`](../../../../spec/01-source-language.md#use);
  [01 Canonical format](../../../../spec/01-source-language.md#canonical-format);
  [questions](../spec-questions.md) Q10, Q20.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-syntax/src/source/mod.rs:31` | `pub use declaration::{Atom, Declaration, Ident, Use}` | the typed view of a `use` |
| `crates/vibra-resolve/src/snapshot.rs:108` | `pub struct ResolvedImport` | the resolution the plan must keep |
| `crates/vibra-workspace/src/format_plan.rs:285` | `pub fn format_mode` | the path check of a module |

Proposed: `edit::imports::plan_imports`.

## Files owned

- `crates/vibra-workspace/src/edit/imports.rs`
- `crates/vibra-workspace/tests/it/edit_imports.rs`
- `crates/vibra-cli/tests/it/edit_imports.rs`
- `conformance/rules/m3-20.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `PATH`: a module of the root project's units, else invalid input.
2. The plan: the import block (the leading `use` declarations, each with
   the comment lines right before it) reordered by the UTF-8 bytes of the
   paths; one edit over the block; an already sorted block gives an empty
   plan, `changed` false in the human format and `edits: []`.
3. The staged check must give the same `imports` and `references` as the
   capture's, asserted by the step; a protected module under an approval
   is `@tool.actionDenied`.
4. `--write` through `apply`.

## Test matrix

Cases are `edit` with `kind = "imports"`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-edit-imports-sorted` | positive | `use @std.str enduse`, `use @app.model enduse`, `use @std.core.Ordering enduse` | one edit; the block in the order `@app.model`, `@std.core.Ordering`, `@std.str` | Q20 |
| `V2-TOOL-edit-imports-keeps-as` | positive | `use @billing.cents as money enduse` | the `as` kept | 05: "The formatter and the import organizer keep a written `as`" |
| `V2-TOOL-edit-imports-keeps-comment` | positive | a comment line before the second `use` | the comment moves with its `use` | Q20 |
| `V2-TOOL-edit-imports-already-sorted` | positive | a sorted block | `edits: []`; `@command.ok` | Q20 |
| `V2-TOOL-edit-imports-resolution-unchanged` | positive | the first row's tree | `resultingDiagnostics: []`; the host test compares the resolution's imports and references before and after | 05: "Organizing imports changes neither resolution nor operand order" |
| `V2-TOOL-edit-imports-removes-nothing` | positive | an unused `use` | kept | Q20 |
| `V2-TOOL-edit-imports-not-a-module` | negative | `project.vibon` | `@command.invalidInput` | 05 Command surface: `PATH` |
| `V2-TOOL-edit-imports-dependency` | negative | a module under `dep/` | `@command.invalidInput` | 05: "never vendored dependencies" |
| `V2-TOOL-edit-imports-protected` | negative | `tests/main.vib` under a current approval | `@tool.actionDenied` | 05: "a protected … test" |
| `ATK-edit-imports-apply` | positive | `--write` on Windows | written through `apply` | 14 |
| JSON validates | positive | every envelope | validates | 05 |

## Diagnostics and schema changes

Codes: `@tool.actionDenied`. Schema: none new.

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

Read: all exit 0; `help` lists `edit imports` as supported.

## Stays unavailable

Nothing new.

## Excluded

Adding or removing imports (the replace edit adds; nothing removes).

## Completion evidence

- [ ] `conformance/rules/m3-20.tsv` has one `active` row per case id.
- [ ] The validation commands exit 0.
