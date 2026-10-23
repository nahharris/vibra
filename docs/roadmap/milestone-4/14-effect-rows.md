# Step 14 — effect declarations and rows

Prerequisite: Step 13 merged (the contracts below are frozen). Stage 4B behavior
step, static checking only: it implements `deffect`, effect rows, and the
`@effect.*` diagnostics in the checker, the typed IR, and the interpreter's
admission path. It adds no host operation and no `@host` external; the first host
operation is Step 16b.

## Read before editing

- [Effects](../../spec/03-effects.md): all of it, above all **Nominal effects**,
  **Function effect rows** (the row record, the complete row of a callee, no row
  subtyping), **Performed rows are transitively closed**, and **The witness of
  `@effect.outside-ceiling`**.
- [Source language](../../spec/01-source-language.md): the `deffect` and
  `effect-member` productions, `effect-row`, and **External definitions** (the
  `@host` attribute diagnostics).
- [Types](../../spec/02-type-system.md): **Namespaces and resolution** (effect
  namespace, member collision), the function type (rows are part of equality), and
  interface contract ceilings.
- [Runtime](../../spec/06-runtime.md): **Canonical value encoding** (the `fn`
  type gains `effects: R`).
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): **Effect and host
  diagnostics** and the effect coverage paragraph.
- [Ledger](decision-ledger.md) rows D23.1, D23.2, D24.1, D25.1, D26.1.

## Scope

1. `deffect` roots and operations with Vibra bodies and additive rows, resolved in
   the effect namespace, with canonical identities `@unit.module.root` and
   `@unit.module.root.operation`.
2. Effect rows on `defn`, `lambda`, function types, tests, interface contract
   members, and the `iter` default callbacks, as the record of D25.1: typed IR
   `EffectRow { roots, tail: None }`, sorted by canonical identity.
3. The performed row of a body over the function-call graph, with the complete
   row of a callee: an ordinary `defn`'s performed row, an operation's owner and
   additive rows, a function value's type row, and a contract member's ceiling.
   The rows of a `let` value, a `let-else` value and fallback, and a `return`
   operand join their body (inherited row I8).
4. `@effect.outside-ceiling` with the witness, `@effect.invalid-reference`,
   `@contract.unused-effect` for an unused ceiling root, and `@syntax.invalid-attribute`
   for `effects:` on a `deffect`.
5. The canonical `fn` type encoding with `effects: R`, and the reference
   interpreter and Wasm backend accepting programs with nonempty rows (they erase
   rows; neither performs a host call yet).

