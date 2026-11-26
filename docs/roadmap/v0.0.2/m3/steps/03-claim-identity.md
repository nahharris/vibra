# Step 03: claim-identity

Kind: vertical. The `@claim.v2` record of every declaration of a snapshot
and its identity, by kind, with the target facts of an entry; the 05 claim
vector; independence from layout, path, platform, and line endings.
Depends on: 02.
Crates: `vibra-workspace`, `conformance/cases`.

## Prerequisites

- Read: [05 Snapshot, claim and witness identity](../../../../spec/05-tooling.md#snapshot-claim-and-witness-identity)
  ("Claim and witness", the `@claim.v2` table, the target fact, the claim
  vector); [06 Open signatures](../../../../spec/06-runtime.md#open-signatures);
  [02 Declared types](../../../../spec/02-type-system.md#declared-types);
  [architecture](../architecture.md) rows I1, I2, I4, "The theories";
  [questions](../spec-questions.md) Q2, Q3, Q6.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-resolve/src/snapshot.rs:54` | `pub struct ResolvedDeclaration` | the anchor, identity, access, and span of each declaration |
| `crates/vibra-resolve/src/identity.rs:131` | `pub fn identity` | the declaration identity |
| `crates/vibra-ir/src/signature.rs:105` | `pub struct SignatureRecord` | the signature a claim embeds |
| `crates/vibra-ir/src/signature.rs:223` | `pub struct ImplementationRecord` | the implementation records |
| `crates/vibra-ir/src/encode.rs:340` | `pub fn encode_open_type` | the open type encoding of a signature |
| `crates/vibra-types/src/check/mod.rs:229` | `pub fn types_document` | where the checker already writes every signature record (`@types.v2`) |
| `crates/vibra-workspace/src/entry.rs:42` | `pub struct EntryFacts` | the entry of each target |
| `crates/vibra-workspace/src/binding.rs:115` | `pub fn plan_of` | the `BindingPlan` of a target, for its target fact |
| `crates/vibra-host/src/recipe.rs:715` | `pub fn recipe_identity` | the recipe identity a target fact carries |
| `crates/vibra-workspace/src/revision.rs:203` | `pub fn workspace_revision` | the revision whose CRLF behaviour the matrix contrasts |

Proposed: `identity::claim::{ClaimRecord, claim_of}`,
`identity::{Identities, identities}` (the witness half is step 04; this
step leaves `witness` empty strings and the field documented as such).

## Files owned

- `crates/vibra-workspace/src/identity/mod.rs`
- `crates/vibra-workspace/src/identity/claim.rs`
- `crates/vibra-workspace/tests/it/claim.rs`
- `conformance/rules/m3-03.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `ClaimRecord` with the nine fields of the chapter in order, a canonical
   writer that reuses the VIBON canonical serializer of `vibra-syntax`
   (build a `TypedDocument` against a `Schema` for `@claim.v2`, never a
   hand-written string), and `identity()` = `sha256:` of
   `vibraClaimIdentityV2`, a zero byte, the canonical bytes. Invariant: the
   claim vector of 05 hashes to `sha256:ee73…302a` with 436 canonical bytes.
2. `claim_of` by kind, as Q3 fills each: the signature record from the
   checker's `@types.v2` facts (read the same record the document holds,
   never a second computation); `defaults` from `fragment::canonical_text`
   of each labelled default; `guards` likewise; `representation` resolved;
   `implementations` sorted by the canonical bytes of `contract` then
   `receiver`; `targets` for an entry: `{target, effects (sorted by path),
   bindings (sorted by leaf, each with the recipe identity)}`. Invariant:
   an omitted part is written as its value (empty row, resolved
   visibility, bound `Any`).
3. `identities(snapshot, resolution, check)` over every declaration of
   every unit, local and dependency, in identity order; a declaration
   whose check failed has no claim (the map omits it, and the report says
   why: an unknown signature is not a public contract).
4. Host tests on independence: the same tree hashed from two checkout
   paths, once with CRLF endings, give equal claims and different
   revisions.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| a claim is content-addressed | a comment; layout; a body edit; against: a signature, a bound, a default, a guard, a representation, an `impl` choice, a target binding |
| a claim is per kind | `fn`, `proc`, member, `type` with `where`, `case`, `interface`, associated type, module value, `test`, entry |

## Test matrix

No command shows a claim before step 22, so each `V2-` row below is a
`workspaceCheck` case under `staticV2`, accepted, whose tree the host test
`claim::corpus_claims` also loads to assert the stated relation between
the claims of its declarations. The trees are corpus cases so that step 22
reuses them: its index shows the identities these rows compare.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-claim-vector` | positive | a project whose entry is `proc main returns code: ExitCode do return ExitCode(0u8) endproc` in `@hello.main`, package `hello` `0.1.0`, with no bindings | the claim record's canonical bytes begin `{format: @claim.v2 declaration: "sha256:213e`, end `implementations: [] targets: [{…}]}` for the entry; the vector's record (no target) hashes to `sha256:ee7305…302a` | 05: the claim vector |
| `V2-TOOL-claim-body-edit-same` | positive | two trees differing in `main`'s body | equal anchor, equal claim | 05: "Editing a body does not change it" |
| `V2-TOOL-claim-comment-same` | positive | two trees differing by a comment inside a `fn` header | equal claim | Q2 |
| `V2-TOOL-claim-layout-same` | positive | two trees, one formatted, one with every section on one line | equal claim | Q2 |
| `V2-TOOL-claim-signature-changes` | negative | `returns n: I32` against `returns n: I64` | different claims | 05: "public signatures with their binders and bounds" |
| `V2-TOOL-claim-default-changes` | negative | `labelled {by: I32 be 2i32}` against `be 3i32` | different claims | 05: "labelled defaults" |
| `V2-TOOL-claim-guard-changes` | negative | a bare `requires` added | different claims | 05: "guard predicates and consequences" |
| `V2-TOOL-claim-representation-changes` | negative | `representation @private` added to a public type | different claims | 05: "representations and their visibility" |
| `V2-TOOL-claim-impl-choice-changes` | negative | an `impl Iter` whose `type Item of U8` becomes `of U16` | the type's claim differs | 05: "implementations with their choices" |
| `V2-TOOL-claim-binding-changes` | negative | the entry's project with a policy suffix `.txt` against `.md` | the entry's claim differs; no other claim differs | 05: "changing what a target binds or permits changes its entry's claim" |
| `V2-TOOL-claim-rename-changes` | negative | `fn f` renamed `fn g` | different anchor, different claim | 02 Declared types: "Renaming or moving a declaration changes its identity" |
| `V2-TOOL-claim-omitted-row-equals-empty` | positive | `proc p` against `proc p performs {}` | equal claims | Q3 |
| `V2-TOOL-claim-omitted-representation` | positive | a public type with `representation @public` against none | equal claims | 02 Representation visibility: "When the section is omitted, the representation has the declaration's own visibility" |
| `V2-TOOL-claim-kind-type-where` | positive | `type Positive of I32 where Ordered.greater(value 0i32) endtype` | `kind: @type`, `representation`, `signature.typeInfo` `@i32`, one `@where` guard | Q3 |
| `V2-TOOL-claim-kind-interface` | positive | `interface P fn render takes {value: Self} returns t: Str endfn endinterface` | the interface's claim has no `signature`; `render` has its own claim with `receiver: 0u64` | Q3 |
| `V2-TOOL-claim-kind-case` | positive | `type Shape case circle {radius: F64} case empty endtype` | `circle`'s claim `signature.typeInfo` is the record `{radius: @f64}`; `empty`'s the empty tuple | Q3 |
| `V2-TOOL-claim-kind-module-value` | positive | `let n: I32 be 1i32 endlet` | `kind: @value`, `signature` with `binders: []` | Q3 |
| `V2-TOOL-claim-kind-test` | positive | `test "t" takes {w: TestWorld} performs {TestWorld} do endtest` | `kind: @test`, `signature.typeInfo` a `Fn` with the written row as `effects` | Q3 |
| `V2-TOOL-revision-crlf-differs` | positive | one tree, then with CRLF endings | different workspace revisions, equal claims | 05: "deliberately not canonical bytes" |
| `ATK-identity-crlf-same-claim` | negative | the host test of the row above over `Identities` | equal claims for every declaration | Q6 |
| `ATK-identity-path-independent` | negative | the same tree captured at two absolute paths, one with a space and a non-ASCII name | equal claims and equal revision | 05: "never timestamps or absolute paths" |
| `ATK-claim-record-is-schema-typed` | negative | a record built with fields out of order | the writer refuses: the canonical bytes come from the schema | 04 Canonical serialization: "a record's fields in schema order" |

## Diagnostics and schema changes

Codes: none. Schema: `@claim.v2` is defined in 05 and gains a decoder here;
no JSON form.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; the corpus gains the cases above.

## Stays unavailable

Unchanged: no command shows a claim until steps 13 and 22.

## Excluded

Witnesses (04); approvals (09); the index (22).

## Completion evidence

- [ ] `conformance/rules/m3-03.tsv` has one `active` row per case id.
- [ ] The claim vector's 436 bytes and hash are asserted by a host test
      that the report names.
- [ ] Every row of "Positions" has its positive and negative case; the
      report maps each.
- [ ] The validation commands exit 0.
