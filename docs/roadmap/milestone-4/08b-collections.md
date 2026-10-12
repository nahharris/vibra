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
`str`, `bytes`, and `dict` through their standard-library bodies. The `str`,
`bytes`, and `atom` literals themselves are already lowered by Step 5a, as arena
objects built from passive data segments; this step adds the operations on them
([surface inventory](supported-surface.md)). A `str` is a
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

## As built

Landed as `landed`, conditional on its pull request merging, on `m4` at `a450155`.

- **Which rows are inline, which are native imports, and which run their body.**
  One rule decides. A row whose meaning is a few instructions is lowered inline
  from the one table of `crates/vibra-wasm/src/primitive.rs` (now also `text.length`,
  `bytes.length`, `array.length`, `text.equal`, `bytes.equal`, and the identities
  `array.of` and `dict.of`, which receive the collection the call already packed).
  A row whose algorithm loops is toolchain Rust in `crates/vibra-native`, reached
  by an import from `vibra_native_v1`: `text.concat`, `text.compare`, `text.slice`,
  `text.to-chars`, `text.from-chars`, `text.to-utf8`, `text.from-utf8`, the five
  matching `bytes.*` rows, `array.append`, `array.concat`, `array.slice`, and
  `dict.entries` (16 imports, the table `NATIVE_ROWS` in `vibra-ir`). A native
  whose meaning applies a function value has no import and is run as its body:
  that is `array.fold`, and it is the same rule that makes a dict ordered by a
  declared key's own `compare` lowered code, below.
- **The native crate.** `vibra-native` is written against `Source` (read a range of
  units) and `Sink` (put a unit). The interpreter implements them over its own
  values (`crates/vibra-interp/src/native.rs`), the runner over the instance memory
  (`vibra_native::arena`, reached by `crates/vibra-wasm-run/src/natives.rs`), and
  the interpreter's earlier copies of these algorithms were deleted. The shape of an
  arena object is written once in `vibra_ir::arena`, which the emitter's layout
  re-exports. The emitter depends on neither crate; the architecture boundary test
  fixes that the interpreter and the runner are the only dependents.
- **No allocation in native code.** The module's allocator stays the only allocator,
  and the export table stays exactly the specification's. A row that builds an object
  takes the destination as its last operand and is called twice by emitted code:
  with `0` it measures (the count, or `-1` for `none` or `invalid-format`), and with
  the object the module then allocated it fills it, taking one `dup` for each
  reference it copies. An operand that is not an object of the row, an offset
  outside the memory, or a destination of another size is a `Fault`, which the
  runner reports as a defect and never as a read outside the arena.
- **Dict order.** `dict.of` and a dict tail are built in the module: entries are
  evaluated key then value, inserted by binary search into a scratch array, and an
  equal key is replaced whole by the later entry. A closed key type is ordered by one
  routine per shape (`crates/vibra-wasm/src/key.rs`), which calls `text.compare` and
  `bytes.compare` for text. A declared key is ordered by its own `compare`, which the
  emitter finds statically (the one implementation whose receiver is the key type)
  and calls as an ordinary language function on an arena frame, so no native ever
  calls back into the module and `V1-RUNTIME-dict-compare-nesting` nests three
  hundred levels. A key that needs dispatch from a run-time type (a declared key
  inside a tuple, a generic key) stays `NotLowered` with the form `dict:user-key`,
  Step 9's.
- **Wrappers over text.** `(str array)` is `text.from-chars` and `(bytes array)` is
  `bytes.from-array`; the wrapper pattern over `str` or `bytes` meets the array
  that `text.to-chars` or `bytes.to-array` makes of the subject, dropped when the
  arm ends or fails.
- **Shared vectors.** `CompilerIntrinsic::vectors` gained the text and bytes rows
  (`external/vectors.rs`), with outcomes computed by Rust's `str` and slice
  operations: scalars of each UTF-8 width, prefixes, empty and past-the-end ranges,
  and ill-formed UTF-8. The interpreter's registry test and the new
  `the_text_and_bytes_vectors_hold_the_interpreter_and_the_module_to_one_answer`
  (in `natives_m3_step4b`) hold both backends to them. Array rows have no shared
  vector because a vector operand is a scalar `Value`; their expectations are the
  hand-written encodings of `collections_m4_step8b`.
- **The native harness on all paths.** `natives_m3_step4b` now runs each sample
  as the interpreter's native, the interpreter's body, the module's import, and the
  body lowered to Wasm, and asserts all four agree.
- **Reclamation.** Every test of `collections_m4_step8b` asserts the live size
  returns to what the module values keep, and no frame is left.
- **Test time.** `ConformanceRunner::run_cases` runs cases on every core (the
  parity test uses it), and the dev profile optimizes dependencies, because an
  unoptimized Cranelift dominated every test that runs a module.
- **Left open.** `array.fold` as a module-run body: a native function is a
  wrapper in checked IR (`CheckedFunction::new_external`), so the body of the
  standard library never reaches the emitter. It needs the IR to carry both the
  wrapper the interpreter runs and the body a module runs. `V1-RUNTIME-library-roles`
  stays `not-lowered` for it. Step 9's `V1-RUNTIME-tail-calls-through-flow` needs the
  same.
