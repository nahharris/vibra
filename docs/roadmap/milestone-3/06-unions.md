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
