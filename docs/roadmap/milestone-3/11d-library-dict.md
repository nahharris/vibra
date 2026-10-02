# Step 11d — the library dict

Prerequisite: Step 11c merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Language core and standard
  library**, **Nominal declarations** (dict keys, canonical key order).
- [Runtime](../../spec/06-runtime.md): **Native implementations**,
  **Representation latitude**, the registry table.
- [Decision ledger](decision-ledger.md) rows D17.1, D17.3, D17.5, D19.3, and
  D19.4.

## Scope

1. **The library dict.** `dict` stops being an intrinsic type. `@std.builtin`
   declares it as a `deftype` over a sorted array of entries, with
   `where: (k ordered v any)`, claiming `@dict`. Every run declares it, as it
   declares the other role types. Representation latitude keeps the compiler's
   compact dict, and `dict` stays a reserved form head, so the declaration has no
   written constructor or pattern.
2. **Its members.** `dict.of` keeps its native and its body, the sorted
   variadic tail. `dict.entries` is a new registry row and the one primitive
   over a dict: its entries in key order. Lookup stays the language form, whose
   key order Step 11c fixed.
3. **Library conformances.** `equatable` and `ordered` in `@std.core` carry
   implementations for the builtin integers and `char`, as `impl` blocks nested
   in the interfaces; `bool`, `str`, and `bytes` implement both where they are
   declared. An `impl` in a type the toolchain represents directly takes that
   representation as its receiver. The closed registry is their native: every
   run without those modules answers through it, and a harness holds the two
   together. Atom types and anonymous structures stay the registry's own.
4. **One identity per library declaration.** A library module checked on its
   own declares its types and interfaces under their standard identities, so
   its implementations attach to the interface its calls name.

## Test matrix

- Positive: `dict.entries` over closed keys, a user key, a repeated key, and an
  empty dict, in one source and in a workspace; every library conformance
  against the closed registry over sampled operands, with a guard that the
  written implementation ran.
- Negative: none new. A key that does not satisfy `ordered` is
  `@type.invalid-dict-key` from Step 11c.

## Done

The `TypeExpr::Dict` row names the library declaration, `@dict` is claimed, the
body/native harness covers the conformances, and validation passes.