`@host` externals, the target `effects` array, and the query metadata are not in
this step.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-syntax/src/ast.rs` (`Declaration::Deffect`, `Attribute::Effects`) | The parsed forms; the AST is already complete |
| `crates/vibra-resolve/src/lib.rs` (`Declaration::Deffect` at the header and member passes) | Root and operation identities; the effect namespace and reference resolution live here |
| `crates/vibra-types/src/lib.rs` (the two places that report a nonempty ceiling as unavailable) | Replace the availability checks with row checking; `nominal.rs` and `defaults.rs` for declarations and defaults |
| `crates/vibra-ir/src/lib.rs` (`FunctionSignature`, `canonical_function_signature`) | Gains the row; `Type::Function` equality includes it |
| `crates/vibra-interp`, `crates/vibra-wasm` | Rows erased; the lowering must tolerate operations whose body is a Vibra body |
| `crates/vibra-workspace/src/semantic.rs` | Admission of a program with nonempty rows; the unavailable path for them is removed |

New modules are proposals: `vibra-types/src/effects.rs` (row algebra and the call
graph), and `vibra-ir`'s `EffectRow`.

## Ordered tasks

1. Add `EffectRow { roots, tail: None }` to `vibra-ir` with canonical ordering and
   the VIBON text `(record roots: (array …))`; make it part of the function type,
   its equality, and `canonical_function_signature`. Regenerate the snapshots that
   change (`types.vibon` of the two type cases, and the signatures of the index
   cases) and review each diff: the only change is `effects: (record roots:
   (array))` after `result`.
2. Resolve `deffect`: roots, operations, the flat member namespace
   (`@name.member-collision`), `visibility:`, and `@syntax.invalid-attribute` for
   `effects:` on a `deffect`.
3. Resolve effect rows: unqualified and dotted symbols, `@effect.invalid-reference`
   for an atom or an unresolved symbol, duplicate entries, and sorting by identity.
4. Build the function-call graph once, in `vibra-types`, with the four kinds of
   edge of D24.1, and compute performed rows to a fixed point (recursion and mutual
   recursion), then check ceilings: ordinary bodies, operations (owner plus
   additive), contract members within their ceiling.
5. Emit the witness: one diagnostic for each offending function and missing root,
   primary span the first qualifying call site, related spans the shortest chain,
   ties in source order, hops through function values and contract members marked.
6. Unused declared roots: `@contract.unused-effect` at the ceiling atom.
7. Remove the `@tool.unavailable` checks for nonempty rows, `deffect`, and the
   effect-bearing function types; replace the `V1-EFFECT-availability-*` cases with
   positive and negative cases.
8. Lower and run programs with nonempty rows in both backends (rows are erased) and
   move their executable cases to `matched` in `conformance/parity.tsv`.

Invariants preserved: the performed row never changes an omitted ceiling; a row is
a record in every representation; checker and later queries share one graph;
creating a function value performs nothing; no root absorbs another.

## Test matrix

- Positive: an operation with a Vibra body and an empty additive row; an operation
  with an additive row and its transitive callers; a caller that lists both
  roots; mutual recursion; a function-value parameter with a written row; a
  `lambda` with a row; a contract with a ceiling and an implementation within it;
  a `let` value, a `let-else` fallback, and a `return` operand that perform.
- Negative: the example of the effects chapter (a caller naming only the owner
  root) with its witness spans; two missing roots giving two diagnostics; a
  witness through three hops, through a function value, and through a contract
  member; two chains of different length and of equal length (the shortest, then
  source order); a function value of one row where another is expected, both ways;
  an atom in a row; an unknown symbol; `effects:` on a `deffect`; an implementation
  over its contract ceiling; a duplicate root in a row.
- Recovery: after a ceiling error the next declaration still checks, and a callee
  that is itself offending does not cascade into its callers (callers see its
  performed row, not an error).
- Boundary: a row of many roots from several packages (sorted by canonical
  identity), textually equal roots from different packages as different roots,
  a deep call chain (the witness is found without recursion on the host stack).
- Formatter: row order in source is by resolved identity, each lexical reference
  preserved; a row across lines; idempotence over the new cases.

## Diagnostic and schema changes

No new code (the registry has every code this step emits). Schema: the typed-IR
canonical form and the canonical `fn` type encoding gain the row; the
position-query type object and the index change in Step 20, which must keep the
existing cases valid until then.

## Validation

The [pre-merge list](validation.md#before-merging-each-step) and `cargo test
--locked --offline -p vibra-resolve -p vibra-types -p vibra-ir -p vibra-interp -p
vibra-fmt`, then `-p vibra-workspace -p vibra-conformance`. Record the corpus counts
per profile and per backend; the parity inventory must show every new executable
case `matched`.

## Excluded

`@host` externals (16b), the target array (15), effectful tests (15), metadata
(20), row variables and sealed roots (post-v1), any effect inference.

## Completion evidence

The witness cases of the effects chapter and the exit-gate clause "an additive root
reaches the performed row and the rejection of every transitive caller" pass at the
checker; the unavailable cases are replaced, not deleted.

## As built

Landed as `landed`, conditional on its pull request merging; the base is Step 13
(PR #371, merge `437e928` on `m4`), on which the branch is rebased. The step is
one pull request over the crates below.

- **The row** (`vibra-ir`, `effect.rs`). `EffectRoot { path, id }` is the canonical
  atom path (`unit.module.root`, rendered with `@` in VIBON) with the canonical
  declaration identity, which includes the package, so textually equal roots of two
  packages differ and the derived order is the UTF-8 order of the path, then the
  identity. `EffectRow { roots, tail }` keeps `roots` sorted and duplicate-free and
  `tail` as `Option<RowTail>` with `RowTail` an empty enum, so v1 has no row
  variable and a later one is one more variant. `FunctionSignature` carries the row;
  equality, `same_shape`, `admits`, the inference unifier, `Display`, and
  `canonical_type` all include it, and the encoding writes `effects: (record roots:
  (array @root...))` after `result`, empty included. `CheckedFunction` gains
  `effect_owner`, the root an operation belongs to; an operation's signature row is
  the owner with its additive row, which is the complete row a caller sees.
- **Resolution** (`vibra-types`, `nominal.rs`). An effect root is declared in
  `TypeNames` beside the types, so both checkers share it. A row symbol resolves
  in the current module, through a module alias (`log.write`), or through a
  declaration alias (`audit` after `(import audit @ops.log.audit)`); anything else,
  including a function or a type of that spelling, is `@effect.invalid-reference` at
  the entry. A private root of another module is `@name.private-access` at the
  entry. Entries carry their own spans (`EffectRow::entry_spans`,
  `FunctionType::effects` as `EffectReference`s whose `TypeSpan` takes no part in
  equality). `vibra-resolve` only gives roots and operations their identities
  (`EntityKind::Effect` and `Operation`), makes an operation as visible as its root,
  and reports a root used as a value as `@name.wrong-entity-kind`. An atom in a row
  is still reported by the parser, and `effects:` on a `deffect` is
  `@syntax.invalid-attribute` at the attribute (a new `AttributeContext::Effect`).
- **The graph** (`vibra-types`, `effects.rs`). One pass over the checked functions
  and module-value initializers of the typed IR, run by both checkers after every
  body is checked. Nodes are functions, operations, tests, initializers, and each
  `lambda` (its body is its own node, so creating it performs nothing); an
  application is a site classified by its target: a `Direct` call (the callee's
  performed row, or the owner with the additive row for an operation), an `Indirect`
  call (the row of the callee expression's type), or a `Contract` call (the member
  signature's row, which is the ceiling). Performed rows close by a worklist, so
  recursion and mutual recursion converge, and an operation does not propagate its
  body. A function that did not check keeps its declared signature, which is all a
  caller needs of an operation, and marks every node that reaches it as partial, so
  no "never performed" warning is given where the row is only a lower bound.
- **The witness.** For each pair of an offending node and a root missing from its
  ceiling, in canonical root order, one diagnostic: the primary span is the first
  application, in source order, whose callee's complete row holds the root (noted
  `performs <root> via <callee>`), and each related span is a further hop, noted the
  same way, from the shortest chain by hops to the declaration that introduces the
  root, the earliest application first among equal chains. The chain stops at a
  function value, a contract member, or an operation with no application in its
  body that carries the root. The hop counts come from a breadth-first search over
  the reverse call edges, one per missing root, so a chain of 3,000 functions is
  searched without host recursion (`a_deep_call_chain_is_searched_without_host_recursion`).
  A callee is named by its canonical atom path without the package. The diagnostics
  of the pass are sorted by source position and are stable, so those of one function
  with one primary span keep the canonical order of their roots. The workspace
  checkers sort the whole list by primary span, as they always did.
- **Unused roots.** `@contract.unused-effect` at each written entry the body never
  performs, with the same node granularity (a `lambda` has its own warning).
- **The formatter.** A row is written in the canonical order of its roots. The
  checker keeps the order of each written row (`CheckResult::effect_row_orders`,
  `EffectRowOrder`), and `format_source_with_facts` permutes the entry texts within
  their slots before the usual passes, as the structural pass does for anonymous
  types, so every lexical symbol is kept and no outside span moves; a row with a
  comment, or whose fact does not fit, is left as written. `plan_format` supplies the
  facts under the same authorization as binding facts, which is a standalone
  `check_source` of the document, so a row in a module that imports another module
  is not reordered by `vibra fmt`.
- **Both backends.** Nothing changed in the interpreter or the emitter. An
  operation with a Vibra body is an ordinary function of the module, a row is part
  of a type that neither consults, and every executable case matched in Wasm the
  first time it ran.

### Rulings on text the specification was silent about

The maintainer ruled on four readings, and each is now canonical prose in
[the effects chapter](../../spec/03-effects.md):

- A root named twice in one written row is `@effect.duplicate-root` at the second
  entry, relating the first, in a `defn`, `lambda`, function type, test, and
  operation row; the code replaced `@project.duplicate-effect-root` outright and
  also serves a target `effects` array (`V1-EFFECT-duplicate-root-in-row`,
  `a_root_named_twice_in_a_row_is_a_duplicate_at_the_second_entry`,
  `a_duplicate_is_reported_in_every_kind_of_written_row`).
- An implementation member writes exactly its contract member's row, because
  `check_member_signature` compares `same_shape`; another row is `@type.mismatch`,
  and "within the contract ceiling" is the ordinary check of the body against that
  row (`V1-EFFECT-impl-member-row-mismatch`).
- A `def` initializer has the empty ceiling, so applying an operation in it is
  `@effect.outside-ceiling` (`V1-EFFECT-def-initializer-ceiling`).
- An operation is a declaration with its own visibility: private unless it writes
  `visibility: @public`, whatever its root's (`V1-EFFECT-private-operation-import`,
  `V1-EFFECT-public-operation-import`). Every cross-module operation of the corpus
  and of the host tests writes it.

Two readings remain implementation facts:
- The formatter reorders a row only for a file that checks standalone, which is
  the limit binding facts already have; a module that imports another, and a test
  module, keep the written order.
- An unresolved row entry makes the function's signature unavailable, so its body
  is not checked and no ceiling diagnostic follows it.

### Cases

- **Added, static (30):** `V1-EFFECT-function-type-row`, `-function-type-unresolved`,
  `-witness-chapter-example`, `-additive-root-accepted`, `-missing-two-roots`,
  `-witness-three-hops`, `-witness-function-value`, `-witness-contract-member`,
  `-witness-shortest-chain`, `-witness-equal-chains`, `-function-value-row-mismatch`,
  `-atom-in-row`, `-unknown-symbol-in-row`, `-deffect-effects-attribute`,
  `-contract-ceiling-within`, `-contract-ceiling-over`, `-unused-ceiling-root`,
  `-offending-callee-no-cascade`, `-recursion-closes`, `-let-else-return-rows` (single
  source), and `-imported-root-witness`, `-imported-roots-accepted`,
  `-private-root-import`, `-equal-names-different-modules`, `-private-operation-import`, `-public-operation-import` (workspace), and `-duplicate-root-in-row`, `-impl-member-row-mismatch`, `-def-initializer-ceiling` (single source).
- **Added, executable (4, all `matched`):** `V1-RUNTIME-effect-operations`,
  `V1-RUNTIME-effect-function-values`, `V1-RUNTIME-workspace-run-effect-operations`,
  `V1-RUNTIME-workspace-test-effect-operation`. **Added, rejected:** `V1-EFFECT-test-ceiling`.
  **Added, tooling:** `V1-TOOL-format-effect-row-order`.
- **Replaced:** `V1-EFFECT-availability-function-type` (a valid function-type row was
  `@tool.unavailable`) by `V1-EFFECT-function-type-row` and
  `V1-EFFECT-function-type-unresolved`. `V1-EFFECT-availability-empty-row` stays.
- **Re-expected:** `V1-EFFECT-availability-host-member` (the span is the member and not the
  `deffect`, and the two checkers now agree, so it left the expected differences of
  `check_paths_agree`), `V1-PROJECT-workspace-check-nominal-availability` and
  `V1-TOOL-index-unavailable` (the diagnostics at the `deffect` and its resolver stage
  are gone; the member's remains), `V1-SRC-CALLS-functions-effects` (the unresolved
  `io.stdout` is an invalid reference, not unavailable), and
  `V1-TYPE-INTERFACE-iter-default-rejections` (the effectful lambda now names a declared root,
  and the rejection is the argument mismatch against the callback's empty row, which
  also required the unifier to compare rows).
- **Re-expected snapshots (6, 21 function types):** the `fn` type encodings of
  `V1-SRC-CALLS-functions-labelled/types.vibon` (2),
  `V1-TYPE-INTERFACE-contract-member-types-ir/types.vibon` (3),
  `V1-TOOL-index-member-order/index.vibon` (4), `V1-TOOL-index-records/index.vibon` (9),
  `V1-TOOL-index-test-references/index.vibon` (2), and
  `V1-TOOL-index-unavailable/index.vibon` (1). Each type gained ` effects: (record roots:
  (array))` before its closing parenthesis, by a script that matched the parentheses
  of each `(record type: @fn ...)`, and the corpus run, which compares the real
  encodings, accepted every result.
- **Tests re-expressed:** the tests that used `effects: (io.stdout)` or a lambda with
  `effects: (audit)` as their example of an unavailable form (`functions_step7`,
  `process_review`, `process_step13`, `test_runner_step13`, and `resolution_step4`)
  now use a `@host` operation, the one construct that remains unavailable.
  `docs/roadmap/milestone-1/syntax-examples.tsv` follows the line shifts of the
  edited chapters: 51 rows changed, only in their line numbers, digests, and, for
  the five sections whose text changed, their inline counts.

### Evidence

- Corpus at the head: 83 reader, 267 static, 143 interpreter, and 16 tooling cases,
  509 passed, 0 failed, 0 unavailable; the interpreter backend passes 135 cases and
  the Wasm backend matches 135, 0 not lowered (131 at the base). The parity inventory
  holds the four new executable cases as `matched`.
- Host tests: `effects_m4_step14` in `vibra-ir` (7), `vibra-types` (38), `vibra-fmt` (6),
  and `vibra-conformance` (2).

### Left for Step 15

The target `effects` array is not checked: an entry whose written ceiling is nonempty
is admitted whatever the array holds, and `V1-RUNTIME-workspace-run-effect-operations`
lists the canonical atoms its entry needs so that the array is already the
closure of the entry graph when Step 15 starts to check it. The array's atoms are
not resolved, effectful tests have no consent rule beyond their own ceiling, the
entry's witness at the `entry:` value is not written, and `@host` externals,
the position-query and index effect facts, and the standard effect modules wait for
Steps 16 to 20.
