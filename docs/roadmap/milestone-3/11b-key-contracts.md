# Step 11b — key contracts and the library map

Prerequisite: Step 11 merged. Stage 3B behavior step, split from Step 11 so
that the interface machinery lands before the standard library depends on it.

## Read before editing

- [Types](../../spec/02-type-system.md): **Nominal declarations** (map keys and
  the key contracts), **Generics** (interface bounds).
- [Decision ledger](decision-ledger.md) rows D17.1 and D18.2.

## Scope

1. **Key contracts.** `@std.core` declares `equatable` and `ordered`. The
   closed key conformances (the key primitives and anonymous structures of
   them) become ordinary implementations in the standard library.
2. **Bounds beyond `defn`.** Interface bounds on `deftype` parameters and on
   generic `lambda` parameters, which Step 11 still reports as
   `@tool.unavailable`.
3. **The library map.** `map` moves into the standard library as a `deftype`
   over a sorted array of entries that claims `@map`, with `map.of` and lookup
   written in Vibra with native implementations. A map keyed by a generic
   parameter needs `(k ordered)`, and a user `deftype` key needs its own
   `ordered` implementation.

## Test matrix

- Positive: a user record keyed in a map through its own `ordered`; a generic
  function over `(map k v)` with `where: (k ordered)`; a bounded `lambda` and a
  bounded `deftype`; `map.of` and lookup matching their natives.
- Negative: a user key without `ordered`; a map keyed by a generic parameter
  with no `ordered` bound.

## Done

Inventory rows `TypeExpr::Map` (generic keys) and the remaining interface-bound
clause of `Attribute::Where` reference cases, the body/native harness covers the
map rows, and validation passes.
