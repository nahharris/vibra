# Step 4 — collections and the standard-library input

Prerequisite: Step 3 merged. Stage 3A behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Nominal declarations** (map keys,
  canonical key order), **Application**.
- [Source](../../spec/01-source-language.md): **Labels and applications**,
  **Functions and expressions** (collections).
- [Runtime](../../spec/06-runtime.md): **Evaluation**, **Canonical value
  encoding**, **M3 compiler intrinsic registry** (`option` recognition).
- [Projects](../../spec/04-programs-and-packages.md): **Toolchain
  standard-library input**.
- [Decision ledger](decision-ledger.md) rows D3.1, D3.2, D4.4, D5.1, D5.2.

## Scope

Two parts in one PR, because lookups return `(option t)`:

1. Replace the signed M2 bootstrap with the embedded `stdlib/manifest.vibon`
   input as `vibra-stdlib@0.2.0`: delete `stdlib/m2/`, the signature, the key,
   `crates/vibra-types/src/bootstrap.rs`'s signature path, and the `ring`
   dependency if nothing else uses it; move `text.vib` and `assert.vib` under
   `stdlib/src/std/`; add `@std.option`. Update every M2 case and test whose
   provenance observation names `0.1.0` or the old paths.
2. `tuple`, `array`, and `map` types; the static methods `tuple.of`,
   `array.of`, `map.of`, and the `array` operations in `@std.builtin` (ordinary variadic methods except `tuple.of`); tuple
   projection; array, map, `str`, and `bytes` lookups returning `option`;
   variadic array and map declarations, function types, and operands; map-key
   admissibility; canonical key order in the interpreter's map representation.

User-declared map types whose key is a generic parameter stay
`@tool.unavailable` (Step 11); the toolchain-declared `map.of` is not one.

## Ordered tasks

1. Standard-library input replacement with host tests for a digest mismatch, a
   manifest symbol absent from the registry, and a project-declared `external:`.
2. Builtin constructor types and admissible-key checks.
3. The `@std.builtin` collection methods, projection, and lookups; empty
   collections need an expected type; `tuple.of` as a value is
   `@name.wrong-entity-kind`; `array.of` passed as a function value works.
4. Variadic slots and operands, including the M2 variadic availability cases,
   which become positive and negative cases.
5. Interpreter map representation ordered by canonical key order, with a host
   test that no host hash order is reachable.

## Test matrix

- Positive: heterogeneous `tuple.of`; `array.of` with an expected empty type;
  duplicate-key `map.of` keeps the later value; each lookup present and absent;
  a nested tuple key; variadic calls with zero and several tail operands.
- Negative: odd `map.of` and heterogeneous `array.of`
  (`@type.argument-mismatch`); empty `array.of` without expected type
  (`@type.ambiguous-inference`); out-of-range tuple index
  (`@type.invalid-tuple-index`); `f64`, `void`, array, and record keys
  (`@type.invalid-map-key`); `fn` key (`@type.function-not-equatable`);
  source `(array ...)` in expression position.
- Interpreter: two insertion orders produce identical encoding.

## Done

Inventory rows for the tuple/array/map types, variadics, and `Declaration::Import`
input replacement reference their cases; M2 row C1.6 is implemented;
validation passes.
