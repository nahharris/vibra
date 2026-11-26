# Step 28: edit-fix

Kind: vertical. `vibra edit fix SUBJECT [--write]`: exactly one applicable
safe fix at the subject, the first two codes whose fix capability becomes
`@safe`, the `fixes` array of a diagnostic, and the plan that applies it.
Depends on: 15.
Crates: `vibra-diagnostics`, `vibra-workspace`, `vibra-cli`,
`conformance/cases`.

## Prerequisites

- Read: [07 Diagnostics are a language surface](../../../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface)
  (fix capability); [07 Diagnostic documents and spans](../../../../spec/07-diagnostics-and-conformance.md#diagnostic-documents-and-spans)
  (`fixes`); [05 Command surface](../../../../spec/05-tooling.md#command-surface)
  ("`edit fix` needs exactly one applicable safe fix");
  [03 Effect ceilings](../../../../spec/03-effects.md#effect-ceilings);
  [04 Tests](../../../../spec/04-programs-and-packages.md#tests);
  [questions](../spec-questions.md) Q10, Q21.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-diagnostics/src/registry.rs:216` | `pub const fn fix_capability` | the two codes become `@safe` |
| `crates/vibra-diagnostics/src/diagnostic.rs:86` | `pub struct Fix` | the fix a diagnostic carries |
| `crates/vibra-diagnostics/src/diagnostic.rs:242` | `pub fn with_fix` | attaching it |
| `crates/vibra-diagnostics/src/diagnostic.rs:386` | `fn no_code_is_registered_as_fixable` | the test that becomes the list of the two codes |
| `crates/vibra-workspace/src/semantic.rs:80` | `pub fn check_snapshot` | where both codes reach the engine, which attaches the fixes by code and span |

Proposed: `edit::fix::{plan_fix, applicable_fixes}`, filled into the plan
table of step 01; `vibra-cli::edit::fix::run`.

## Files owned

- `crates/vibra-diagnostics/src/registry.rs`
- `crates/vibra-diagnostics/src/diagnostic.rs`
- `crates/vibra-workspace/src/edit/fix.rs`
- `crates/vibra-workspace/tests/it/edit_fix.rs`
- `crates/vibra-cli/src/edit/fix.rs`
- `crates/vibra-cli/tests/it/edit_fix.rs`
- `conformance/rules/m3-28.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `@effect.duplicateRoot` and `@module.missingRequiredImport` gain fix
   capability `@safe`; every other code stays `@none`; the registry test
   lists exactly the two. The diagnostics of those codes carry one
   `Fix::safe(description, document revision)` each; attaching the fix is
   done in the engine from the diagnostic's code and span, so no checker
   file changes. Invariant: a `fixes` entry appears only for a `@safe` code.
2. `applicable_fixes(capture, subject)`: the diagnostics of the check
   whose primary span contains the position, or lies in the entity, and
   whose code is `@safe`; exactly one is required, else invalid input (no
   fix, or several).
3. The two fixes as plans: remove the later entry of a row or `effects`
   list (and the separating whitespace), the set unchanged; insert `use
   @std.assert enduse` into the test module's import block in sorted
   position. A fix of the `effects` list of a target edits `project.vibon`,
   which is no source document: it is a plan the publisher refuses as a
   destination (Q17) and stays a preview, and under an approval in force it
   changes the entry's claim and is `@tool.actionDenied`.
4. A protected module is `@tool.actionDenied`; `--write` through `apply`.

## Test matrix

Cases are `edit` with `kind = "fix"`; `workspaceCheck` where stated.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-fix-capability-duplicate-root` | positive, `workspaceCheck` | `performs {DirReadText DirReadText}` | the diagnostic has one fix, `safe: true`, `expectedRevision` the document's | 07: "Only `@safe` permits an automatic fix" |
| `V2-TOOL-fix-capability-missing-import` | positive, `workspaceCheck` | a test module without `@std.assert` | one safe fix | Q21 |
| `V2-TOOL-fix-capability-none` | positive, `workspaceCheck` | a `@type.mismatch` | `fixes: []` | 07: "empty when the code's fix capability is `@none`" |
| `V2-TOOL-edit-fix-duplicate-root` | positive | the position of the second `DirReadText` | one edit removing it and its separator; `resultingDiagnostics: []`; `postconditions: []` | 03: "A row that names one interface twice, under any spelling, is `@effect.duplicateRoot`" |
| `V2-TOOL-edit-fix-duplicate-root-alias` | positive | the repeat under another spelling (an `as` import) | the later entry removed | the same, "under any spelling" |
| `V2-TOOL-edit-fix-effects-list` | positive | a repeat in a target's `effects` list; subject the position in `project.vibon` | the plan over `project.vibon` as a preview; `--write` is refused, since the project file is no source document | 07: "a row, or a target's `effects` list"; Q17 |
| `V2-TOOL-edit-fix-missing-import` | positive | the test module's first test name | one edit inserting `use @std.assert enduse` in sorted position; `resultingDiagnostics: []` | 04 Tests: "MUST contain an import that resolves to the module `@std.assert`" |
| `V2-TOOL-edit-fix-none-applicable` | negative | a position with no diagnostic | `@command.invalidInput` | 05: "`edit fix` needs exactly one applicable safe fix" |
| `V2-TOOL-edit-fix-not-safe` | negative | a position with a `@type.mismatch` only | `@command.invalidInput` | the same |
| `V2-TOOL-edit-fix-two-applicable` | negative | an entity with two duplicate roots | `@command.invalidInput` | the same, "exactly one" |
| `V2-TOOL-edit-fix-protected` | negative | the missing import in `tests/` under an approval in force | `@tool.actionDenied` | 05: "a protected … test" |
| `V2-TOOL-edit-fix-no-claim-change` | positive | the duplicate-root fix | no `@claim` postcondition: the row's set is unchanged | Q21 |
| `ATK-edit-fix-apply` | positive | `--write` on Windows | written through `apply`; a following `check` has no diagnostic | 07: "fixes respect revisions" |
| `ATK-edit-fix-stale-fix-revision` | negative | a fix whose `expectedRevision` differs from the document | refused as stale | 07: "A diagnostic describes a fix and never applies it" |
| JSON validates | positive | every envelope and every diagnostic document with a fix | validates | 07 |

## Diagnostics and schema changes

Codes: `@effect.duplicateRoot` and `@module.missingRequiredImport` gain
fix capability `@safe` (registry data, Q21); `@tool.actionDenied`.
Schema: the `fixes` array is produced for the first time; its schema
exists.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-diagnostics -p vibra-syntax -p vibra-fmt -p vibra-resolve -p vibra-ir -p vibra-interp -p vibra-types -p vibra-host -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. Every crate depends on `vibra-diagnostics`, so every
crate's tests run; `help` lists `edit fix` as supported.

## Stays unavailable

Every other code's fix; `lint` (30); `--write` of a fix in `project.vibon`.

## Excluded

`lint`; fixes that change a claim.

## Completion evidence

- [ ] `conformance/rules/m3-28.tsv` has one `active` row per case id.
- [ ] The registry test lists exactly two `@safe` codes.
- [ ] The validation commands exit 0.
