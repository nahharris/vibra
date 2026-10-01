# Step 11d — the library map

Prerequisite: Step 11c merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Language core and standard
  library**, **Nominal declarations** (map keys, canonical key order).
- [Runtime](../../spec/06-runtime.md): **Native implementations**,
  **Representation latitude**.
- [Decision ledger](decision-ledger.md) rows D17.1, D17.3, D17.5, and D19.3.

## Scope

1. **The library map.** `map` leaves the compiler's builtin declarations and
   becomes a standard-library `deftype` over a sorted array of entries, with
   `where: (k ordered v any)`, claiming `@map`. As with `str`, representation
   latitude keeps the compiler's compact map; wrapping sorts and merges
   entries through the key order, so no source can build an unsorted map.
2. **Vibra meaning.** `map.of` and lookup are written in Vibra over the
   entries, calling `ordered.compare`, with native implementations that agree
   with them; the body/native harness covers both, including a user key.
3. **Library conformances.** The closed key conformances for the core and
   library types become ordinary implementations in the standard library,
   which needs standard-library implementations to join every checking run.
   The structural rule for anonymous types stays the toolchain's.

## Test matrix

- Positive: `map.of` and lookup over closed and user keys matching their
  natives; a library conformance called directly; a map built by wrapping
  unsorted, duplicated entries.
- Negative: a map applied to a key type that does not satisfy `ordered`
  through the library declaration's bound.

## Done

The `TypeExpr::Map` row names the library declaration, the body/native
harness covers the map rows, `@map` is claimed, and validation passes.
