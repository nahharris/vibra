# Step 07: edit-plan-data

Kind: infrastructure. The `EditPlan` data type, its builder with the
ordering and overlap rules, its JSON schema, and the postcondition and
document-revision records every edit of M3 fills. No command produces a
plan yet.
Depends on: 01.
Crates: `vibra-workspace`, `vibra-schema`.

## Prerequisites

- Read: [05 Transactional edit plans](../../../../spec/05-tooling.md#transactional-edit-plans);
  [07 Diagnostic documents and spans](../../../../spec/07-diagnostics-and-conformance.md#diagnostic-documents-and-spans)
  (`fixes`); [architecture](../architecture.md) "Boundary types" (Edit),
  section 3; [questions](../spec-questions.md) Q12, Q13, Q18, Q19.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-diagnostics/src/diagnostic.rs:60` | `pub struct DocumentRevision` | the revision of a document a fix was computed against |
| `crates/vibra-diagnostics/src/span.rs:23` | `pub struct ByteSpan` | an edit's half-open span |
| `crates/vibra-schema/src/diagnostic.rs:119` | `pub struct DiagnosticDocument` | `resultingDiagnostics` are rendered with it |
| `crates/vibra-schema/src/envelope.rs:217` | `pub struct Envelope` | the plan rides in the `edit` payload |

Proposed: `edit::plan::{EditPlan, Edit, PlanDocument, Postcondition,
PostconditionKind, Operation, SyntaxIdentity, PlanBuilder}`;
`vibra-schema::edit_plan::{EditPlanDocument, EditPayload}` with
`urn:vibra:schema:v2:edit-plan`.

## Files owned

- `crates/vibra-workspace/src/edit/mod.rs`
- `crates/vibra-workspace/src/edit/plan.rs`
- `crates/vibra-workspace/tests/it/edit_plan.rs`
- `crates/vibra-schema/src/edit_plan.rs`
- `crates/vibra-schema/schemas/v2/edit-plan.json`
- `crates/vibra-schema/schemas/v2/payloads/edit.json`
- `crates/vibra-schema/tests/it/edit_plan.rs`

## Ordered tasks

1. `EditPlan`: `{schemaVersion, toolVersion, workspaceRevision, operation,
   target, documents, edits, resultingDiagnostics, postconditions}`;
   `PlanDocument {path, revision}` with the revision `sha256:` of the
   document's exact bytes (Q12); `Edit {path, span, replacement,
   syntaxIdentity: {nodeId, syntaxKind}, fingerprint}` (Q13);
   `Postcondition {declaration, kind, before, after}` (Q19).
2. `PlanBuilder`: collects edits, sorts by path then span start, rejects
   an overlap as a defect (`@runtime.invalidCheckedProgram`, Q18), and
   derives `documents` from the edits' paths with the revisions of the
   capture's bytes. Invariant: a plan's edits never overlap and are sorted.
3. The JSON form and schema: the `edit` payload is `{plan, applied,
   resultingRevision}`, `plan` the plan document or `null`.
4. A fixture plan the tests of steps 11, 14, 19, 20, 21 reuse: two
   documents, three edits, one postcondition.

## Test matrix

Host tests.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-edit-plan-schema` | positive | the fixture plan rendered to JSON | validates against `urn:vibra:schema:v2:edit-plan`; the `edit` payload validates in an envelope | 05: "`EditPlan`, version 2" |
| `V2-TOOL-edit-plan-document-revision` | positive | a document of 5 bytes | `revision` is `sha256:` of those bytes | Q12 |
| `V2-TOOL-edit-plan-fields` | positive | one edit | `syntaxIdentity.nodeId` is `path#start-end`; `fingerprint` is the SHA-256 of the replaced bytes | Q13 |
| sorted | positive | edits given out of order over two paths | sorted by path, then span | 05: "Edits are ordered by path and span" |
| `ATK-edit-plan-overlap-is-defect` | negative | two edits whose spans overlap | the builder returns the defect; no plan | 05: "do not overlap"; Q18 |
| adjacent not overlapping | boundary | spans `3..5` and `5..7` | accepted | the same |
| postconditions sorted | positive | three postconditions out of order | sorted by declaration then kind | Q19 |
| schema rejects an unknown field | negative | a plan JSON with `extra: 1` | the schema refuses it | 05: "Unknown input fields and versions are rejected" |

## Diagnostics and schema changes

Codes: none emitted; `@runtime.invalidCheckedProgram` names the defect.
Schema: `urn:vibra:schema:v2:edit-plan` (new); the `edit` payload.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

Every `edit` command and `lint`.

## Excluded

Computing any edit (11, 19, 20, 21); applying (14).

## Completion evidence

- [ ] Each row is a test, named in the report.
- [ ] The report quotes `PlanBuilder::build`'s overlap check.
- [ ] The validation commands exit 0.
