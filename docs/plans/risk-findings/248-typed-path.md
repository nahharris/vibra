## Pre-implementation findings

Pre-implementation investigation into whether a `try` propagation form can be
built on the typed path. The specific question — *does the compiler know the
enclosing function's return type while checking a body?* — has a clean answer:
**yes, on both paths.** That is not the blocker. The blocker is that the typed
path cannot compile `result` at all.

### What was checked

- Which lowering path compiles source today.
- Whether the enclosing function's return type is in scope at every nested
  statement position, on each path.
- What a new surface form actually touches, and whether that work is
  path-dependent.
- Whether the typed path can lower the `match` that `try` desugars into.

### 1. Enclosing return type: available on both paths

**Legacy (`src/lower.rs`).** `UserFnContext { return_type, home_module }` is
defined at `src/lower.rs:63-68`, built per-function at `src/lower.rs:2865-2868`,
and passed as `Some(&fn_ctx)` into `lower_statement` at `src/lower.rs:2880`.
`lower_statement` takes it as `fn_ctx: Option<&UserFnContext>`
(`src/lower.rs:5701,5710`) and threads it into **every** nested statement
position — if-branches (`src/lower.rs:5613`), loop bodies (`src/lower.rs:5684`),
`$let` sub-lowering (`src/lower.rs:5832`), match arm bodies
(`src/lower.rs:8869,8878`), and branch-to-body (`src/lower.rs:5484`). It is
already consumed exactly the way `try` would consume it: the `$return` arm reads
`ctx.return_type` at `src/lower.rs:5967` and does the compatibility check at
`src/lower.rs:6028-6049`, including the `E-TY-002` mismatch message and the
`E-NEWTYPE-001` coercion guard.

**Typed (`src/typed_body.rs`).** `return_type: &TypeRef` is an explicit parameter
of `validate_statements` (`src/typed_body.rs:591`), seeded from
`&signature.return_type` at `src/typed_body.rs:378` and recursed into every
nested body. The `Statement::Return` arm uses it at `src/typed_body.rs:683-693`.

