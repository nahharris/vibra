# Step 8c — number text, floats, and NaN

Prerequisite: Step 8b merged. Stage 4A behavior step, WebAssembly backend. It
holds the looping primitive rows: integer and float `to-str` and `parse` stay
primitive rows with no Vibra body, implemented once in Rust by the
native-code crate and reached by the interpreter directly and by a module
through the pure `vibra_native_v1` import
([ledger D10.1, D10.2](decision-ledger.md)). No float printer or parser is
written in Vibra, and no `to-bits` or `from-bits` row exists.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Evaluation** (floating point, NaN, the
  canonical float serialization), **M3 compiler intrinsic registry** (the integer
  and float tables), **Native implementations**, **WebAssembly boundary** (the
  native import module and the memory export).
- [M4 ledger](decision-ledger.md) rows D10.1–D10.3, D11.1, D11.2 and the D10
  options table.

## Scope

1. The float primitive rows `add`, `sub`, `mul`, `div`, `neg`, `equal`, and
   `compare-total` for `f32` and `f64`, lowered inline by the emitter and held to
   shared vectors.
2. NaN canonicalization at the observation points of the specification
   (`equal`, `compare-total`, `to-str`, the canonical encoding, and test
   canonical equality), and the engine's NaN canonicalization as defence.
3. The 20 looping rows, integer and float `to-str` and `parse`, as pure Rust
   functions in the native-code crate, written against the value-access
   interface, called directly by the interpreter and imported by modules.
