# Step 04: witness-identity

Kind: vertical. Recursive groups over resolved references, the
`@witness.v2` record, and the witness identity of every declaration; the
05 witness vector; a dependency's change reaches every dependent and no
other.
Depends on: 02.
Crates: `vibra-workspace`, `conformance/cases`.

## Prerequisites

- Read: [05 Snapshot, claim and witness identity](../../../../spec/05-tooling.md#snapshot-claim-and-witness-identity)
  ("A `@witness.v2` record covers one recursive group", the vector);
  [architecture](../architecture.md) rows I3, "The theories";
  [questions](../spec-questions.md) Q2, Q4, Q5.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-resolve/src/snapshot.rs:151` | `pub struct ResolvedReference` | the edges of the reference graph |
| `crates/vibra-resolve/src/snapshot.rs:54` | `pub struct ResolvedDeclaration` | the nodes, with their spans |
| `crates/vibra-resolve/src/lib.rs:119` | `pub fn resolve` | the resolution the engine already has |

From step 02: `vibra_fmt::fragment::canonical_text`.

Proposed: `identity::witness::{Group, groups, WitnessRecord, witnesses}`;
`Identities::witness` filled.

## Files owned

- `crates/vibra-workspace/src/identity/witness.rs`
- `crates/vibra-workspace/tests/it/witness.rs`
- `conformance/rules/m3-04.tsv`
- the case directories of the matrix below

## Ordered tasks

1. The reference graph: a node per declaration of every unit (`@std` and
   dependency packages included); an edge from the declaration whose span
   contains a reference to the declaration it resolves to (Q5). Members
   are nodes of their own and their references are also their owner's
   (Q4). Invariant: built in one pass over `Resolution::references`.
2. `groups`: strongly connected components by an iterative Tarjan with an
   explicit stack, components numbered in reverse topological order so a
   dependency's witness exists before its dependent's. Invariant: the
   component graph has no cycle, asserted after construction.
3. `WitnessRecord`: `members` sorted by path spelling, each `{path,
   declaration, body}` with `body` the canonical text of the whole
   declaration; `dependencies` sorted by `declaration`, one per
   declaration outside the group a member resolves to, with that group's
   witness. Canonical bytes through the schema writer; identity = `sha256:`
   of `vibraWitnessIdentityV2`, a zero byte, the bytes. Invariant: the
   vector hashes to `sha256:241d…5e33` with 234 bytes.
4. `witnesses` in component order, with a map from declaration to its
   group's witness; every member shares it.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| a witness follows dependencies | direct callee; a type in a `let` annotation; a `@std` declaration; a dependency package declaration; a signature reference; against: an unrelated declaration |
| a group is one unit | mutual recursion of two `fn`s; a self-recursive `proc`; against: two independent declarations |

## Test matrix

Corpus rows are `workspaceCheck` trees read by the host test
`witness::corpus_witnesses` as in step 03.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-witness-vector` | positive | the tree of `V2-TOOL-claim-vector` | `main`'s record has one member with the vector's `body` and `dependencies: []`, 234 canonical bytes, hash `sha256:241dbc…5e33` | 05: the witness vector |
| `V2-TOOL-witness-dependency-change` | negative | `fn f` calls `fn g`; `g`'s body changes | `g`'s and `f`'s witnesses change | 05: "a change to the body of anything a declaration resolves to, however far, changes the declaration's witness" |
| `V2-TOOL-witness-transitive` | negative | `f` calls `g` calls `h`; `h` changes | all three change | the same, "however far" |
| `V2-TOOL-witness-unrelated-same` | positive | `f` and `k` unrelated; `k` changes | `f`'s witness is equal | 05: "A declaration that is not recursive is a group of one" |
| `V2-TOOL-witness-group-acyclic` | positive | `even` and `odd` mutually recursive | one group with two members, equal witness, `dependencies` without each other | 05: "a cycle of references is one group" |
| `ATK-witness-no-self-dependency` | negative | the host test over the component graph of every corpus tree | no group lists its own witness; the component graph is acyclic | 05: "so no witness depends on itself" |
| `V2-TOOL-witness-member-edit-changes-owner` | negative | `type Box` with member `twice`; `twice`'s body changes | `twice`'s and `Box`'s witnesses change | Q4 |
| `V2-TOOL-witness-member-own-group` | positive | the same tree | `twice` is a member of its own group with its own record | Q4 |
| `V2-TOOL-witness-std-reference` | positive | `fn f takes {s: Str} returns n: U64 do return Str.length(s) endfn` | `f`'s `dependencies` names `Str.length`'s declaration with its witness | Q5 |
| `V2-TOOL-witness-dependency-package` | positive | a `@path` dependency `money` whose `cents` is called | the caller's `dependencies` names `cents`; a change in `cents` changes the caller | Q5; 04 Dependencies and lock |
| `V2-TOOL-witness-signature-reference` | positive | `fn f takes {u: User} …`; `User`'s declaration text changes | `f`'s witness changes | Q5 |
| `V2-TOOL-witness-comment-same` | positive | a comment added in a body | equal witness | Q2 |
| `ATK-witness-deep-chain` | boundary | a chain of 20,000 declarations each calling the next, on a 256 KiB thread | witnesses computed, no overflow | architecture, risk 6; the standing rule on host recursion |

## Diagnostics and schema changes

Codes: none. Schema: `@witness.v2` gains a decoder; no JSON form.

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

Read: all exit 0.

## Stays unavailable

Unchanged.

## Excluded

Claims (03); the index (22).

## Completion evidence

- [ ] `conformance/rules/m3-04.tsv` has one `active` row per case id.
- [ ] The report quotes the component algorithm's loop and states it uses
      no host recursion.
- [ ] The vector's 234 bytes and hash are asserted by a named host test.
- [ ] The validation commands exit 0.