So requirement #248 names as its distinguishing need is satisfied on both. Note
one asymmetry: legacy passes `None` for `main`'s top-level statements
(`src/lower.rs:2281,2443`), so `try` inside `main` must be rejected explicitly —
mirror the existing message at `src/lower.rs:5967` (*"`$return` is only valid
inside user-defined functions"*). On the typed path `main` is materialized like
any other function and does carry a `return_type` (of `Void`), so the two paths
would need the same rejection written twice, differently.

### 2. The typed path cannot host the semantics

`try` unwraps a `result`. `result` is a generic enum
(`stdlib/src/result.vib:2`: `(def result (enum (err e) (ok t)) where: (t any e any))`).
The typed executable subset rejects every ingredient:

- `ensure_safe_type` (`src/typed_body.rs:548-583`) permits only primitives,
  arrays, records, tuples, maps, refs. Enums, nominal types, interfaces and
  fn-types hit the `bail!` at `src/typed_body.rs:580`.
- generic functions rejected: `src/typed_body.rs:305`.
- generic calls rejected: `src/typed_body.rs:920` (`"typed generic calls remain staged"`).
- **`Statement::Match` bails outright**: `src/typed_body.rs:834` —
  `"typed match remains staged with enum and interface semantics"`. `try`
  desugars to a match; there is nothing to desugar into.
- `Statement::Cast` bails at `src/typed_body.rs:1358`; interface patterns at
  `src/typed_body.rs:3403`.

Measured today on this branch (`agent/vib-source-call-order`), corpus migrator
dry run:

```
surface-valid:      71/71
signature-valid:    71/71
body-valid:         22/71
materialized-valid:  6/22
```

Every stdlib-importing file — i.e. every file that could contain a `try` —
fails at the *body* tier, before materialization is even attempted. The single
cause: `(option.from-value t value)` (`stdlib/src/result.vib:63`) passes the
generic type argument as a leading positional argument, which lands in
`arguments` rather than `type_arguments`, and
`typed_body::bind_call_arguments` (`src/typed_body.rs:2490`, error at
`src/typed_body.rs:2599`) has no type-argument splitting. `tools/corpus-migrator/README.md:83`
still reports `body-valid: 57/58` / `materialized-valid: 19/57`; that is stale
and should be refreshed.

### 3. Most of the work is path-independent anyway

A new grammar form is mostly *surface* work, and none of it is tied to a
lowering path:

| work | file |
| --- | --- |
| new `ExprKind` variant | `src/ast/surface.rs:479` |
| parse into it | `src/ast/surface.rs` |
| formatter (idempotence criterion) | `src/syntax/printer.rs` |
| macro expander must not choke on it | `src/ast/typed_macro_expand.rs` |
| grammar recorded in the contract | `docs/decisions/s-expression-language.md` |

Critically, **`src/surface_adapter.rs` must handle the new form regardless of
which path implements the semantics**, because it is the live bridge
(`src/load.rs:275,297-298`; `src/main.rs:574-579`) and it fails closed —
`src/surface_adapter.rs:24-27` documents that any construct it cannot map
produces a hard `E-ADAPT-*` error rather than a guess. An unmapped `try` breaks
compilation of any file using it.

Only the desugaring itself is path-specific, and on the legacy path all the
machinery exists: `parse_match_statement` (`src/lower.rs:8800`) with enum tag
resolution and `E-MATCH-001` exhaustiveness, plus the return-type compatibility
check pattern at `src/lower.rs:6028-6049`.

### Uncertainty worth resolving before coding

The issue's acceptance criteria list *"interaction with `do` value position"*.
I could not establish what `do` in value position means today, and the two paths
disagree: the typed path bails with *"nested `do` is only valid as a statement
sequence"* (`src/typed_body.rs:2158`), while legacy `lower_branch_to_body`
(`src/lower.rs:5470-5509`) accepts a non-sequence branch by wrapping it in
`Statement::Eval`. Whether `try` is an expression usable inside `do`, or a
statement form, changes the desugaring shape substantially. **Pin this down
first** — writing a test for "interaction with `do` value position" is not
possible until the semantics of `do` in value position are stated.

Second open item from the issue that has a concrete answer here: exact
error-type match is materially cheaper. The compatibility check would reuse
`type_compatible` / `crosses_newtype_boundary` exactly as `src/lower.rs:6037-6049`
does. `into`-style conversion would additionally need interface dispatch on the
error type, which resolves statically (`src/lower.rs:6314-6353`) but rejects
generic-typed subjects with `E-DISPATCH-001` (`src/lower.rs:6321`) — meaning
conversion would not work inside generic functions, which is where propagation
is most useful. That argues for exact match, matching the philosophy document.

### VERDICT

**Must use the legacy path, but accept only *partial* rework — the surface work
is fully reusable.**

The stated blocker for this issue (enclosing return type) is not a blocker: both
paths have it, legacy via `UserFnContext` (`src/lower.rs:63,2865,2880`), typed
via the `return_type` parameter (`src/typed_body.rs:591`). The real blocker is
that the typed path cannot lower `match`, cannot type `result`, and does not
compile a single stdlib-importing file today.

Split the change accordingly:

1. Surface form — AST, parser, printer, macro expander, `surface_adapter`
   mapping, contract amendment. Zero rework at cutover; `surface_adapter` work
   is mandatory either way.
2. Desugaring — `src/lower.rs`, reusing `parse_match_statement` and the
   `ctx.return_type` check. This is the part that gets rewritten when the typed
   path lands, and it is the smaller part.

Explicitly reject `try` in `main`'s statement list (`src/lower.rs:2281,2443`
pass `fn_ctx: None`), and do not silently fall through to a confusing
"unknown form" error.
