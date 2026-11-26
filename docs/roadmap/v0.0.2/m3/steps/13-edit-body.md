# Step 13: edit-body

Kind: vertical. `vibra edit body SUBJECT DATA`: the preview of the repair
edit of Q1. The subject's body section alone is replaced, the listed `use`s
inserted, the candidate formatted, the staged snapshot checked, every claim
and every resolution outside the subject compared before and after, and the
identity changes reported; a protected document, a wrong subject, or a
mismatch is refused. Applying is step 15.
Depends on: 04, 07, 09, 10.
Crates: `vibra-workspace`, `vibra-cli`, `vibra-conformance`,
`conformance/cases`.

## Prerequisites

- Read: [05 Transactional edit plans](../../../../spec/05-tooling.md#transactional-edit-plans);
  [05 Command surface](../../../../spec/05-tooling.md#command-surface) (`DATA`);
  [01 Declarations](../../../../spec/01-source-language.md#declarations);
  [01 Bodies and statements](../../../../spec/01-source-language.md#bodies-and-statements);
  [architecture](../architecture.md) section 3, rows I10, I11, I13, I28;
  [questions](../spec-questions.md) Q1, Q10, Q17, Q18, Q19, Q31, Q42.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/snapshot.rs:272` | `pub fn with_module_bytes` | staging a replacement in memory |
| `crates/vibra-workspace/src/semantic.rs:80` | `pub fn check_snapshot` | the re-check of the staged snapshot |
| `crates/vibra-workspace/src/format_plan.rs:381` | `pub fn plan_format` | the formatting of the staged module |
| `crates/vibra-syntax/src/document.rs:200` | `pub fn parse_source` | parsing the body text as a statement sequence standing alone |
| `crates/vibra-syntax/src/source/section.rs:46` | `pub struct Construct` | the subject's sections: the body section's span |
| `crates/vibra-resolve/src/snapshot.rs:54` | `pub struct ResolvedDeclaration` | the subject's declaration and span |
| `crates/vibra-resolve/src/snapshot.rs:248` | `pub fn references(&self) -> &[ResolvedReference] {` | the resolutions compared before and after |

From steps 04, 09, 07, 10: `identity::identities`, `edit::plan::PlanBuilder`,
`approval::protects`. Proposed: `edit::body::{BodyData, plan_body}`;
`vibra-cli::edit::body::run`; `vibra-conformance::edit::EditHandler`.

## Files owned

- `crates/vibra-workspace/src/edit/body.rs`
- `crates/vibra-workspace/tests/it/edit_body.rs`
- `crates/vibra-cli/src/edit/body.rs`
- `crates/vibra-cli/tests/it/edit_body.rs`
- `crates/vibra-conformance/src/edit.rs`
- `conformance/rules/m3-13.tsv`
- the case directories of the matrix below

## Ordered tasks

1. Subject: a `fn`, a `proc`, a `fn` or `proc` member, or a module value of
   a module of the root project's targets or `@tests`; anything else, a
   `@path` dependency, `@std`, a type, a case, a field, a `use`, a test,
   is invalid input (I10).
2. `DATA`: one VIBON record `{body: Str uses: List(Atom)}`, else invalid
   input. `body` is parsed standing alone as the statement sequence of a
   callable (or as one expression for a module value) with
   `parse_source` over a wrapper the engine builds; a token the wrapper's
   closer would not consume (a section keyword, a declaration keyword, a
   closer) is invalid input, so the text cannot close the body early or
   open a declaration; a parse error inside is `@command.diagnostics`
   with that error at the data's offset mapped into a source identity
   `<data>`. Each entry of `uses` is a module path.
3. The plan: one edit replacing the subject's body section (the text
   between `do` and the closer, or the expression after `be`) with the new
   body laid out by the formatter at the subject's indentation; one edit
   per listed path the module lacks, a `use P enduse` inserted into the
   import block in sorted position; `documents` with the module's
   revision. Then the staged snapshot is checked; `resultingDiagnostics`
   holds the diagnostics of the affected modules; `postconditions` the
   identity changes (Q19), computed by recomputing every identity of the
   local package on the staged snapshot. Invariant: the header, the
   guards, the doc, and every other declaration of the module are the same
   bytes before and after, by construction.
4. The comparison (I28): every claim of the local package and the
   resolution of every reference outside the subject's body are computed
   on the exact formatted candidate and compared with the capture's; a
   difference the staged check did not already report is a mismatched plan,
   `Planned::Invalid` with the code of Q42, result `@command.diagnostics`,
   plan returned, `applied: false`.
5. A protected document under an approval in force is `@tool.actionDenied`
   (Q10), result `@command.diagnostics`, plan returned, `applied: false`.
6. The `edit` payload `{plan, applied: false, resultingRevision: null}`;
   the human format prints each edit as a unified diff of the document.
7. `EditHandler`: builds an `EditRequest` from the manifest's `edit`
   record (`kind`, `subject`, `newName`, `path`, `data`, where `data` is a
   file holding the record) and dispatches through the plan table of step
   01, so the handler serves every kind and no later step edits it;
   `expect.editPlan` compared as parsed JSON after validation,
   `workspaceRevision` and `documents[].revision` rewritten by
   `revisions.ps1`.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| the body alone changes | `fn`; `proc`; member `fn`; member `proc`; module value; against: a type, a test, a field, a `use`, a text that closes early, a text that is a section |
| imports | a needed path listed and absent; a listed path already present; against: a listed path whose local name collides; a path that resolves to nothing |
| nothing outside the subject changes | the good repair; against: a difference the guard finds (with the collision check bypassed) |

## Test matrix

Cases are `edit` under `toolingV2` with `kind = "body"`. `T` is a tree
with `src/app/math.vib` holding `fn double takes {n: I32} returns twice:
I32 do return I32.add(n n) endfn` and a test module.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-edit-body-fn` | positive | `@app.math.double` with `{body: "return I32.mul(n 2i32)" uses: []}` | one edit over the body section's span; `resultingDiagnostics: []`; `postconditions` one `@witness` entry for `double` and one per dependent; `applied: false` | Q1 |
| `V2-TOOL-edit-body-postconditions-witness-only` | positive | the same | no `@nominal` and no `@claim` entry | 05: "Editing a body does not change it" |
| `V2-TOOL-edit-body-claims-unchanged` | positive | the same | every claim of the local package equal before and after; the header bytes of `double` unchanged | Q1: "a body edit cannot change a claim" |
| `V2-TOOL-edit-body-proc` | positive | a `proc` subject with an `await` in the new body | the plan; the row's ceiling unchanged | Q1 |
| `V2-TOOL-edit-body-member` | positive | `@app.model.User.nameLength` | one edit over the member's body, indented as the member | Q1 |
| `V2-TOOL-edit-body-module-value` | positive | `@app.config.limit` with `{body: "10i32" uses: []}` | one edit over the expression after `be` | Q1 |
| `V2-TOOL-edit-body-adds-use` | positive | a body using `Str.length` with `uses: [@std.str]` and the module lacking it | two edits, the `use` inserted in sorted position | Q1 |
| `V2-TOOL-edit-body-use-present` | positive | the same with the `use` already in the module | one edit | Q1 |
| `V2-TOOL-edit-body-use-collides` | negative | `uses: [@std.str]` in a module that declares `str` | `resultingDiagnostics` holds `@name.redeclaration`; `@command.diagnostics`; nothing written | 07: "a repeated module, import, local, test, or `given` key name" |
| `V2-TOOL-edit-body-use-unknown` | negative | `uses: [@app.nope]` | `resultingDiagnostics` holds `@module.unknownPath` | 07: "a path whose unit is not visible to the importing module, or that reaches no module" |
| `V2-TOOL-edit-body-closes-early-invalid` | negative | `{body: "return 1i32 endfn fn evil returns n: I32 do return 2i32" uses: []}` | `@command.invalidInput` | Q1: "a token that would close the body or open a declaration is invalid input" |
| `V2-TOOL-edit-body-section-text-invalid` | negative | `{body: "requires Ordered.greater(n 0i32) do return n" uses: []}` | `@command.invalidInput` | the same: a guard is a section, not a statement |
| `V2-TOOL-edit-body-two-bodies-invalid` | negative | a body text holding `do … enddo` as its only statement and then a second sequence | `@command.invalidInput` | the same |
| `V2-TOOL-edit-body-parse-error` | negative | `{body: "return (1i32" uses: []}` | `@command.diagnostics` with `@syntax.unmatchedDelimiter` at `<data>`, plan `null` | 07 Syntax diagnostics |
| `V2-TOOL-edit-body-resulting-error` | negative | a body with a type error | the plan with `resultingDiagnostics` holding the `@type.mismatch`; result `@command.diagnostics` | 05: "A plan that is … ill-typed … fails with nothing written" |
| `V2-TOOL-edit-body-type-subject-invalid` | negative | `@app.model.User` | `@command.invalidInput` | Q1: a type has no body |
| `V2-TOOL-edit-body-test-subject-invalid` | negative | a test named by its module and name | `@command.invalidInput` | 05 Command surface: `SUBJECT` is a query subject; a test has no entity atom |
| `V2-TOOL-edit-body-test-denied` | negative | a helper `fn` of `tests/math.vib` under an approval in force | `@tool.actionDenied`, `@command.diagnostics`, plan returned, `applied: false` | 05: "No fix or format may alter a protected claim, test, or policy outside its approval workflow" |
| `V2-TOOL-edit-body-test-helper-no-approval` | positive | the same helper with no approval | the plan | 05: "pinning every body would make repair impossible" |
| `V2-TOOL-edit-body-under-approval-allowed` | positive | `@app.math.double` under an approval in force | the plan, no denial | the same |
| `V2-TOOL-edit-dependency-subject-invalid` | negative | a declaration of a `@path` dependency | `@command.invalidInput` | 05: "Edits reach local modules and never vendored dependencies"; Q17 |
| `V2-TOOL-edit-std-subject-invalid` | negative | `@std.str.length` | `@command.invalidInput` | the same |
| `V2-TOOL-edit-body-field-invalid` | negative | a record field as subject | `@command.invalidInput` | Q1 |
| `V2-TOOL-edit-body-data-not-record` | negative | `DATA` is `"return 1i32"` | `@command.invalidInput` | 05 Command surface: `DATA` is "one canonical VIBON value" of the stated type |
| `V2-TOOL-edit-body-formatted` | positive | `body` on one line with odd spacing | the edit's replacement is the formatter's layout | 05: "format, without reordering operands" |
| `ATK-edit-body-other-subject` | negative | the host test over `plan_body` with a `type` subject and with a subject of another module's unit | `Planned::InvalidInput` | Q1 |
| `ATK-edit-body-resolution-guard` | negative | the host test: the collision check of the `use` insertion bypassed by a hook so that a listed `use` changes the resolution of a reference outside the subject | `Planned::Invalid` with the code of Q42; with the check in place the row is `@name.redeclaration` | Q18; Q42 |
| `ATK-edit-body-test-protected` | negative | the host test: a protected module reaches `PlanBuilder` with the protection check removed | the test fails; with the check it is `Planned::Denied` | architecture, I13 |
| JSON validates | positive | every envelope | validates | 05 |

## Diagnostics and schema changes

Codes: `@tool.actionDenied` (Q10), `@tool.invalidPlan` (Q42), the syntax,
name, module, and type codes of the staged check. Schema: the `edit`
payload (07), produced here.

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

Read: all exit 0; `help` lists `edit body` as supported with `["edit"]`.

## Stays unavailable

`--write` on every edit (15); `edit rename`, `edit imports`, `edit fix`,
`lint`; any edit of a header, a guard, a signature, or a type.

## Excluded

Applying; renames; imports; fixes; claim changes.

## Completion evidence

- [ ] `conformance/rules/m3-13.tsv` has one `active` row per case id.
- [ ] Every row of "Positions" has its case; the report maps each.
- [ ] `ATK-edit-body-test-protected` and `ATK-edit-body-resolution-guard`
      were seen to fail with their checks removed; the report says how.
- [ ] The validation commands exit 0.
