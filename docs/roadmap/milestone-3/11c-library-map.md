# Step 11c — the library map

Prerequisite: Step 11b merged. Stage 3B behavior step, split from Step 11b
because a map ordered by a user `compare` needs that call in Vibra.

## Read before editing

- [Types](../../spec/02-type-system.md): **Nominal declarations** (map keys,
  canonical key order), **Generics** (interface bounds).
- [Runtime](../../spec/06-runtime.md): **Native implementations**,
  **Representation latitude**.
- [Decision ledger](decision-ledger.md) rows D17.1 and D18.2.

## Scope

1. **Bounds beyond `defn`.** Interface bounds on `deftype` parameters, checked
   at every application of the type, and on generic `lambda` parameters, which
   Step 11b still reports as `@tool.unavailable`.
2. **The library map.** `map` moves into the standard library as a `deftype`
   over a sorted array of entries, with `where: (k ordered v any)`, that claims
   `@map`. `map.of` and lookup are Vibra calling `ordered.compare`, with native
   implementations that agree with them.
3. **User and generic keys.** A `deftype` is an admissible key through its own
   `ordered` implementation, which orders its map, and so is a parameter
   bounded by `ordered`. A key structure holding such a parameter conforms
   through it, which Step 11b still rejects as `@type.unsatisfied-bound`.
4. **Library conformances.** The closed key conformances for the core and
   library types become ordinary implementations in the standard library,
   which needs library implementations to join every checking run.

## Test matrix

- Positive: a user record keyed in a map through its own `ordered`; a map
  keyed by an `ordered`-bounded parameter instantiated with that record; a
  bounded `deftype` and a bounded `lambda`; `map.of` and lookup matching their
  natives.
- Negative: a user key without `ordered`; a bounded `deftype` applied to a type
  that does not satisfy its bound.

## Done

The `TypeExpr::Map` and `Attribute::Where` rows reference cases, the
body/native harness covers the map rows, and validation passes.
