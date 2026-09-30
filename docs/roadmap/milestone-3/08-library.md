# Step 8 — core library foundation

Prerequisite: Step 7 merged. Stage 3A behavior step.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **M3 compiler intrinsic registry**,
  **Canonical value encoding**.
- [Projects](../../spec/04-programs-and-packages.md): **Toolchain
  standard-library input**, **Tests**, **M3 assertion contract**.
- [Decision ledger](decision-ledger.md) rows D4.2, D4.3, D5.2, D7.2.

## Scope

Every Stage 3A registry row, bound by trusted declarations in the listed
modules; the reviewed Vibra-written composite functions (boolean connectives,
`char` comparison and classes, text search/split/trim, array folds); and the
generic `assert.equal` replacing the five `assert.equal-*` members. Change the
**Tests** example in the projects chapter to `assert.equal` in the same PR,
refresh the example inventory, and migrate every M2 case and CLI process test
that uses a removed assertion.

Per ledger D17.1–D17.3 (Step 4a), this step also moves `bool`, `str`, `bytes`,
`ordering`, and the standard error enums into the standard library as
`deftype`s over the core, claiming `@bool`, `@str`, and `@bytes`. The text and
bytes operations become standard-library Vibra with native implementations,
and the registry keeps only primitive operations over the core. A migrated
type keeps its canonical value encoding.

## Implementation notes

The registry lives with the M2 compiler registry in `vibra-ir::external`;
each operation has an interpreter implementation and host tests at its
boundaries: minimum and maximum values, zero divisors, the signed minimum
divided or negated, shift amounts equal to and above the width, empty and
non-ASCII strings, invalid UTF-8, surrogate code points, NaN and signed zeros.
Registry signatures are checked exactly against the trusted declarations.

## Test matrix

- One interpreter case per operation family through its real module import,
  plus the boundary cases above in host tests.
- Negative: an unknown `symbol:` in the trusted package
  (`@external.unknown-symbol`); a signature that differs from the registry; a
  project module declaring `external:`.
- Tests: `assert.equal` pass and failure with compound values rendered by the
  canonical encoding; an `fn` operand (`@type.function-not-equatable`).

## Done

The M2 registry note on `integer.*` symbols is closed by the static methods of
the numeric primitive types; all registry rows have evidence; validation passes.

## Delivery split

Step 8 lands as three PRs, like 4a/4b:

- **8 — registry.** Every Stage 3A `@compiler` row: the integer and float
  static methods of the numeric builtin types in `@std.builtin`, and
  `@std.char`, `@std.text`, and `@std.bytes`, with interpreter semantics and
  host boundary tests.
- **8b — composites and assertions.** The reviewed Vibra composites and the
  generic `assert.equal`, with the projects-chapter example and the migrated
  cases and CLI tests. Array folds move to 8c: they are the first `@std.builtin`
  members with Vibra bodies, which need the same bodied-member support as the
  native text and bytes rows.
- **8c — library migration.** `bool`, `str`, `bytes`, `ordering`, and the error
  enums as standard-library `deftype`s under their roles. The text and bytes
  rows become native implementations, and `array.fold` joins `@std.builtin`.

## Delivery notes (8)

- `vibra-ir::external` is its own module. `CompilerIntrinsic` has typed
  numeric families, `Integer(NumericType, IntegerOp)` and
  `Float(NumericType, FloatOp)`, next to the flat module and collection rows.
  `CompilerIntrinsic::all()` lists the 134 rows and `symbol()` spells
  `T.<name>`. `RoleTypes` also binds `@result` and the three `@std.core`
  types, found by their standard-library identity, so no signature fixes a
  library identity in the registry.
- `@std.builtin` declares `(deftype i32 (intrinsic-type @i32) …)` for every
  numeric type, with its methods as `external: @compiler` members. They are
  reached as `(i32.add-checked a b)` with no import, as the collection members
  are. `@std.char` and `@std.bytes` are new embedded modules, and `@std.text`
  gains every text row. The manifest lists all 132 compiler symbols. Until 8c,
  the text and bytes rows are primitive operations over the toolchain's
  representation.
- `vibra-interp`'s `registry` module is the reference semantics: integer
  arithmetic in `i128` with range checks; binary32 arithmetic rounded at its
  own width; NaN canonicalized for `compare-total`; scalar-counted text
  slices; and every partial row answering with the `option`, `result`, or
  `ordering` value of its checked result type.
- The runtime chapter now defines the canonical float serialization, which
  was referenced but never specified. Value encodings use it too, so
  integral floats encode as `1.0f64` and extreme ones in scientific notation
  (`V1-TYPE-INFER-float-boundaries` was updated).
- The module rows are covered through their real imports by the workspace-test
  case `V1-RUNTIME-registry-modules`: the single-file checker still admits only
  the `@std.text` import.

## Delivery notes (8b)

- `@std.assert` has three members: `true`, `false`, and the generic `equal`
  (`TestAssertion::Equal`, `where: (t any)`). The interpreter compares every
  assertion operand by canonical value encoding and reports failures with
  those encodings, so compound values render as the runtime chapter writes
  them (`V1-RUNTIME-workspace-test-assert-equal`). An operand whose type is or
  contains a function, including through declared bodies, is
  `@type.function-not-equatable` at that operand.
- Every corpus case, host test, and CLI process test that used a removed
  `assert.equal-*` member now uses `assert.equal`, as does the projects
  chapter's **Tests** example. The corpus case that expected the generic
  member to be unavailable became the new coverage.
- The canonical string encoding escapes the remaining control characters as
  `\u{HEX}`, so the value encoding and the test runner's failure strings are
  one encoding.
- The composites are reviewed Vibra in their modules:
  - `@std.bool`: `not`, `and`, `or`, and `xor`, eager because they are
    functions.
  - `@std.char`: `equal`, `compare`, and the ASCII classes `is-ascii-digit`,
    `-upper`, `-lower`, `-letter`, and `-whitespace` (space, tab, line feed,
    form feed, carriage return).
  - `@std.text`: `starts-with`, `ends-with`, `index-of` (in scalars),
    `contains`, `split` (an empty separator yields the whole text), and
    `trim`, `trim-start`, and `trim-end` over ASCII whitespace.

  `@std.bool` joins the embedded set. They are covered through their imports
  by `V1-RUNTIME-library-composites`.
- `@std.text` now imports `@std.char` and `@std.core`, so no single-module
  checker can check it. The M2 `check_bootstrap_source` entry point is
  removed; the module is checked through the resolved path wherever it is
  imported. The CLI and runner tests that used a generic assertion as their
  unavailable form now use a nonempty lambda effect row, which stays
  unavailable until M4.
