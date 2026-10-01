# Step 12 — interface values

Prerequisite: Step 11d merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Interfaces and methods** (`any`
  and interface values), **Type ascription and widening** (concrete-to-
  interface widening and the no-chaining rule).
- [Runtime](../../spec/06-runtime.md): **Evaluation** (dispatch through an
  interface value).

## Scope

`any` and declared interfaces in type position; widening a concrete value to an
interface it conforms to at every written expected type the widening section
lists, applied once; dispatch of a contract member through an interface value;
unions implementing interfaces through their own `impl` blocks (never lifted
from members); and `(as I e)` ascription to an interface. An interface value
carries its receiver's implementation table; `any` values can only be passed
along. A member with another `self` parameter cannot be called through an
interface value, and an interface value satisfies no generic bound (ledger
D20.1). Dispatch through a generic interface's value, such as `(iter item)`,
arrives with Step 14.

## Test matrix

Follow the conformance chapter's widening and union paragraphs, including the
interface clauses Step 6 deferred:

- Positive: widening to an interface at each written boundary; dispatch through
  a user interface value, in one module and across an import; a union
  implementing an interface and dispatching through it; an `any` parameter
  passing a value along.
- Negative: a member value written where an interface implemented only by its
  union is expected, and an atom singleton written where `any` is expected
  (no chaining); a concrete type that does not conform; inspecting an `any`
  value; an interface value as a `defint` target, a union member, or a map
  key; an interface value at a generic bound; a member with another `self`
  parameter called through one.

## Done

The interface clause of `ExpressionKind::As` and `TypeExpr::Name` reference
cases; `V1-TYPE-GENERIC-stage-3b-types` becomes a positive case for its `any`
parameter. Validation passes.
