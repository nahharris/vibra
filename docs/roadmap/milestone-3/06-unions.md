# Step 6 — unions, widening, and `as`

Prerequisite: Step 5 merged. Stage 3A behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Overlap and non-unifiability**,
  **Nominal declarations** (unions), **Type ascription and widening**,
  **Control flow and failure** (`as` patterns).
- [Source](../../spec/01-source-language.md): **Type ascription and narrowing**.
- [Runtime](../../spec/06-runtime.md): **Evaluation** (discriminants, erased
  `as`), **Canonical value encoding** (unions).
- [Conformance](../../spec/07-diagnostics-and-conformance.md): union,
  widening, ascription, and narrowing coverage paragraphs.

## Scope

Declared and anonymous unions with the too-few, overlap (Step 3 unifier),
and concreteness checks, order-insensitive anonymous identity, and the declared
union constructor `(z f)`; atom singleton types; member-to-union and
singleton-to-`atom` widening at exactly the written expected types the chapter lists, applied once; `as`
ascription (no-op, widening, inference constraint); `as` narrowing patterns in
the Step 5 engine with union members as a constructor space. Interface widening
is Step 12.

## Test matrix

Follow the chapter's union, widening, ascription, and narrowing coverage
paragraphs exactly, minus their interface clauses, which Step 12 owns. In
particular: `(union (array t) (array i32))` rejected even with a bound; `if`
branches `i32`/`f32` rejected without a written union; `(array i32)` not
widening to `(array number)`; `(array.of @ok @err)` rejected and its `as`
spelling accepted; `(as i64 3i32)` and `(as i32 some-number)` rejected; `as`
patterns in `let` and parameters rejected; non-union scrutinee and non-member
type rejected. An interpreter case shows `as` lowers to its operand alone.

## Done

Inventory rows `DeftypeBody::Union`, `ExpressionKind::As`, and
`PatternKind::As` reference cases; M2 row C1.7 is implemented; validation
passes.

## Delivery notes

- Unions lower to `Type::Union` (anonymous, members sorted by canonical type
  encoding) and `TypeBody::Union` (declared, declaration order). Both orders
  fix the discriminants. `crates/vibra-types/src/union.rs` checks members:
  another union (declared or anonymous) or a bare generic name is
  `@type.union-member-not-concrete`, and two members unifiable under some
  substitution, regardless of bounds, are `@type.union-member-overlap`.
  Declared bodies are rechecked once every body exists, so a member naming a
  later declared union is caught. Interface members wait for Step 12, when
  interfaces become types.
- A written atom has the singleton type `Type::AtomSingleton`, encoded
  `(record type: @atom-singleton atom: @name)` (the runtime chapter now
  lists it). Singleton types are never written; they only widen to `atom`.
  A singleton scrutinee is closed by its one atom arm.
- Widening is one IR node, `Expr::Widen`, with a discriminant for a union and
  none for atom widening, which the interpreter erases. The checker widens in
  `check_expression_in_position`: when the expected type is `atom` or a
  union, the operand is checked without that expectation and then widened
  once. `if`, `match`, `let`, and `do` pass the expectation to their results
  instead, so each branch widens where it ends. Widening never chains: a
  singleton does not reach a union containing `atom`.
- A declared union's constructor `(z f)` injects exactly one member value,
  inferring a generic union's arguments from the member that unifies.
  Constructor patterns do not name unions: narrowing is only `(as t p)`.
- `(as t e)` checks the operand at `t` and is erased: the typed IR holds the
  operand, plus the boundary's `Widen` when it widens
  (`V1-TYPE-CONVERT-ascription-erased`). A mismatch at the operand becomes one
  `@type.invalid-ascription` at the form.
- `as` patterns are a union constructor space in the Step 5 engine. The
  uncovered-shape note spells a member as `(as t -)`.
- Limitation: labelled defaults are still M2's reviewed literal defaults,
  stored as plain values, so a labelled parameter of union type cannot have a
  default until defaults become expressions.
