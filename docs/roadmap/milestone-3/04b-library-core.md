# Step 4b — library-first core mechanics

Prerequisite: Step 4a merged. Stage 3A behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Language core and standard
  library**, **Nominal declarations** (builtin types).
- [Source](../../spec/01-source-language.md): the attribute grammar (`role:`,
  `native:`).
- [Projects](../../spec/04-programs-and-packages.md): **Modules and imports**,
  **Toolchain standard-library input**.
- [Runtime](../../spec/06-runtime.md): **M3 compiler intrinsic registry**,
  **Native implementations**.
- [Decision ledger](decision-ledger.md) rows D17.1–D17.4.

## Scope

The mechanisms every later migration uses, with no type moved except
`option`:

1. **Declaration imports.** The resolver accepts an import path that ends in
   one public top-level declaration of a module and binds the alias in every
   namespace that declaration occupies; both check paths see it. A private
   declaration is `@name.private-access`; a path naming neither is
   `@module.unknown-path`.
2. **Roles.** The reader accepts `role:` on `deftype`; the checker binds each
   role from the embedded standard library and replaces every hardcoded
   identity (today `vibra_ir::OPTION_ID` and `standard::option_id`) with a role
   lookup. `role:` outside the embedded library is rejected like `external:`.
   A missing or repeated role is an operational provenance failure at load.
3. **`option` claims `@option`.** `stdlib/src/std/option.vib` writes
   `role: @option`; lookups and `array.slice` answer with the role type, and
   `(option t)` and `option.some`/`option.none` need no import.
4. **Natives.** The reader accepts `native:` on `defn`; the manifest gains the
   `native` list; loading requires every `native:` symbol to be listed and
   implemented. The interpreter runs the native implementation when present
   and the body otherwise, and a conformance harness runs both over the same
   inputs and compares canonical encodings.
5. **Registry split.** The `@compiler` registry is documented and tested as
   primitive operations over the core; the text rows stay primitive until
   Step 8 moves `str` into the library.

## Test matrix

- Positive: a declaration import of a type and of a function; `(option i32)`,
  `option.some`, and a lookup without an import; a `native:` function whose
  native implementation and body agree.
- Negative: a declaration import of a private declaration and of a missing
  name; `role:` and `native:` in a project module; a manifest missing a
  `native` symbol; a repeated role.
- Host: the body/native harness reports a deliberate disagreement.

## Done

Inventory rows for the new attributes and declaration imports reference their
cases; the M3 ledger rows D17.2–D17.4 point at evidence; validation passes.

## Delivery notes

- `role:` and `native:` are reader attributes (`Attribute::Role`,
  `Attribute::Native`), rejected with `@tool.unavailable` outside the embedded
  standard library, as `external:` is.
- The checker's type table records the type claiming each role, so `option`
  is found by role, never by identity: the IR's `OPTION_ID` is gone, registry
  signatures take the role-bound types (`RoleTypes`), and a registry call
  carries its checked result type so the interpreter builds `option` values
  from it.
- Role types resolve by their spelling in both check paths; the workspace
  resolver leaves `option.some` to the checker, which reports an unknown
  variant.
- The loader rejects an unknown role and a role claimed twice. A role nothing
  claims yet is still implemented by the toolchain until its migration step,
  so the "missing role" check waits for Step 14, when every role is claimed.
- `array.of` and `map.of` are the first native implementations: their bodies
  are the packed tail. The registry classifies each entry as primitive or
  native, the manifest lists them in `compiler` and `native`, and
  `crates/vibra-conformance/tests/natives_m3_step4b.rs` runs every listed native
  against its body, which is checked as standard-library code.
- User programs call the native implementation; the body is checked by the
  harness rather than in every program, which the spec permits because the
  body and the native implementation agree.
- A declaration import may bind a type or a function. Its alias names the
  declaration in every namespace, and the privacy check happens once, at the
  import.
