# Step 11: edit-replace

Kind: vertical. `vibra edit replace SUBJECT DATA`: the preview of the
repair edit of Q1. The replacement is parsed, the subject's span replaced,
missing imports inserted, the staged snapshot checked and formatted, the
identity changes reported; a protected document or a wrong subject is
refused. Applying is step 14.
Depends on: 03, 04, 07.
Crates: `vibra-workspace`, `vibra-cli`, `vibra-conformance`,
`conformance/cases`.

## Prerequisites

- Read: [05 Transactional edit plans](../../../../spec/05-tooling.md#transactional-edit-plans);
  [05 Command surface](../../../../spec/05-tooling.md#command-surface) (`DATA`);
  [01 Declarations](../../../../spec/01-source-language.md#declarations);
  [architecture](../architecture.md) section 3, rows I10, I11, I13;
  [questions](../spec-questions.md) Q1, Q10, Q17, Q19, Q31.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/snapshot.rs:272` | `pub fn with_module_bytes` | staging a replacement in memory |
| `crates/vibra-workspace/src/semantic.rs:80` | `pub fn check_snapshot` | the re-check of the staged snapshot |
| `crates/vibra-workspace/src/format_plan.rs:381` | `pub fn plan_format` | the formatting of the staged module |
| `crates/vibra-syntax/src/document.rs:200` | `pub fn parse_source` | parsing `DATA` as a module fragment |
| `crates/vibra-syntax/src/source/section.rs:46` | `pub struct Construct` | the subject's span and keyword |
| `crates/vibra-resolve/src/snapshot.rs:54` | `pub struct ResolvedDeclaration` | the subject's declaration and span |

From steps 03, 04, 07, 09: `identity::identities`, `edit::plan::PlanBuilder`,
`approval::protects`. Proposed: `edit::replace::plan_replace`;
`vibra-cli::edit::run` (the `replace` arm);
`vibra-conformance::edit::EditHandler`.

## Files owned

- `crates/vibra-workspace/src/edit/replace.rs`
- `crates/vibra-workspace/tests/it/edit_replace.rs`
- `crates/vibra-cli/src/edit.rs`
- `crates/vibra-cli/tests/it/edit_replace.rs`
- `crates/vibra-conformance/src/edit.rs`
- `conformance/rules/m3-11.tsv`
- the case directories of the matrix below

## Ordered tasks

1. Subject: a top-level declaration or a `fn` or `proc` member of a module
   of the root project's targets or `@tests`; anything else, a `@path`
   dependency, `@std`, a case, a field, a `use`, is invalid input (I10).
2. `DATA`: one VIBON `Str`; its value parsed with `parse_source`; it MUST
   hold zero or more `use` declarations and then exactly one declaration
   of the subject's keyword form and name, else invalid input; a parse
   error inside is `@command.diagnostics` with that error at the data's
   offset mapped into a source identity `<data>`.
3. The plan: one edit replacing the subject's whole span with the new
   declaration laid out by the formatter at the subject's indentation;
   one edit per `use` the module lacks, inserted into the import block in
   sorted position; `documents` with the module's revision. Then the
   staged snapshot is checked; `resultingDiagnostics` holds the
   diagnostics of the affected modules; `postconditions` the identity
   changes (Q19), computed by recomputing every identity of the local
   package on the staged snapshot.
4. A protected document under a current approval is `@tool.actionDenied`
   (Q10), result `@command.diagnostics`, plan returned, `applied: false`.
5. The `edit` payload `{plan, applied: false, resultingRevision: null}`;
   the human format prints each edit as a unified diff of the document.
6. `EditHandler` for `kind = "replace"`: `data` is a file holding the
   `Str` literal; `expect.editPlan` compared as parsed JSON after
   validation, `workspaceRevision` and `documents[].revision` rewritten by
   `revisions.ps1`.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| the replacement keeps the subject's form and name | `fn`; `proc`; `type`; member `fn`; `test`; against: another name, another keyword, two declarations, no declaration |
| imports | a needed `use` absent; a `use` already present; against: a `use` of an unknown path (a resulting diagnostic) |

## Test matrix

Cases are `edit` under `toolingV2` with `kind = "replace"`. `T` is a tree
with `src/app/math.vib` holding `fn double takes {n: I32} returns twice:
I32 do return I32.add(n n) endfn` and a test module.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-edit-replace-body` | positive | `@app.math.double` with a body returning `I32.mul(n 2i32)` | one edit over the declaration's span; `resultingDiagnostics: []`; `postconditions` one `@witness` entry for `double` and one per dependent; `applied: false` | Q1 |
| `V2-TOOL-edit-replace-postconditions-witness-only` | positive | the same | no `@nominal` and no `@claim` entry | 05: "Editing a body does not change it" |
| `V2-TOOL-edit-replace-signature-changes-claim` | positive | `returns twice: I64` | a `@claim` entry for `double`, a `@claim` entry for each caller whose signature names nothing (none), `resultingDiagnostics` with the callers' `@type.mismatch` | 05: "A report says which category changed" |
| `V2-TOOL-edit-replace-adds-import` | positive | a body using `Str.length` with `use @std.str enduse` in `DATA` and absent from the module | two edits, the `use` inserted in sorted position | Q1 |
| `V2-TOOL-edit-replace-import-present` | positive | the same `use` already in the module | one edit | Q1 |
| `V2-TOOL-edit-replace-other-name` | negative | `DATA` declares `fn triple` | `@command.invalidInput` | Q1: "a rename is `edit rename`" |
| `V2-TOOL-edit-replace-other-form` | negative | `DATA` declares `proc double` | `@command.invalidInput` | Q1 |
| `V2-TOOL-edit-replace-two-declarations` | negative | `DATA` with two `fn`s | `@command.invalidInput` | Q1: "exactly one declaration" |
| `V2-TOOL-edit-replace-parse-error` | negative | `DATA` missing `endfn` | `@command.diagnostics` with `@syntax.unmatchedDelimiter`, plan `null` | 07 Syntax diagnostics |
| `V2-TOOL-edit-replace-resulting-error` | negative | a body with a type error | the plan with `resultingDiagnostics` holding the `@type.mismatch`; result `@command.diagnostics` | 05: "A plan that is … ill-typed … fails with nothing written" |
| `V2-TOOL-edit-replace-member` | positive | `@app.model.User.nameLength` | one edit over the member's span, indented as the member | Q1 |
| `V2-TOOL-edit-replace-test-protected` | negative | `@tests.math::"adds"` is no entity atom: the test declaration named by its module and name is not a subject | `@command.invalidInput` | 05 Command surface: `SUBJECT` is a query subject; a test has no entity atom |
| `V2-TOOL-edit-replace-test-denied` | negative | a helper `fn` of `tests/math.vib` under a current approval | `@tool.actionDenied`, `@command.diagnostics`, plan returned, `applied: false` | 05: "No fix or format may alter a protected claim, test, or policy outside its approval workflow" |
| `V2-TOOL-edit-replace-under-approval-allowed` | positive | `@app.math.double` under a current approval | the plan, no denial | 05: "pinning every body would make repair impossible" |
| `V2-TOOL-edit-replace-entry-claim-denied` | negative | the entry's `performs` row changed under a current approval | `@tool.actionDenied` | 05: "a protected claim" |
| `V2-TOOL-edit-dependency-subject-invalid` | negative | a declaration of a `@path` dependency | `@command.invalidInput` | 05: "Edits reach local modules and never vendored dependencies"; Q17 |
| `V2-TOOL-edit-std-subject-invalid` | negative | `@std.str.length` | `@command.invalidInput` | the same |
| `V2-TOOL-edit-replace-field-invalid` | negative | a record field as subject | `@command.invalidInput` | Q1 |
| `V2-TOOL-edit-replace-data-not-string` | negative | `DATA` is `1i32` | `@command.invalidInput` | 05 Command surface: `DATA` is "one canonical VIBON value" of the stated type |
| `V2-TOOL-edit-replace-formatted` | positive | `DATA` on one line | the edit's replacement is the formatter's layout | 05: "format, without reordering operands" |
| `ATK-edit-replace-other-name` | negative | the host test over `plan_replace` with a renamed declaration | `Planned::InvalidInput` | Q1 |
| `ATK-edit-replace-test-protected` | negative | the host test: a protected module reaches `PlanBuilder` with the protection check removed | the test fails; with the check it is `Planned::Denied` | architecture, I13 |
| JSON validates | positive | every envelope | validates | 05 |

## Diagnostics and schema changes

Codes: `@tool.actionDenied` (Q10), the syntax and type codes of the staged
check. Schema: the `edit` payload (07), produced here.

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

Read: all exit 0; `help` lists `edit replace` as supported with `["edit"]`.

## Stays unavailable

`--write` on every edit (14); `edit rename`, `edit imports`, `edit fix`,
`lint`.

## Excluded

Applying; renames; imports; fixes.

## Completion evidence

- [ ] `conformance/rules/m3-11.tsv` has one `active` row per case id.
- [ ] Every row of "Positions" has its case; the report maps each.
- [ ] `ATK-edit-replace-test-protected` was seen to fail with the check
      removed; the report says how.
- [ ] The validation commands exit 0.
