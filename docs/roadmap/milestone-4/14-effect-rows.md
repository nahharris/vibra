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
