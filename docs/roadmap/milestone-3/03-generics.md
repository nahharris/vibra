# Step 3 — `any`-bounded generics

Prerequisite: Step 2 merged. Stage 3A behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Overlap and non-unifiability**,
  **Generics**, **Inference and checking**, **Functions as values**.
- [Source](../../spec/01-source-language.md): **Labels and applications**,
  **Declarations**.
- [Decision ledger](decision-ledger.md) rows D1.1, D6.1, S3.

## Scope

`where:` on `deftype`, `defn`, `lambda`, and nested methods when every bound is
`any`; inherited generic names in nested methods; applied type expressions;
invariant inference of complete argument lists from operands and written
result types; `types:` with complete lists, agreement checking, reserved
`types` label, canonical formatter position, and `@style.argument-order`; and
generic function types. The one bound-agnostic unifier lands here and is reused
by Steps 6, 11, and 13.

A bound naming any other interface is `@tool.unavailable` at its `where:` entry
until Step 11.

## Entry points

The Step 2 type model gains a generic-parameter case scoped to its declaring
entity. Inference belongs in `vibra-types`; the interpreter needs no runtime
type arguments because values carry their own shape, so erasure is the default
unless a canonical value encoding needs an argument. Formatter support for
`types:` ordering lives in `crates/vibra-fmt/src/lib.rs` and consumes resolved
binding facts, never guesses.

## Ordered tasks

1. Unifier with occurs check and a substitution type; host tests over
   unifiable and non-unifiable pairs, including generic-versus-concrete.
2. `where:` collection, `any` resolution, inherited names,
   `@name.generic-redeclaration`, and reserved-head generic names.
3. Applied types and arity checking; generic `deftype` constructors.
4. Call-site inference and `types:` checking with `@type.type-argument-mismatch`
   and `@type.ambiguous-inference`.
5. Formatter placement of `types:` and the style warning.

## Test matrix

- Positive: generic identity and pair functions inferred and with `types:`; a
  generic record constructed and projected; a method using an inherited name
  plus its own; `types:` supplying an inferable argument that agrees.
- Negative: wrong `types:` length and contradicting argument
  (`@type.type-argument-mismatch`); uninferable result-only generic
  (`@type.ambiguous-inference`); invariance (`(array i32)` against a generic
  expecting `(array t)` with `t` fixed elsewhere); redeclared inherited name;
  labelled parameter named `types` (`@name.reserved-label`); interface bound
  (`@tool.unavailable`).
- Formatter: `types:` moved before other labels, with the warning emitted only
  after a complete binding.

## Done

Inventory rows `Attribute::Where` and `TypeExpr::Applied` point at cases; M2
ledger rows C1.3 (generic part) and C6.2 are implemented; validation passes.

## Delivery notes

- Generics are erased: `Type::Param` is rigid inside its declaration and
  `Type::Applied` names an instantiated declared type. Checked IR carries no
  type arguments on calls; IR validation treats a parameter as admitting any
  type, and the checker guarantees each call is consistently instantiated.
- The unifier lives in `crates/vibra-types/src/infer.rs`. An instantiation
  renames the callee's parameters to `?index:name` variables, so they never
  collide with the caller's rigid parameters.
- Operands whose parameter type is already fixed are checked against it; the
  rest are checked alone and unified, with `lambda` operands last. When a
  written `types:` fixes the parameter an operand contradicts, the operand's
  `@type.argument-mismatch` becomes `@type.type-argument-mismatch`.
- A generic function named outside callee position is instantiated from its
  written expected `fn` type, or is `@type.ambiguous-inference`.
- Type-application arity had no diagnostic (G14); D14.1 assigns
  `@type.type-argument-mismatch`.
- An applied type adds to size through each argument its declaration stores
  directly, so `(deftype n (record inner (holder n)))` is
  `@type.infinite-size` while an argument used only under `fn` is not.
- `option` and `result` in type position stay `@tool.unavailable` until
  Steps 4 and 7 declare them.
- The reader and the source grammar rejected `where:` on `lambda` (G15). D15.1
  admits it: a `let`-bound generic lambda stays generic under quantified
  names (`t#index@site`, which no source name can spell), each call
  instantiates it, and any other use instantiates it from the expected `fn`
  type. Closures stay erased, so IR slot and capture checks use `admits`.
- The invariance matrix item uses a generic `boxed` wrapper instead of
  `(array t)`, because arrays arrive in Step 4.
- Review fixes: the unifier follows binding chains and never rebinds; a
  generic parameter no operand, result, or `types:` fixes is ambiguous even
  when the signature never mentions it; a generic lambda's `types:` list is
  its complete `where:` list; a generic value contradicting its expected type
  is a mismatch rather than ambiguity; diagnostics spell parameters as
  written; only an operand whose whole parameter type is fixed by `types:`
  becomes `@type.type-argument-mismatch`; constructors and the formatter warn
  about a late `types:` only after a complete binding; `any` and `self` are
  reserved generic names, and a function-type labelled slot named `types` is
  `@name.reserved-label`.
