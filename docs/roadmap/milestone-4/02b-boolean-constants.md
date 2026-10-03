# Step 2b: boolean constants and constant patterns

Requires Step 2. Decided by Hannah on 2026-10-03 (ledger D14.1, D16.1-D16.4);
her goal is the fewest keywords possible.

## What changes

- **No boolean literal in source.** `true` and `false` are the public values
  `(def true bool (bool.true))` and `(def false bool (bool.false))` of
  `stdlib/src/std/bool.vib`. `.vibon` keeps them as literals.
- **The prelude.** The closed import-free vocabulary of the type chapter is the
  one set of declarations every module sees. It now holds the two values. The
  modules that declare it are in every checked graph: a workspace check scope
  always includes `@std.bool`, a single source declares the values from the
  embedded module, and every workspace command loads the embedded library.
- **Constant patterns.** A pattern name, bare or dotted, that resolves to a
  module `def` with a constant initializer is a value pattern. `true` and
  `false` are such names, with no special treatment. A non-constant `def`, a
  function, a parameter, or a local stays `@name.redeclaration`.
- **Reserved names.** Every prelude name is a reserved spelling at a binder
  site (`@name.reserved-declaration`) and at a module value, function, or alias
  (`@name.reserved-value-spelling`, for `true` and `false`).

## Not changed

- Typed IR gains no expression and no pattern. A read of `true` is an
  `Expr::Global`; a constant pattern lowers to the literal or constructor
  pattern it expands to. The interpreter is unchanged.
- The M4 surface inventory is unchanged: no AST variant was added.

## Proof

Cases: `V1-TYPE-CONTROL-constant-patterns`,
`V1-TYPE-CONTROL-constant-pattern-rejections`, `V1-RUNTIME-constant-patterns`,
`V1-TYPE-NAMES-vocabulary-binders`, `V1-TYPE-NAMES-vocabulary-declarations`,
`V1-PROJECT-workspace-check-constant-pattern-import`, and
`V1-PROJECT-workspace-check-vocabulary-declaration`. Host tests:
`vocabulary_binders_m4_step2b` in `vibra-resolve` and `vibra-types`.

## Left alone, reported

Existing special rules in the areas touched: the exclusion of float and `void`
literal patterns; the lowering of a `bool` variant to a literal pattern; the
hard-coded `assert.*` declarations in the resolver; the `tuple` case of
`is_reserved_value_spelling`; the `@literal` query kind for `(bool.true)`; a
module value or alias named after a role type is still accepted; and a
labelled parameter default must be a literal or an atom, so `true` cannot be
one.
