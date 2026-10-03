# Step 8b — collections, text, bytes, and dict

Prerequisite: Step 8a merged. Stage 4A behavior step, WebAssembly backend.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Evaluation** (variadic tails, dict
  order, lookups, string and byte indexing), **M3 compiler intrinsic registry**
  (`array.*`, `dict.entries`, the `text.*` and `bytes.*` natives),
  **Native implementations**, **Canonical value encoding**.
- [Types](../../spec/02-type-system.md): **Nominal declarations** (closed key
  conformance, canonical key order), **Application**.
- [Library bodies](../../../stdlib/src/std/text.vib) and
  [bytes](../../../stdlib/src/std/bytes.vib), the meaning of the natives.
- [M4 ledger](decision-ledger.md) rows D10.1, D10.3, D11.1.

## Scope

`array` construction, `array.of`, variadic array and dict tails, checked lookups
returning `option`, `array.length`, `append`, `concat`, `slice`, `array.fold`,
`dict.of` and `dict.entries` in canonical key order, array patterns, and
`str`, `bytes`, and `dict` through their standard-library bodies. A `str` is a
wrapper over an array of `char`, so it needs no kind of its own beyond what the
arena names.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-ir/src/external.rs`, `stdlib/src/std/builtin.vib`, `text.vib`, `bytes.vib` | The primitive array rows and the Vibra bodies, which stay the meaning of the natives ([ledger D10.1](decision-ledger.md)) |
| `crates/vibra-native` (new, proposed) | The Rust of the `text.*`, `bytes.*`, `array.of`, and `dict.of` natives, written once against the value-access trait that the interpreter and the runner each implement; no Wasmtime dependency |
| `crates/vibra-wasm` | Array layout with an element-count header, element `dup` on read and `drop` on release, dict as sorted entries with the key order of `ordered.compare` |
| `crates/vibra-conformance/tests/natives_m3_step4b.rs` | The native samples that the native code, the body, and the import must all pass |

## Ordered tasks

1. Array layout, `array.length`, lookup, `append`, `concat`, `slice` with
   copy-on-construction (no in-place update, which is M7's), and variadic tails.
2. `array.fold` as its library body over a function value: it applies a function
   value, so it has no native import and a module runs its body.
3. `dict.of` and `dict.entries`: later pair replaces an equal key, order is the
   key type's canonical order for each closed key type, and nothing depends on
   insertion or a hash.
4. `str` and `bytes` values, and the text and bytes natives as imports of
   `vibra_native_v1` backed by the native-code crate, including UTF-8 encoding and
   decoding; the interpreter calls the same functions directly.
5. Array patterns with exact length.
6. Canonical encoding of every collection in the host-side encoder.
7. Move the matched cases and extend the native samples to run the interpreter's
   native, the interpreter's body, and the module's import.

Invariants preserved: dict order is key order; a missing key or index is
`option.none` and never a trap; element ownership is balanced; a value nested
through arrays releases with bounded stack.

## Test matrix

- Positive: arrays of every element kind; a dict built in two insertion orders
  that iterates and encodes identically; every `text.*` and `bytes.*` operation;
  a recursive record through an array; `array.fold` with a closure.
- Negative: no new source diagnostic; an out-of-range lookup and a slice with
  start past end return `none`.
- Recovery: not applicable (no new source form).
- Boundary: empty array and dict, a duplicate key, a nested tuple key, a
  surrogate-adjacent `char`, an invalid UTF-8 sequence, a ten-thousand-element
  array, an array nested 5,000 deep.
- Formatter: no change.

## Diagnostic and schema changes

None.

## Validation

The [8a–8c row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step), with the native
harness run on all three paths.

## Excluded

Integer and float `to-str` and `parse` (Step 8c); in-place update or any
copy-avoidance (M7); a Wasm-native reimplementation of any symbol.

## Completion evidence

Every `Lowered` row owned by Step 8b in the [inventory](supported-surface.md)
has a matched case; the natives agree with their bodies on all three paths.
