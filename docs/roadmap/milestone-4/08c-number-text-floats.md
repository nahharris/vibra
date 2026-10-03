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
