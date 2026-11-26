# Step 05: query-subjects

Kind: vertical. The `query` command becomes available: the four subject
forms are parsed and resolved, the `--include` and `--expand` options are
validated, the diagnostic-code and `@workspace` subjects are answered, and
a position or entity subject is resolved to its node or declaration with
compact metadata. The structural record is step 06 and the semantic
members step 12; until they land their includes answer `unavailable`
fields, never a wrong fact.
Depends on: 01.
Crates: `vibra-workspace`, `vibra-cli`, `vibra-schema`, `vibra-conformance`,
`conformance/cases`, `conformance/tools`.

## Prerequisites

- Read: [05 Workspace queries](../../../../spec/05-tooling.md#workspace-queries);
  [05 Command payloads](../../../../spec/05-tooling.md#command-payloads) (`query`);
  [05 Program arguments, execution and evaluation](../../../../spec/05-tooling.md#program-arguments-execution-and-evaluation) ("Limits");
  [07 Case operations and inputs](../../../../spec/07-diagnostics-and-conformance.md#case-operations-and-inputs);
  [questions](../spec-questions.md) Q30, Q32, Q33, Q36, Q37.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/format_plan.rs:53` | `pub struct WorkspaceCapture` | the one capture a query answers from |
| `crates/vibra-workspace/src/format_plan.rs:150` | `pub fn revision` | the revision the result names |
| `crates/vibra-resolve/src/snapshot.rs:209` | `pub struct Resolution` | declarations by identity and path |
| `crates/vibra-diagnostics/src/registry.rs:185` | `pub enum DiagnosticCode` | the code subject |
| `crates/vibra-diagnostics/src/registry.rs:224` | `pub const fn summary` | its summary |
| `crates/vibra-cli/src/check.rs:146` | `pub fn run` | the shape of a command over a capture, with limits reached |
| `crates/vibra-conformance/src/format.rs:23` | `pub struct FormatHandler` | the model of a `toolingV2` handler |

Proposed: `query::subject::{Subject, parse_subject}`, `query::{query,
QueryOptions, QueryResult, Include, Expand}`; `vibra-cli::query::run`;
`vibra-schema::query_payload::QueryPayload` with the schema
`urn:vibra:schema:v2:query`; `vibra-conformance::query::QueryHandler`.

## Files owned

- `crates/vibra-workspace/src/query/mod.rs`
- `crates/vibra-workspace/src/query/subject.rs`
- `crates/vibra-workspace/tests/it/query_subject.rs`
- `crates/vibra-cli/src/query.rs`
- `crates/vibra-cli/tests/it/query_subject.rs`
- `crates/vibra-schema/src/query_payload.rs`
- `crates/vibra-schema/schemas/v2/payloads/query.json`
- `crates/vibra-schema/tests/it/query_payload.rs`
- `crates/vibra-conformance/src/query.rs`
- `crates/vibra-conformance/tests/it/engine_reads.rs`
- `conformance/tools/revisions.ps1`
- `conformance/rules/m3-05.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `parse_subject`: `path:offset` (a confined workspace path with `/`, a
   decimal offset), a canonical entity atom, a registered diagnostic code,
   or `@workspace`; anything else is invalid input. Invariant: the parse
   touches no file.
2. `query`: a code subject reads the registry and takes no snapshot
   (`workspaceRevision` null, Q37); every other subject needs the capture.
   A capture fault is `@command.diagnostics` with the empty payload; a
   snapshot with a document over the nesting limit answers the limit alone
   (Q36). A path that is no module of the snapshot, an offset past the end
   or inside a scalar, an atom that resolves to nothing, an unknown
   include or expand name: invalid input. Invariant: the result names the
   capture's revision.
3. The compact metadata with no include: `{nodeId, identity,
   resolvedKind}`; `identity` of an entity subject from the resolution, of
   a position the declaration whose span contains it. Each include not yet
   produced (steps 06, 12, 15, 16, 22) answers `{status: "unavailable",
   value: null}` under its member.
4. `QueryPayload`, its schema with the two position records as `$defs`
   (shapes from the chapter; their producers are steps 06 and 12), and the
   envelope's validation.
5. `vibra-cli::query::run`, `SUPPORTED = true`; the human format prints
   the subject, kind, identity, and each present member as text.
6. `QueryHandler`: runs the subject with the case's include and expand
   lists and compares `expect.query` as parsed JSON after validation, and
   `expect.index` as canonical bytes; `revisions.ps1` learns to rewrite
   `"workspaceRevision": "sha256:…"` in a case's JSON expectations.
7. The architecture test `engine_reads.rs`: every call of `std::fs` read
   functions in `vibra-workspace/src` is in `discovery.rs`, `snapshot.rs`,
   `approval.rs`, `format_plan.rs`, or `edit/journal.rs`; no `query` module
   reads a file.

## Test matrix

Cases are `query` under `toolingV2`; a row with no case id is a host test.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-query-revision-named` | positive | `@workspace`, no include | `@command.ok`; `workspaceRevision` is the tree's; `resolvedKind: "workspace"`; `metadata` compact | 05: "A query answers from one captured revision" |
| `V2-TOOL-query-entity-compact` | positive | `@app.math.double` | `resolvedKind: "entity"`, `identity: {kind: "@fn", canonical: "sha256:…"}` | 05 Workspace queries: "a canonical entity atom" |
| `V2-TOOL-query-entity-unknown` | negative | `@app.math.nope` | `@command.invalidInput`, exit 2, no diagnostic | 05 Command surface: "A subject that matches nothing … is invalid input" |
| `V2-TOOL-query-code-known` | positive | `@type.mismatch` | `resolvedKind: "diagnosticCode"`, `metadata.diagnostic` with `level: "@error"`, `domain: "type"`, `fixCapability: "@none"`; `workspaceRevision: null` | 05 Index records: "A diagnostic code used as a query subject reads the compiler's registry" |
| `V2-TOOL-query-code-unknown` | negative | `@typo.mismatch` | `@command.invalidInput` | Q37 |
| `V2-TOOL-query-position-compact` | positive | `src/app/main.vib:12` inside `double`'s body | `resolvedKind: "position"`, `identity` of `double`, `nodeId` `src/app/main.vib#<start>-<end>` | 05: "`nodeId` is `<sourceId>#<start>-<end>`" |
| `V2-TOOL-query-position-not-a-module` | negative | `project.vibon:0` | `@command.invalidInput` | Q36 |
| `V2-TOOL-query-position-dependency` | positive | a position in a `@path` dependency's module | answered | Q32 |
| `V2-TOOL-query-include-unknown` | negative | `--include types` | `@command.invalidInput` | 05: the option table is closed |
| `V2-TOOL-query-include-pending` | positive | `@app.math.double --include type` before step 12 | `metadata.type` is `{status: "unavailable", value: null}` | 05: "`unavailable` (with a `null` value)" |
| `V2-TOOL-query-nesting-limit` | negative | a tree with a module nested 1,025 levels; subject `@workspace` | `@tool.limitExceeded` alone, status `unknown`, `limits.reached: ["sourceNesting"]`, empty payload | 05 Limits: "No later stage runs on the snapshot that holds such a document" |
| `V2-TOOL-query-capture-fault` | negative | a tree whose `project.vibon` has an invalid shape | `@command.diagnostics` with the capture's one diagnostic, empty payload | 05: "A run whose project could not be captured … answers `@command.diagnostics`" |
| `ATK-engine-reads-enumerated` | negative | the architecture test given a `query/subject.rs` containing `std::fs::read` | the test reports it | 05 One workspace engine: "Semantic phases consume immutable inputs they are given" |
| subject grammar | negative | `src/app/main.vib:x`, `src/app/main.vib`, `main.vib:3`, `@`, a code without its `@` | each `@command.invalidInput` | 05 Workspace queries |
| JSON validates | positive | every envelope of the rows above | validates against the command-result schema with the query payload | 05: "A producer MUST validate every document it emits" |
| revisions in JSON | positive | `revisions.ps1 -Check` over a case with `expect.query` | finds and checks the `workspaceRevision` field | 05 Snapshot identity |

## Diagnostics and schema changes

Codes: `@tool.limitExceeded`, `@tool.unavailable` (the pending includes
answer a status, not a diagnostic). Schema: `urn:vibra:schema:v2:query`
(new), the `query` payload of the command-result schema.

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

Read: all exit 0; `help` now lists `query` as supported with `["query"]`.

## Stays unavailable

The includes `syntax` (06), `context`, `source`, `signature`, `type`,
`visibility`, `diagnostics` (12), `effects` (15), `index` (22); every
expansion (15, 16). Each answers the unavailable status.

## Excluded

Structural and semantic facts; expansions; the index.

## Completion evidence

- [ ] `conformance/rules/m3-05.tsv` has one `active` row per case id.
- [ ] `query::query` takes a `&WorkspaceCapture` and no path: the report
      quotes the signature.
- [ ] The validation commands exit 0.
