# Step 25: expansions

Kind: vertical. The expansions `references`, `members`, `calls`,
`applications`, and `declarations`, each a list of the index's records in
the index's order, under the output bound.
Depends on: 04, 09, 12.
Crates: `vibra-workspace`, `vibra-schema`, `conformance/cases`.

## Prerequisites

- Read: [05 Workspace queries](../../../../spec/05-tooling.md#workspace-queries)
  (`--expand`, "`calls` holds the edges that execute a function");
  [05 Index records](../../../../spec/05-tooling.md#index-records) (the
  record shapes); [questions](../spec-questions.md) Q28, Q34.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-resolve/src/snapshot.rs:151` | `pub struct ResolvedReference` | `references` |
| `crates/vibra-resolve/src/snapshot.rs:54` | `pub struct ResolvedDeclaration` | `declarations`, `members` |
| `crates/vibra-types/src/check/mod.rs:66` | `pub struct CheckResult` | the application facts of step 08 |
| `crates/vibra-cli/src/render.rs:23` | `pub const LIMIT_PROFILE` | the output bound's name joins `limits.reached` |

Proposed: `query::expansions::{expand, Expand}`, filled into the
dispatcher's five entries, and the record builders that step 29 reuses for
the index: `declaration_record`, `reference_record`, `application_record`.

## Files owned

- `crates/vibra-workspace/src/query/expansions.rs`
- `crates/vibra-workspace/tests/it/query_expansions.rs`
- `crates/vibra-cli/tests/it/query_expansions.rs`
- `crates/vibra-schema/schemas/v2/query/expansions.json`
- `conformance/rules/m3-25.tsv`
- the case directories of the matrix below

## Ordered tasks

1. The record builders, in the index's field order and sort order, from
   the resolution and the facts; `facts` of a declaration record absent
   when its check failed; `references[].to` absent when it depends on
   failed checking.
2. `references`: the reference records whose `to` is the subject;
   `members`: the subject's members' declaration records; `calls`: the
   application facts whose callee is the subject with kind `@function` or
   `@procPlan`; `applications`: every application fact whose callee is the
   subject; `declarations`: every declaration record of the local package
   (Q34).
3. The bound: an expansion whose envelope would exceed the output limit
   is the exhausted envelope of step 01, with every expansion list empty,
   `complete: false`, status `unknown`, `limits.reached` `outputBytes`
   (Q28); no list is truncated.

## Test matrix

Cases are `query` under `toolingV2`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-query-expand-references` | positive | `@app.math.double --expand references` with three uses in two modules | three reference records sorted by source, start, end, each `from` the enclosing declaration's identity | 05 Index records: `references` |
| `V2-TOOL-query-expand-references-unresolved` | positive | a use inside a body with an error | its record has no `to` | 05: "`to` (optional `Str`, absent when it depends on failed checking)" |
| `V2-TOOL-query-expand-members` | positive | `@app.model.User --expand members` | the records of its fields' owner's members: the `fn` and `proc` members | 05: `members` |
| `V2-TOOL-query-expand-calls` | positive | a `fn` called twice and passed once as a value | two application facts of kind `@function` | 05: "`calls` holds the edges that execute a function" |
| `V2-TOOL-query-expand-applications` | positive | a constructor applied, a plan built, a field read | `applications` holds `@constructor`, `@procPlan`, `@recordProjection` facts | 05: "`applications` also holds constructions, projections, collection lookups, and the building of plans" |
| `V2-TOOL-query-expand-declarations` | positive | `@workspace --expand declarations` | every declaration record of the local package sorted by `id`; none of `@std` | 05: "The index covers the local package" |
| `V2-TOOL-query-expand-declaration-facts` | positive | a declaration that checks | `facts` with `signature`, `errors: []`, `applications`, `contracts: []`, `claimIdentity`, `witnessIdentity` | 05: the `facts` record |
| `V2-TOOL-query-expand-declaration-no-facts` | positive | a declaration whose body has an error | no `facts` field | 05: "`facts` (optional, absent when not established)" |
| `V2-TOOL-query-expand-bounded` | positive | a generated tree with 20,000 references to one declaration | the exhausted envelope: `expansions.references: []`, `complete: false`, status `unknown`, `limits.reached: ["outputBytes"]`, the envelope within 16 MiB and valid | 05: "the tool emits a bounded, valid document with status `unknown`"; Q28 |
| `V2-TOOL-query-expand-unknown` | negative | `--expand bodies` | `@command.invalidInput` | 05: the option table |
| JSON validates | positive | every envelope | validates | 05 |

The `claimIdentity` and `witnessIdentity` of a `facts` record are those of
steps 04 and 09, which this step depends on.

## Diagnostics and schema changes

Codes: `@tool.limitExceeded`. Schema: the `expansions` member of the query
schema, one file.

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

Read: all exit 0.

## Stays unavailable

`--include index` (29).

## Excluded

The index document (29).

## Completion evidence

- [ ] `conformance/rules/m3-25.tsv` has one `active` row per case id.
- [ ] The bounded row's envelope size is in the report.
- [ ] The validation commands exit 0.
