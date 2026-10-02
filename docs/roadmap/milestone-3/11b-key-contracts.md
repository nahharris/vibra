# Step 11b — key contracts

Prerequisite: Step 11 merged. Stage 3B behavior step, split from Step 11 so
that the interface machinery lands before the standard library depends on it.

## Read before editing

- [Types](../../spec/02-type-system.md): **Nominal declarations** (dict keys and
  the key contracts), **Generics** (interface bounds).
- [Decision ledger](decision-ledger.md) row D18.2.

## Scope

1. **Key contracts.** `@std.core` declares `equatable` and `ordered`. Every
   checking run sees them, as it sees `ordering`, and a module names them
   through `@std.core` or a declaration import such as
   `(import ordered @std.core.ordered)`, since a `where:` bound is one local
   name.
2. **Closed conformance.** The key primitives, atom singletons, and anonymous
   structures of admissible keys conform to both contracts through the closed
   toolchain registry: they satisfy the bounds, and their contract calls are
   answered by canonical key order, statically and through a bounded generic.
3. **Generic keys.** `(dict k v)` whose `k` has no `ordered` bound is
   inadmissible (`@type.invalid-dict-key`). With the bound it stays
   `@tool.unavailable`: `k` may then be a `deftype` ordered by its own
   `compare`, which only the library dict honors.

Step 11c brings `ordered`-bounded and user `deftype` keys and bounds on
`deftype` and `lambda` parameters; Step 11d moves `dict` into the standard
library with library-written key conformances.

## Test matrix

- Positive: each contract member on a key primitive, an anonymous structure,
  and a bounded generic; the same across an import in a workspace.
- Negative: a float and an array against the contracts
  (`@type.unsatisfied-bound`); a dict keyed by an `any`-bounded parameter.

## Done

The `TypeExpr::Dict` row references the generic-key case, and validation
passes.
