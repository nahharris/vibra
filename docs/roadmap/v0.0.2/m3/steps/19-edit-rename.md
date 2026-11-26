# Step 19: edit-rename

Kind: vertical. `vibra edit rename SUBJECT NEWNAME [--write]`: one
resolved renameable identity, every token resolution binds to it edited,
collisions and shadowing refused, the nominal and claim changes listed.
Depends on: 14, 16.
Crates: `vibra-workspace`, `vibra-cli`, `conformance/cases`.

## Prerequisites

- Read: [05 Transactional edit plans](../../../../spec/05-tooling.md#transactional-edit-plans)
  ("Rename rejects…"; "A field is renamed only where resolution binds a
  token to that field"); [05 Command surface](../../../../spec/05-tooling.md#command-surface)
  (`edit rename`, `NEWNAME`); [01 Declarations](../../../../spec/01-source-language.md#declarations)
  (shadowing); [02 Declared types](../../../../spec/02-type-system.md#declared-types);
  [questions](../spec-questions.md) Q10, Q15, Q19.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-resolve/src/snapshot.rs:151` | `pub struct ResolvedReference` | every token bound to the subject |
| `crates/vibra-resolve/src/identity.rs:184` | `pub struct DeclarationId` | the identities before and after |
| `crates/vibra-syntax/src/source/keyword.rs:37` | `pub const TOP_LEVEL` | reserved words `NEWNAME` may not be |
| `crates/vibra-types/src/stdlib/roles.rs:19` | `pub enum Role` | the prelude names `NEWNAME` may not be |

From 07, 11, 14, 16: the builder, `plan_replace`'s staging, `apply`, the
reference records. Proposed: `edit::rename::plan_rename`.

## Files owned

- `crates/vibra-workspace/src/edit/rename.rs`
- `crates/vibra-workspace/tests/it/edit_rename.rs`
- `crates/vibra-cli/tests/it/edit_rename.rs`
- `conformance/rules/m3-19.tsv`
- the case directories of the matrix below

## Ordered tasks

1. Subject: a renameable identity of the root project's units (Q19):
   top-level declaration, member, case, field, type parameter, labelled
   parameter, local binder; a module, `_`, a `@std` or dependency entity, a
   recovered node: invalid input. `NEWNAME`: one valid identifier that is
   not a reserved word or a prelude name, else invalid input; the casing
   convention is `@style.casing` in `resultingDiagnostics`, never a refusal.
2. The edits: the declaration's name token and every reference token
   resolution binds to the identity, in every local module (`@tests`
   included), each a one-token edit; an atom or string equal to the name
   is untouched. A labelled parameter's rename edits every call's label.
3. The staged snapshot is checked: a collision or a shadowing the new
   name causes appears as `@name.redeclaration` (or the applicable name
   code) in `resultingDiagnostics`, result `@command.diagnostics`, plan
   returned, `applied: false`.
4. `postconditions`: `@nominal` for the renamed declaration and each
   member and `impl` block whose owner path changes; `@claim` for every
   declaration whose signature names it; `@witness` for every dependent.
5. A document of the plan that is protected under a current approval is
   `@tool.actionDenied`.
6. `--write` through `apply` of step 14.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| every bound token is renamed | a use in the same module; in another module; in `@tests`; a member through its type; a field in a pattern and a projection; a label at a call; against: an equal atom, an equal string, a name in a comment, a same-spelled local in another scope |

## Test matrix

Cases are `edit` under `toolingV2` with `kind = "rename"`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-edit-rename-fn` | positive | `@app.math.double twice` with uses in two modules and a test | one edit per token: the declaration and each use; `documents` lists three files | 05: "`edit rename` needs one resolved renameable identity" |
| `V2-TOOL-edit-rename-postconditions-nominal` | positive | the same | `postconditions` has `@nominal` for `double` with before and after identities, `@witness` for each caller | 05: "A rename records any change of nominal or claim identity in `postconditions`" |
| `V2-TOOL-edit-rename-type-members` | positive | `@app.model.User Person` with members and an `impl` | `@nominal` entries for the type, each member, and the `impl` block's members | 02: "its anchor has … the type or interface it is nested in as owner path" |
| `V2-TOOL-edit-rename-claim-of-callers` | positive | the same | `@claim` for every declaration whose signature names `User` | 05: "A rename or move changes it, and the edit preview says so" |
| `V2-TOOL-edit-rename-field` | positive | a field `name` of `User` to `fullName`, with a pattern, a projection, a constructor label, and an unrelated local `name` | the field's tokens edited; the local and an atom `@name` untouched | 05: "A field is renamed only where resolution binds a token to that field; an equal atom or string is left alone" |
| `V2-TOOL-edit-rename-labelled` | positive | a labelled parameter `by` to `factor` | the declaration and each call's label edited | Q19 |
| `V2-TOOL-edit-rename-local` | positive | a local `x` to `y` | the binder and its uses in the sequence | Q19 |
| `V2-TOOL-edit-rename-case` | positive | `Shape.circle` to `round` | the case and every constructor and pattern | Q19 |
| `V2-TOOL-edit-rename-collision` | negative | to a name the module declares | `resultingDiagnostics` holds `@name.redeclaration`; `@command.diagnostics`; `applied: false` | 05: "collisions" |
| `V2-TOOL-edit-rename-shadowing` | negative | a local to a visible parameter's name | `@name.redeclaration` in the plan | 05: "shadowing"; 01: "No declaration or binder may shadow a visible name" |
| `V2-TOOL-edit-rename-keyword` | negative | `NEWNAME` `endfn` | `@command.invalidInput` | 05: "keywords" |
| `V2-TOOL-edit-rename-prelude` | negative | `NEWNAME` `Option` | `@command.invalidInput` | 05: `NEWNAME` is "one valid identifier"; 01: a prelude name is `@name.reservedDeclaration` |
| `V2-TOOL-edit-rename-discard` | negative | subject `_` | `@command.invalidInput` | 05: "Rename rejects discards" |
| `V2-TOOL-edit-rename-recovered` | negative | a declaration inside an `@error` node | `@command.invalidInput` | 05: "generated nodes" |
| `V2-TOOL-edit-rename-private-access` | negative | a rename after which a path in another module resolves to a private declaration | `@name.privateAccess` in `resultingDiagnostics` | 05: "private-access violations" |
| `V2-TOOL-edit-rename-casing` | positive | `double` to `Double` | the plan with `@style.casing` as a warning in `resultingDiagnostics`; result `@command.ok` | 01: casing "by convention that `check` reports and no rule depends on" |
| `V2-TOOL-edit-rename-several-documents` | positive | a public declaration used in two other modules | three documents in the plan | Q15 |
| `V2-TOOL-edit-rename-into-tests-denied` | negative | a declaration used in `tests/` under a current approval | `@tool.actionDenied` | 05: "No fix or format may alter a protected … test" |
| `V2-TOOL-edit-rename-std-invalid` | negative | `@std.str.length` | `@command.invalidInput` | 05: "never vendored dependencies"; Q19 |
| `V2-TOOL-edit-rename-module-invalid` | negative | `@app.math` | `@command.invalidInput` | Q19 |
| `ATK-edit-rename-apply-three-files` | positive | `--write` of the three-document plan on Windows | all three replaced through the journal; `applied: true` | 05: "publish all files, or none" |
| JSON validates | positive | every envelope | validates | 05 |

## Diagnostics and schema changes

Codes: `@name.redeclaration`, `@name.privateAccess`, `@style.casing`,
`@tool.actionDenied`. Schema: none new.

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

Read: all exit 0; `help` lists `edit rename` as supported.

## Stays unavailable

A rename that keeps a nominal identity (excluded by 05).

## Excluded

Imports (20); fixes (21).

## Completion evidence

- [ ] `conformance/rules/m3-19.tsv` has one `active` row per case id.
- [ ] Every row of "Positions" has its case; the report maps each.
- [ ] The validation commands exit 0.
