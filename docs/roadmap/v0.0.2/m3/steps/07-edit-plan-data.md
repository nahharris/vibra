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
  section 3; [questions](../spec-questions.md) Q12, Q13, Q18, Q19, Q42.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-diagnostics/src/diagnostic.rs:60` | `pub struct DocumentRevision` | the revision of a document a fix was computed against |
| `crates/vibra-diagnostics/src/span.rs:23` | `pub struct ByteSpan` | an edit's half-open span |
| `crates/vibra-schema/src/diagnostic.rs:119` | `pub struct DiagnosticDocument` | `resultingDiagnostics` are rendered with it |
| `crates/vibra-schema/src/envelope.rs:217` | `pub struct Envelope` | the plan rides in the `edit` payload |

Proposed: `edit::plan::{EditPlan, Edit, PlanDocument, Postcondition,
PostconditionKind, Operation, SyntaxIdentity, PlanBuilder, Planned}`;
`vibra-schema::edit_plan::{EditPlanDocument, EditPayload}` with
`urn:vibra:schema:v2:edit-plan`.

## Files owned

- `crates/vibra-workspace/src/edit/plan.rs`
- `crates/vibra-workspace/tests/it/edit_plan.rs`
- `crates/vibra-schema/src/edit_plan.rs`
- `crates/vibra-schema/schemas/v2/edit-plan.json`
- `crates/vibra-schema/schemas/v2/payloads/edit.json`
- `crates/vibra-schema/tests/it/edit_plan.rs`

## Ordered tasks

1. `EditPlan`: `{schemaVersion, toolVersion, workspaceRevision, operation,
   target, documents, edits, resultingDiagnostics, postconditions}`;
   `Operation` one of `format`, `fix`, `rename`, `imports`, `body`;
   `PlanDocument {path, revision}` with the revision `sha256:` of the
   document's exact bytes (Q12); `Edit {path, span, replacement,
   syntaxIdentity: {nodeId, syntaxKind}, fingerprint}` (Q13);
   `Postcondition {declaration, kind, before, after}` (Q19).
2. `PlanBuilder`: collects edits, sorts by path then span start, and
   derives `documents` from the edits' paths with the revisions of the
   capture's bytes. An overlap is a typed refusal of the plan, never a
   trap (Q18): the builder returns `Planned::Invalid(plan, diagnostic)`
   with the plan as built and the code of Q42, so the caller answers
   `@command.diagnostics` with `applied: false`. Invariant: a plan that
   reaches `Planned::Plan` has sorted, non-overlapping edits.
3. The JSON form and schema: the `edit` payload is `{plan, applied,
   resultingRevision}`, `plan` the plan document or `null`; the payload's
   exhausted form (Q28) is `{plan: null, applied, resultingRevision}`.
4. A fixture plan the tests of steps 13, 15, 26, 27, 28 reuse: one
   document, two edits, one postcondition; and a second fixture over two
   documents, for the refusal rows of step 15.

## Test matrix

Host tests.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-edit-plan-schema` | positive | the fixture plan rendered to JSON | validates against `urn:vibra:schema:v2:edit-plan`; the `edit` payload validates in an envelope | 05: "`EditPlan`, version 2" |
| `V2-TOOL-edit-plan-document-revision` | positive | a document of 5 bytes | `revision` is `sha256:` of those bytes | Q12 |
| `V2-TOOL-edit-plan-fields` | positive | one edit | `syntaxIdentity.nodeId` is `path#start-end`; `fingerprint` is the SHA-256 of the replaced bytes | Q13 |
| sorted | positive | edits given out of order over two paths | sorted by path, then span | 05: "Edits are ordered by path and span" |
| `ATK-edit-plan-overlap-refused` | negative | two edits whose spans overlap | `Planned::Invalid` with the plan and the diagnostic of Q42; no trap | 05: "A plan that is stale, ambiguous, overlapping, ill-typed, mismatched, or that changes a policy without approval fails with nothing written"; Q18 |
| adjacent not overlapping | boundary | spans `3..5` and `5..7` | accepted | 05: "do not overlap" |
| postconditions sorted | positive | three postconditions out of order | sorted by declaration then kind | Q19 |
| exhausted form | positive | `EditPayload::exhausted()` of an applied plan | `{plan: null, applied: true, resultingRevision: "sha256:…"}` validates | Q28 |
| schema rejects an unknown field | negative | a plan JSON with `extra: 1` | the schema refuses it | 05: "Unknown input fields and versions are rejected" |

## Diagnostics and schema changes

Codes: `@tool.invalidPlan` (Q42: not in the registry today; the row is
written against the recommended answer and the step stops if the ruling has
not landed). Schema: `urn:vibra:schema:v2:edit-plan` (new); the `edit`
payload.

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

Computing any edit (13, 26, 27, 28); applying (15).

## Completion evidence

- [ ] Each row is a test, named in the report.
- [ ] The report quotes `PlanBuilder::build`'s overlap check and the
      `Planned::Invalid` it returns.
- [ ] The validation commands exit 0.
