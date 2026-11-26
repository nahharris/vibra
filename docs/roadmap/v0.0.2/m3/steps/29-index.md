# Step 29: index

Kind: vertical. `query @workspace --include index --expand declarations`:
the `@index.v2` document with every declaration, implementation, and
reference of the local package, each declaration's claim and witness
identities, its JSON form, and the output bound.
Depends on: 20, 25.
Crates: `vibra-workspace`, `vibra-schema`, `conformance/cases`.

## Prerequisites

- Read: [05 Index records](../../../../spec/05-tooling.md#index-records) whole;
  [06 Open signatures](../../../../spec/06-runtime.md#open-signatures);
  [architecture](../architecture.md) rows I11, I17, I23;
  [questions](../spec-questions.md) Q28, Q33, Q34.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-resolve/src/snapshot.rs:322` | `pub fn canonical_text` | the model of a canonical observation writer (`@resolved.v2`) |
| `crates/vibra-ir/src/signature.rs:223` | `pub struct ImplementationRecord` | `implementations[].implementation` |
| `crates/vibra-syntax/src/schema.rs:472` | `pub fn canonical_bytes` | canonical bytes of the index |

From 04, 09, 20, 25: identities and the record builders. Proposed:
`query::index::{index, IndexDocument}`, filled into the dispatcher's
`index` entry; `vibra-schema::index` with `urn:vibra:schema:v2:index`.

## Files owned

- `crates/vibra-workspace/src/query/index.rs`
- `crates/vibra-workspace/tests/it/index.rs`
- `crates/vibra-cli/tests/it/query_index.rs`
- `crates/vibra-schema/src/index.rs`
- `crates/vibra-schema/schemas/v2/index.json`
- `crates/vibra-schema/schemas/v2/query/index.json`
- `crates/vibra-schema/tests/it/index.rs`
- `conformance/rules/m3-29.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `IndexDocument` with `format`, `revision`, `declarations` sorted by `id`,
   `implementations` sorted by the canonical bytes of `contract` then
   `receiver`, `references` sorted by source, start, end; written through
   the schema writer; the chapter's example decodes and round-trips.
2. Each declaration record: `id`, `kind`, `module`, `owner` (optional),
   `access`, `source`, `text` (the formatted source), `binding` for an
   external, `facts` when established with `claimIdentity` and
   `witnessIdentity`; an implementation record with its `members`; a
   reference record with `from`, `written`, `source`, `to`.
3. The JSON form with the same order and facts, atoms keeping `@`, absent
   fields `null`, under `metadata.index`; the VIBON text under
   `expect.index` for cases.
4. The bound: an index whose envelope would exceed the output limit is the
   exhausted envelope of step 01, `metadata.index` null, `complete: false`,
   `limits.reached: ["outputBytes"]`, status `unknown` (Q28).

## Test matrix

Cases are `query` with `@workspace --include index --expand declarations`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-index-example` | positive | the chapter's `hello` project | the chapter's example document, byte for byte, with the tree's revision | 05: the `@index.v2` example |
| `V2-TOOL-index-round-trip` | positive | the same | the JSON form decodes to the same facts; the VIBON decodes and round-trips | 07 Required coverage 15 |
| `V2-TOOL-index-facts` | positive | a project with a checked `fn`, a `proc` with a row, an `impl` | the `fn` has `facts` with `signature`, `errors`, `applications`, `contracts`, both identities; the `proc` has `ceiling` and `effects`; the `impl` has an implementation record with `members` | 05: the `facts` record |
| `V2-TOOL-index-identities-match` | positive | the trees of `V2-TOOL-claim-body-edit-same` | `claimIdentity` and `witnessIdentity` equal the host test's `Identities` | 05: "A `facts` record always holds its two identities" |
| `V2-TOOL-index-no-facts` | positive | a declaration whose body has an error | no `facts` | 05: "`facts` (optional, absent when not established)" |
| `V2-TOOL-index-local-only` | positive | a project with a `@path` dependency and `@std` references | no dependency or `@std` declaration; references `to` their identities | 05: "The index covers the local package; a dependency or standard-library entity appears only as a reference target" |
| `V2-TOOL-index-implementation-identity` | positive | two `impl Iter(Item: U8)` and `impl Iter(Item: Str)` on one type | two implementation records; a changed choice changes the receiver's claim | 05: "Two implementations for different applications of a generic interface never collapse into one" |
| `V2-TOOL-index-module-declaration` | positive | the module record | no `facts.signature`; `text` the whole formatted module | 05: "A module declaration has no `facts.signature` and its whole formatted text" |
| `V2-TOOL-index-external-binding` | positive | a library query of a declaration bound `external` is outside the local package; a local project has none | the `binding` field is absent for every local declaration, and the schema admits it | 05: "`binding` (optional record …, present for an external declaration)" |
| `V2-TOOL-index-rejected-body` | positive | a rejected `proc` | its record has no `facts`, so no rows | 05: "A rejected body never shows exact rows or proofs" |
| `V2-TOOL-query-index-bounded` | positive | a generated project with 10,000 declarations | the exhausted envelope: `metadata.index` null, `complete: false`, status `unknown`, `limits.reached: ["outputBytes"]`, a valid envelope within 16 MiB | 05: "the tool emits a bounded, valid document with status `unknown`"; Q28 |
| `V2-TOOL-index-without-expand` | positive | `--include index` alone | `declarations: []` and the implementations and references | 05: "`query @workspace --include index --expand declarations` emits" the full index |
| JSON validates | positive | every envelope | validates against `urn:vibra:schema:v2:index` and the query schema | 05 |

## Diagnostics and schema changes

Codes: `@tool.limitExceeded` (the output bound). Schema:
`urn:vibra:schema:v2:index` (new); the `index` member of the query schema;
`@index.v2` gains a decoder.

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

Read: all exit 0; the bounded row's envelope size is in the report.

## Stays unavailable

Nothing new.

## Excluded

Embedding, ranking, storage.

## Completion evidence

- [ ] `conformance/rules/m3-29.tsv` has one `active` row per case id.
- [ ] The chapter's example is reproduced byte for byte; the report shows
      the diff is empty.
- [ ] The validation commands exit 0.
