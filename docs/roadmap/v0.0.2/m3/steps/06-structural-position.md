# Step 06: structural-position

Kind: vertical. The structural record of a position query: the smallest
node that contains the offset, its kind and category, the forms and labels
the grammar permits there, with the tie rules of the chapter; the
`--include syntax` member.
Depends on: 05.
Crates: `vibra-syntax`, `vibra-workspace`, `vibra-schema`, `conformance/cases`.

## Prerequisites

- Read: [05 Structural position and semantic envelope](../../../../spec/05-tooling.md#structural-position-and-semantic-envelope)
  (the structural record); [07 Persistent observations](../../../../spec/07-diagnostics-and-conformance.md#persistent-observations)
  (`@tree.v2`, the node rule); [07 Recovery](../../../../spec/07-diagnostics-and-conformance.md#recovery);
  [questions](../spec-questions.md) Q32, Q33.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-syntax/src/cst.rs:305` | `pub struct CstNode` | the lossless tree |
| `crates/vibra-syntax/src/cst.rs:37` | `pub enum SyntaxKind` | the node kinds, including `@error` and `@missing` |
| `crates/vibra-syntax/src/cst.rs:179` | `pub const fn tree_kind` | the `@tree.v2` kind atom of a node |
| `crates/vibra-syntax/src/source/keyword.rs:86` | `pub const ADMISSIBLE_SECTIONS` | the permitted forms of a declaration position |
| `crates/vibra-syntax/src/source/keyword.rs:158` | `pub const TAG_SETS` | the permitted labels of a tag section |
| `crates/vibra-syntax/src/document.rs:53` | `pub struct Document` | the parsed document |

From step 05: the `query` command and its handler. Proposed:
`vibra_syntax::position::{at, Structural, Category}`;
`query::structural::record`, filled into the dispatcher's `syntax` entry.

## Files owned

- `crates/vibra-syntax/src/position.rs`
- `crates/vibra-syntax/tests/it/position.rs`
- `crates/vibra-workspace/src/query/structural.rs`
- `crates/vibra-workspace/tests/it/query_structural.rs`
- `crates/vibra-cli/tests/it/query_structural.rs`
- `crates/vibra-schema/schemas/v2/query/syntax.json`
- `conformance/rules/m3-06.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `position::at(document, offset)`: refuses an offset past the end or not
   on a character boundary; otherwise the smallest non-empty node with
   `start <= offset < end`, preferring a zero-width `@missing` or `@error`
   node at the offset, then the deepest, then source order; at the end of
   the file the smallest node whose `end` equals the offset, deepest
   first, else the document node (Q32). Iterative descent.
2. `Category` from the grammar slot: `module`, `declaration`, `type`,
   `pattern`, `expression`, `declarationAttribute`, `effectRow`,
   `dataField`, `trivia`, `recovery`; `status` `exact` or `recovered`
   (inside an `@error` node or a node with a `@missing` child);
   `permittedForms` from the construct tables (the sections a declaration
   still admits in order, the statement forms of a body, the data forms of
   a VIBON slot); `permittedLabels` from the tag sets, `null` where the
   grammar gives none, `[]` where known empty.
3. `record`: `{schemaVersion: 2, offset, sourceId, span, mode, syntaxKind,
   category, status, permittedForms, permittedLabels}`, the
   `--include syntax` member and its schema file; `mode` `source` or
   `data`.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| the smallest containing node | inside a token; at a boundary; at the end; in trivia; in a zero-width recovery node; against: past the end; inside a scalar |

## Test matrix

Cases are `query` with `--include syntax`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-query-position-identifier` | positive | the offset of `d` in `double(n)` | the `@identifier` leaf, `category: "expression"`, `status: "exact"` | 05: "the smallest non-empty syntax node that contains the offset" |
| `V2-TOOL-query-position-boundary` | positive | the offset where `double` ends and `(` begins | the `(` punctuation leaf, span starting at the offset | Q32 |
| `V2-TOOL-query-position-end-of-file` | positive | the offset equal to the file's length | the document node's last leaf, deepest first; `span.end` equals the offset | 05: "may be the end of the file" |
| `V2-TOOL-query-position-comment` | positive | inside `// note` | `category: "trivia"`, `syntaxKind: "@comment"` | 05: the categories include `trivia` |
| `V2-TOOL-query-position-in-recovery` | positive | the offset of a `}` that closes nothing | the `@error` node's leaf, `category: "recovery"`, `status: "recovered"` | 05: "A zero-width recovery node at the offset wins" |
| `V2-TOOL-query-position-missing-closer` | positive | the offset right after a `fn` whose `endfn` is missing | the zero-width `@missing` leaf wins over the enclosing node | the same |
| `V2-TOOL-query-position-inside-scalar` | negative | the second byte of `é` | `@command.invalidInput` | 05: "An offset outside the source or inside a character is rejected" |
| `V2-TOOL-query-position-past-end` | negative | the file's length plus one | `@command.invalidInput` | the same |
| `V2-TOOL-query-position-declaration-forms` | positive | the offset after `takes {…}` of a `fn` | `category: "declaration"`, `permittedForms` lists the sections still admissible in order (`labelled`, `returns`, …) | 05: `permittedForms` |
| `V2-TOOL-query-position-tag-labels` | positive | inside `visibility @public` | `permittedLabels: ["@public", "@private"]` | 05: `permittedLabels` |
| `V2-TOOL-query-position-data` | positive | an offset in a `project.vibon` field name, subject the project file | `mode: "data"`, `category: "dataField"`; the position subject admits the project document | 05: "`mode` (`source` or `data`)" |
| `V2-TOOL-query-position-row` | positive | inside `performs {DirReadText}` | `category: "effectRow"` | 05 |
| `V2-TOOL-query-position-no-check-implied` | positive | a module with a type error; a position in its sound part | the structural record is `exact`; the row asserts nothing semantic | 05: "Structural facts do not imply the source checked" |
| deep nesting | boundary | an offset inside 1,024 nested lists, on a 256 KiB thread | a record, no overflow | the standing rule on host recursion |

## Diagnostics and schema changes

Codes: none. Schema: the `sourcePositionQuery` member of the query schema
is produced, in `schemas/v2/query/syntax.json`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-syntax -p vibra-fmt -p vibra-resolve -p vibra-types -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
pwsh -NoProfile -File conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

Every semantic include and expansion.

## Excluded

Semantic facts (08, 12).

## Completion evidence

- [ ] `conformance/rules/m3-06.tsv` has one `active` row per case id.
- [ ] Every row of "Positions" has its positive and negative case.
- [ ] The validation commands exit 0.
