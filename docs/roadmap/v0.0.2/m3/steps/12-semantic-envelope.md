# Step 12: semantic-envelope

Kind: vertical. The semantic members of a query: `context`, `source`,
`signature`, `type`, `visibility`, `diagnostics`, for a position and for an
entity, projected from the checker's facts, the resolution, and the
diagnostics, each field `exact`, `recovered`, or `unavailable` on its own.
Depends on: 05, 06, 08.
Crates: `vibra-workspace`, `conformance/cases`.

## Prerequisites

- Read: [05 Structural position and semantic envelope](../../../../spec/05-tooling.md#structural-position-and-semantic-envelope)
  whole; [07 Recovery](../../../../spec/07-diagnostics-and-conformance.md#recovery);
  [06 Open signatures](../../../../spec/06-runtime.md#open-signatures);
  [architecture](../architecture.md) row I18; [questions](../spec-questions.md) Q32, Q33.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/check/mod.rs:66` | `pub struct CheckResult` | `facts()` of step 08 |
| `crates/vibra-resolve/src/snapshot.rs:180` | `pub struct VisibleLocals` | `visibleLocals` |
| `crates/vibra-resolve/src/snapshot.rs:108` | `pub struct ResolvedImport` | `visibleImports` |
| `crates/vibra-ir/src/encode.rs:340` | `pub fn encode_open_type` | the type document |
| `crates/vibra-types/src/check/obligation.rs:62` | `pub struct Obligation` | `obligations` |
| `crates/vibra-schema/src/diagnostic.rs:144` | `pub fn render` | the diagnostics member |

Proposed: `query::semantic::{envelope, entity_metadata}`.

## Files owned

- `crates/vibra-workspace/src/query/semantic.rs`
- `crates/vibra-workspace/tests/it/query_semantic.rs`
- `crates/vibra-cli/tests/it/query_semantic.rs`
- `conformance/rules/m3-12.tsv`
- the case directories of the matrix below

## Ordered tasks

1. For a position: the node's fact (08) gives `role`, `context`,
   `identity`, `expectedType`, `observedType`, `application`, `pattern`,
   each as `{status, value}`; the resolution gives `visibleLocals` (lexical
   order), `visibleImports` (written order), `declarationCandidates`
   (identity order, private ones excluded where not visible); the check's
   obligations whose span contains the position give `obligations`. A
   missing fact is `unavailable`.
2. For an entity: `signature` (the signature record), `type` (the open
   type), `visibility` `{access, representation}`, `source` `{span,
   text}`, `diagnostics` (those whose primary span lies in the
   declaration), `context` with the declaration's `kind` and `module`.
3. Types as the JSON projection of the open type encoding, binder
   references indexing the enclosing signature's binders.
4. The members placed as Q33 says; the default compact metadata unchanged.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| a semantic fact has its own status | an exact node; a node in a body with an error; a node in a module beside a broken module; against: a node whose type the checker could not fix |

## Test matrix

Cases are `query` under `toolingV2`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-TOOL-query-type-at-literal` | positive | `--include type` at `21i32` in `double(21i32)` | `expectedType {status: exact, value: "@i32"}` written false, `observedType` `@i32` | 05: "expected and observed types, whether the expected type was written" |
| `V2-TOOL-query-type-written` | positive | at the initializer of `let x: I32 be 1i32 endlet` | expected written true | the same |
| `V2-TOOL-query-context-locals` | positive | `--include context` after two `let`s in one sequence | `visibleLocals` lists both, in lexical order; inside the second's initializer only the first | 05 Locals: "The visible locals at a position are the earlier successful `let`s of the same sequence" |
| `V2-TOOL-query-context-let-else` | positive | inside a `let … else` fallback | the `let`'s own names absent | 05: "a `let`'s own names are not visible in its initializer or its `else`" |
| `V2-TOOL-query-context-imports` | positive | a module with three imports | `visibleImports` in written order | 05: "imports in written order" |
| `V2-TOOL-query-context-candidates` | positive | a position in a module importing a module with a private declaration | the private declaration is no candidate | 05: "A private entity is not a candidate where it is not visible" |
| `V2-TOOL-query-role-by-slot` | positive | `@std.io` in an expression and in a `use` | `@atomValue` and `@entityReference` | 05 Roles |
| `V2-TOOL-query-discard` | positive | at `_` | role `@discard`, `identity: null` | 05: "`_` has the discard role and no identity" |
| `V2-TOOL-query-local-identity` | positive | at a use of `x` | `identity {kind: "@localBinding", canonical: "binder:src/app/main.vib:<s>-<e>"}` | 05 Identities |
| `V2-TOOL-query-application-fact` | positive | `--include signature` at `double(n)` | the application fact with kind `@function`, operands in written order | 05 Application fact |
| `V2-TOOL-query-application-plan` | positive | at `log(x)` where `log` is a `proc`, and at the `await` | `@procPlan`; the `await` reported separately with the plan's row | 05: "An `await` is reported separately" |
| `V2-TOOL-query-pattern-fact` | positive | at `Shape.circle(radius: r)` in a `case` | `{kind: @variant, scrutineeType, bindings, constructor}` | 05 Pattern fact |
| `V2-TOOL-query-obligations` | positive | at a call of a `fn` with a bare `requires` | one obligation record with its status and rules | 05 Obligation record |
| `V2-TOOL-query-entity-signature` | positive | `@app.math.double --include signature,visibility,source` | the signature record; `{access: "@private", representation: null}`; the span and text | 05: "signature", "visibility" |
| `V2-TOOL-query-entity-type-record` | positive | a public type with a private representation | `visibility {access: "@public", representation: "@private"}` | 02 Representation visibility |
| `V2-TOOL-query-entity-diagnostics` | positive | `--include diagnostics` on a declaration with a warning | the diagnostic rendered | 05: "diagnostics and fix capability" |
| `V2-TOOL-query-semantic-beside-broken-module` | positive | a workspace with a type error in module B; a position in sound module A | every fact `exact` | 05: "A complete fact stays `exact` beside an unrelated broken module" |
| `V2-TOOL-query-recovered-fact` | positive | a position in a body with a type error | `observedType.status: "recovered"` | 05: "An incomplete fact is `recovered`" |
| `V2-TOOL-query-unavailable-in-trivia` | positive | `--include type` in a comment | `{status: "unavailable", value: null}` | 05: "`unavailable` (with a `null` value)" |
| `V2-TOOL-query-open-type` | positive | inside `fn first(T) takes {xs: List(T)}` at `xs` | `observedType` is `{kind: @list, arguments: [{kind: @binder, index: 0}]}` | 05 Type document: "whose binder references index the binders of the enclosing declaration's signature record" |
| `V2-TOOL-query-declared-not-expanded` | positive | at a `User` value | `{kind: @declared, path, seed, arguments: []}`, no fields | 05: "A declared type is referenced by its path and identity, never expanded" |
| JSON validates | positive | every envelope | validates against the query schema with the `workspacePositionQuery` `$def` | 05 |

## Diagnostics and schema changes

Codes: none. Schema: the `workspacePositionQuery` `$def` is produced.

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

`effects` (15); every expansion (15, 16); `index` (22).

## Excluded

Effects; expansions; the index.

## Completion evidence

- [ ] `conformance/rules/m3-12.tsv` has one `active` row per case id.
- [ ] Every row of "Positions" has its case.
- [ ] The report shows that no function of `semantic.rs` calls the checker
      or the resolver: it reads results.
- [ ] The validation commands exit 0.
