# Step 8a — integer and `char` primitive rows

Prerequisite: Step 7 merged. Stage 4A behavior step, WebAssembly backend. It was
split from the planned Step 8, whose primitive registry is about two hundred
rows, so that each family is held to its own sample vectors: integers and
`char` here, collections in Step 8b, number text and floats in Step 8c.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **M3 compiler intrinsic registry** (the
  integer table, `char.*`, and the tier paragraph), **Native implementations**.
- [Types](../../spec/02-type-system.md): **Language core and standard
  library**, **Nominal declarations** (builtin conformance).
- [Projects](../../spec/04-programs-and-packages.md): **Toolchain standard-library
  input** (the manifest's `compiler` list).
- [M4 ledger](decision-ledger.md) rows D10.1, D10.2, D8.1.

## Scope

The primitive rows of the eight integer types and `char`, lowered by the emitter
before emission and held to shared sample vectors: `add-checked`, `sub-checked`,
`mul-checked`, `div-checked`, `rem-checked`, `neg-checked`,
`shift-left-checked`, `shift-right`, `equal`, `compare`, every `to-U`
conversion, and `char.to-u32` and `char.from-u32`. The `to-str` and `parse` rows
of the integers are looping rows: Rust written once and called by both backends
through the native import module, in Step 8c. The rows here are short enough for
the emitter to lower inline.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-ir/src/external.rs`: `CompilerIntrinsic`, its `signature` | The one table that drives the checker, the interpreter, and Wasm lowering; add a `vectors()` accessor of operand and result samples here so all consumers share it |
| `crates/vibra-interp/src/registry.rs` | The interpreter's implementation, to be held to the same vectors |
| `crates/vibra-wasm` | One inline sequence per row family, returning the standard `option` or `result` values of the arena |
| `stdlib/src/std/builtin.vib`, `stdlib/manifest.vibon` | The declarations; the rows keep `external: @compiler` in this step |

## Ordered tasks

1. Move the sample vectors into `vibra-ir`: every row, with boundary operands
   (zero, one, minimum, maximum, minimum divided by `-1`, shift amounts at and
   past the width), and make the interpreter's registry tests read them.
2. Lower the arithmetic, comparison, and shift families by width and signedness,
   producing `ordering`, `arithmetic-error`, and `conversion-error` values from
   the standard-library enums by canonical identity.
3. Lower the widening and narrowing `to-U` conversions and `char.*`, rejecting a
   surrogate or a value above U+10FFFF as `none`.
4. A differential test that runs every vector through the interpreter and the
   module and compares results.
5. Move the matched cases.

Invariants preserved: every operation is total and trap-free; a partial
operation returns an `option` or `result`; no engine arithmetic trap is
reachable, so a stray one is the toolchain defect.

## Test matrix

- Positive: every vector of every row in both backends; operations composed in a
  program.
- Negative: no source diagnostic; a row without vectors fails the vectors test.
- Recovery: not applicable (no new source form).
- Boundary: each width's minimum and maximum, zero divisors, the signed-minimum
  division, shift amounts of `width - 1`, `width`, and `u32::MAX`, `char`
  surrogate edges, and U+10FFFF.
- Formatter: no change.

## Diagnostic and schema changes

None.

## Validation

The [8a–8c row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step). Report the number of
rows and vectors covered.

## Excluded

`to-str` and `parse` (Step 8c); float rows (Step 8c); arrays, `dict.entries`,
text, and bytes (Step 8b); any change to registry semantics.

## Completion evidence

Every integer and `char` row has vectors that both backends pass; the manifest
list and the vectors agree; the parity inventory only grew.

## As built

- **One table.** `crates/vibra-wasm/src/primitive.rs` maps a `CompilerIntrinsic`
  to a plan: the operands (each an integer type; `char` is a `u32`), the outcome
  (scalar, `bool`, `ordering`, `(result t e)`, or `(option char)`), and the
  computation. `vibra_wasm::lowers_primitive` and the classifier read the same
  table, so a row is lowered exactly when it has a plan. No routine of the
  lowering names a row. The two width classes are one property of the type: a type
  of at most 32 bits is computed exactly in 64 bits and overflows when the result
  lies outside it, and a 64-bit type reads overflow from the operands and the
  wrapped result.
- **Inline, trap-free.** Operands load as exact integers into three new 64-bit
  locals, a row computes one value and one `i32` selector, and no engine
  arithmetic trap is reachable: a divisor is replaced by `1` before it can be
  zero or `-1`, and a shift is checked before its result is used. The lowering
  builds the result from the selector the same way for every row of an outcome,
  as ordinary arena enums (`ok` of a cell, `err` of an enum of `void` variants,
  `some`, `none`, `bool`, `ordering`), with discriminants found by name in the
  checked result type. The error object moves into the `err`, so counts balance on
  every path. A primitive application creates no activation.
- **Shared vectors.** `CompilerIntrinsic::vectors` (`vibra-ir`,
  `external/vectors.rs`) holds 11,414 vectors of 134 rows over the eight integer
  types and `char`, with each boundary value of each type, the signed minimum
  divided by `-1`, zero divisors, shift amounts of `0`, `1`, `width - 1`,
  `width`, `width + 1`, and `u32::MAX`, the edge of every conversion target, and
  the surrogate edges and U+10FFFF. Expected outcomes are computed from the Rust
  integer types, not from either backend, and hand-written examples from the
  specification hold the table. The interpreter's registry test and
  `primitives_m4_step8a` (one program per row, both backends, about 8 s) run
  them all.
- **Native crate.** None. No row of this step loops, so no value-access trait or
  `vibra-native` crate was needed; Steps 8b and 8c create them.
- **Parity.** `V1-RUNTIME-activation-depth`, `-activation-exhaustion`,
  `-workspace-run-activation-exhaustion`, `-tail-closure-captured-loop`,
  `-tail-function-value-parameter`, and `-tail-unrelated-function-loop` moved to
  `matched`. `V1-RUNTIME-registry-integers` stays 8c's: it uses `to-str` and
  `parse`.
- **Left for later.** Integer `to-str` and `parse` and the float rows (8c); the
  array, text, and bytes rows (8b).