4. The host-side float encoder of the runner, which uses the same canonical
   serialization.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-native` (proposed, created here or in Step 8b if text natives need it first) | The Rust for these rows, with the value-access trait that the interpreter and the runner each implement; it must not depend on Wasmtime |
| `crates/vibra-ir/src/external.rs` | The row table gains the native import name and signature of each looping row; the emitter reads names from here only |
| `crates/vibra-interp/src/registry.rs` | Replaces its own `to-str` and `parse` code with calls into the native-code crate |
| `crates/vibra-wasm-run` | Supplies the `vibra_native_v1` imports, reading and writing the module's memory through `vibra_v1_memory` |
| `crates/vibra-conformance/tests/natives_m3_step4b.rs` | Sample table for the shared vectors |

## Ordered tasks

1. Move the interpreter's `to-str` and `parse` code into the native-code crate
   behind the value-access trait, with the interpreter's tests unchanged.
2. Add the import names to the `vibra-ir` table and lower a call of each row as
   an import call; the runner supplies the functions.
3. Lower the float arithmetic rows inline; canonicalize NaN where observed;
   enable the engine's NaN canonicalization in the runner.
4. Shared vectors for every row, run through the interpreter and the module, with
   boundary values and a large deterministic random set for the text rows.
5. Move the matched cases.

Invariants preserved: no NaN sign or payload is observable; `to-str` is the
canonical float serialization; `parse` follows the float literal grammar and
rounds correctly; a native import is pure and reaches no ambient state; the
registry stays closed and its table is unchanged.

## Test matrix

- Positive: every integer type's `to-str` and `parse` at its extremes; floats at
  zero, negative zero, subnormals, the largest finite value, the infinities, a
  value needing seventeen digits, and the canonical notation boundaries `1e-4`
  and `1e16`.
- Negative: `parse` of malformed text (`invalid-format`), of an out-of-range
  integer (`out-of-range`), and of a float that rounds to infinity.
- Recovery: not applicable (no new source form).
- Boundary: NaN with each sign and a nonzero payload through `to-str`,
  `compare-total`, `equal`, and the canonical encoding all agree; a NaN from
  `0.0 / 0.0` in each backend; a string at the end of linear memory.
- Formatter: no change.

## Diagnostic and schema changes

None. The registry table of the specification is unchanged by this step.

## Validation

The [8a–8c row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step). Record the sample
counts of the shared vectors.

## Excluded

Any Vibra body for these rows; `to-bits` and `from-bits`; float conversions to or
from integers, which v1 does not have; a `@host` import for number formatting.

## Completion evidence

All 20 rows are Rust called by both backends; the NaN cases agree; the shared
vectors pass in both; the parity inventory only grew.

## As built

Landed as `landed`, conditional on its pull request merging; the base is Step 8b (PR #364).

- **Which rows are inline and which are imports.** The registry table decides, by whether a row loops, and nothing written in the emitter. The 14 float rows `add`, `sub`, `mul`, `div`, `neg`, `equal`, and `compare-total` of `f32` and `f64` are inline rows of the one table of `crates/vibra-wasm/src/primitive.rs` (`Body::Float`), with the integer and `char` rows and `array.length`. The 20 `to-str` and `parse` rows, of the eight integer types and of `f32` and `f64`, are primitive rows that loop, so each is an import of `vibra_native_v1` (42 rows in `NATIVE_ROWS` with 8b's 22). `plan()` returns nothing for a row that is native or has an import, so the table cannot lower a looping row; `a_row_is_inline_exactly_when_it_is_a_primitive_row_that_does_not_loop` and `the_import_table_names_the_natives_and_the_looping_primitive_rows` (in `primitives_m4_step8a`) guard it and now count the float rows and the 20 imports.
- **The Rust.** `crates/vibra-native/src/number.rs` holds `integer_to_str`, `float_to_str`, `parse_integer`, and `parse_float` against `Source` and `Sink`, and the interpreter's own copies in `registry.rs` were deleted. `float_to_str` calls `vibra_ir::canonical_f32_text` and `canonical_f64_text`, the one serialization, which the canonical value encoding (and so the runner's host-side encoder) uses too. `vibra_native::arena::call` adapts the cells: an integer of at most 32 bits is its 32-bit pattern zero-extended, a float is its bit pattern, and `to-str` receives the 64 bits of its cell as one `i64` (`Operand::Number`).
- **`parse` builds no object.** It takes the address of an 8-byte cell of the module's own frame as its last operand (`Answer::Parsed`), writes the number there in the cell format of its type, and answers a status: `0`, `1` for `invalid-format`, `2` for `out-of-range`. So it allocates nothing and needs no measuring; an address that is `0` or outside the memory is a fault (ledger D10.4). The emitter branches on the status into `ok` of the cell or `err` of the error variant, as the checked integer rows do.
- **NaN.** `canonical_f32_bits` and `canonical_f64_bits` in `vibra-ir` are the one replacement by the quiet NaN of the width. The module's `compare-total` applies it to both operands and then compares by the `totalOrder` key of the bits; `equal` is the engine's comparison, which is false for a NaN already; the other float operations return whatever NaN the engine gives, and `to-str` writes `nan`. The interpreter's observer and the runner's both replace a NaN where a result is read, so `ObservedValue` holds the quiet NaN. The engine's NaN canonicalization stays on as defence and is no part of the proof: the tests hold with it on, and a module whose `compare-total` skips the replacement fails the vectors of NaNs with payloads, which the rewritten literals carry into the module exactly.
- **Shared vectors.** `CompilerIntrinsic::vectors` gained every integer and float row (`external/vectors/numbers.rs`): 11,539 vectors, with 495 for integer `to-str`, 392 for integer `parse`, 2,103 for float `to-str`, 2,296 for float `parse`, and 6,253 for the float arithmetic, equality, and order rows. Their outcomes come from Rust's own `f32` and `f64` operators, the digits of `{:e}` assembled by hand into the canonical serialization (a unit test shows it equals Rust's `{:?}` on 5,000 random bit patterns of each width), `totalOrder` derived from the sign and the magnitude bits (checked against `total_cmp`), and the standard library's integer and float parsers behind a hand-written grammar. They cover zero and negative zero, the subnormals, the largest and smallest finite values, the infinities, NaN of both signs and several payloads, the switch of notation at `1e-4` and `1e16`, out-of-range, malformed, and empty text, each integer type at its minimum and maximum, one past them, a leading `-` on the unsigned types, and non-ASCII digits, plus a deterministic pseudo-random set.
- **Both backends, the same program.** `numbers_m4_step8c` writes a program of each row's samples with placeholder literals and replaces the checked program's literals by the exact operands, because source has no spelling for an infinity or a NaN, let alone for a payload. The interpreter and the module run the rewritten program, and both are held to the table. The interpreter's registry test holds the same table with no program at all.
- **Reclamation and the end of memory.** The arena balance of every path through the `to-str` and `parse` rows, a tail loop of number round trips whose live size is only what the module values keep after 2,000 rounds and after 20,000, in 256 KiB, and a string built at every memory limit from 1 KiB to 256 KiB, which completes or exhausts the memory and never faults.
- **Test time.** `numbers_m4_step8c` runs in about 5 seconds; its 11,539 vectors take about 1.2 seconds of that.
- **Not delivered, and not Step 8c's.** Test canonical equality of a float is the same replacement and is exercised when the Wasm backend runs tests (Step 11). No spec text changed: the registry table, the canonical float serialization, and the NaN rule were already specified.
