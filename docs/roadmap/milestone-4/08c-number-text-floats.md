# Step 8c — number text, floats, and NaN

Prerequisite: Step 8b merged. Stage 4A behavior step, WebAssembly backend. It
holds the work D10 chose: the integer and float `to-str` and `parse` rows move
from primitive to native implementations whose meaning is a Vibra body, and the
registry gains four primitive rows. If the float bodies prove too large for one
PR, split this step into 8c (integers and float arithmetic) and 8d (float text)
and record the split in the README, the ledger, and the guide before delivery.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Evaluation** (floating point, NaN, the
  canonical float serialization), **M3 compiler intrinsic registry** (the tier
  paragraph and the float table with `to-bits` and `from-bits`), **Native
  implementations**.
- [Projects](../../spec/04-programs-and-packages.md): **Toolchain standard-library
  input** (manifest `compiler` and `native` lists; each change replaces the
  package version in place).
- [M4 ledger](decision-ledger.md) rows D10.1, D10.2, D11.1, D11.2 and the D10
  options table.

## Scope

1. Float primitive rows: `add`, `sub`, `mul`, `div`, `neg`, `equal`,
   `compare-total`, and the new `to-bits` and `from-bits`, for `f32` and `f64`,
   lowered by the emitter and held to shared vectors.
2. NaN canonicalization at the observation points of the specification, and the
   engine's NaN canonicalization as defence.
3. Integer `to-str` and `parse` (16 rows) as Vibra bodies in `@std.builtin` with
   `native:` symbols, the interpreter's Rust code kept as the accelerator.
4. Float `to-str` and `parse` (4 rows) as Vibra bodies over `to-bits`,
   `from-bits`, and integer arithmetic: a shortest-round-trip printer and a
   correctly rounding parser, with the interpreter's Rust code kept as the
   accelerator.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-ir/src/external.rs`, `stdlib/manifest.vibon`, `stdlib/src/std/builtin.vib` | Move 20 symbols from the `compiler` list to the `native` list, add the four `to-bits` and `from-bits` symbols to `compiler`, replace the package version in place, and update the manifest hashes |
| `crates/vibra-interp/src/registry.rs` | NaN canonicalization at `equal`, `compare-total`, `to-str`, `to-bits`, and the canonical encoding |
| `crates/vibra-conformance/tests/natives_m3_step4b.rs` | Gains samples for the 20 natives, including boundary and random floats |
| `crates/vibra-wasm`, `crates/vibra-wasm-run` | Float lowering and the host-side float encoder, which uses the same canonical serialization |

## Ordered tasks

1. Write the float and integer text bodies in Vibra, with the interpreter's
   native as the oracle in the body/native harness over every boundary value and
   a large deterministic random set; review the bodies as library source.
2. Move the symbols and update the manifest and its tests; keep `check`, `run`,
   and `test` working at every commit.
3. Lower the float rows; canonicalize NaN where observed; enable the engine's
   NaN canonicalization in the runner.
4. Extend the registry vectors and the differential to the new rows, and run the
   module body against the interpreter's native and body.
5. Move the matched cases.

Invariants preserved: no NaN sign or payload is observable; `to-str` is the
canonical float serialization; `parse` follows the float literal grammar and
rounds correctly; the registry stays closed and a native is never needed for a
correct result.

## Test matrix

- Positive: every integer type's `to-str` and `parse` at its extremes; floats at
  zero, negative zero, subnormals, the largest finite value, the infinities, a
  value needing seventeen digits, and the canonical notation boundaries
  `1e-4` and `1e16`; `to-bits` and `from-bits` round trips.
- Negative: `parse` of malformed text (`invalid-format`), of an out-of-range
  integer (`out-of-range`), and of a float that rounds to infinity.
- Recovery: not applicable (no new source form).
- Boundary: `NaN` with each sign and a nonzero payload through `to-str`,
  `compare-total`, `equal`, `to-bits`, and the canonical encoding all agree; a
  NaN produced by `0.0 / 0.0` in each backend.
- Formatter: no change.

## Diagnostic and schema changes

None. The registry change is a specification change already made in Step 1; the
embedded standard library's version is replaced in place and no earlier manifest
shape is kept.

## Validation

The [8a–8c row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step). Record the sample
counts of the native harness and the time of the slowest float body.

## Excluded

Any optimization of the float bodies; float conversions to or from integers,
which v1 does not have; a host import for number formatting.

## Completion evidence

All 20 symbols are natives with bodies; the four new rows exist in both
backends; the NaN cases agree; the manifest and registry tests pass; the parity
inventory only grew.
