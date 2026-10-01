# Step 11c — user keys and bounds beyond `defn`

Prerequisite: Step 11b merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Nominal declarations** (dict keys,
  canonical key order), **Generics** (interface bounds).
- [Decision ledger](decision-ledger.md) rows D18.2, D19.2, and D19.3.

## Scope

1. **User and generic keys.** A `deftype` is an admissible key through its own
   `ordered` implementation, and so is a parameter bounded by `ordered`, alone
   or inside a key structure. Without either, the key is
   `@type.invalid-dict-key`, whether the dict type is written or inferred.
2. **Ordering by `compare`.** A dict construction or lookup whose key type is
   not purely closed records the `ordered` interface in checked IR
   (`key-order:`), which makes every `compare` implementation a dependency of
   its function. The interpreter then orders, finds, and compares keys through
   a declared type's own `compare`, component-wise through structures, and by
   canonical key order otherwise. A closed `ordered.compare` on a structure
   holding a user key does the same.
3. **Bounds on `deftype` and `lambda`.** A bounded `deftype` parameter is in
   scope as bounded in the type's body, methods, and `impl` members, and every
   application of the type must satisfy it: in a signature, a module value, a
   type body, a written type in a body, and a constructor's inferred type. A
   bounded generic `lambda` checks its bound at each application, including
   through a `let` binding.

## Test matrix

- Positive: a dict keyed by a user record ordered unlike its fields, with its
  lookups, a tuple key holding it, and a bounded generic instantiated with it;
  a bounded `deftype` keying a dict by its parameter; a bounded `lambda`.
- Negative: a user key without `ordered`, written and inferred, and an
  `ordered`-bounded parameter instantiated with it; a bounded `deftype` applied
  to an unsatisfying type at each check point; a bounded `lambda` applied
  likewise.

## Done

The `TypeExpr::Dict` and `Attribute::Where` rows reference the cases, and
validation passes.
