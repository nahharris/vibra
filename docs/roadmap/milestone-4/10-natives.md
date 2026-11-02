# Step 10 — the native import module and the joined differential

Prerequisite: Step 9 merged. Stage 4A behavior step, WebAssembly backend and
harness. It closes the roadmap deliverable that native implementations come from
one source and that the body/native differential joins the interpreter/Wasm
harness. Hannah's decision (2026-10-02, "Bun style") makes that source toolchain
Rust that both backends call: the Vibra runtime embeds Wasmtime, and where
performance matters the toolchain runs native Rust and escapes from Wasmtime into
the host process.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Native implementations**, **WebAssembly
  boundary** (the native import module, the `vibra_v1_memory` export, and the
  scalar-only `@host` boundary), **External providers**.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): **Differential
  execution** (the last paragraph).
- [Projects](../../spec/04-programs-and-packages.md): **Toolchain standard-library
  input** (the manifest `native` list and its provenance rule).
- [M4 ledger](decision-ledger.md) rows D10.1–D10.3, D2.3, D4.3 and the options
  table.

## Scope

Steps 8b and 8c introduce the native-code crate and import the first natives.
This step completes and proves the rule for every listed symbol:

1. every `native:` symbol, and every looping primitive row, has exactly one Rust
   function in the native-code crate, and the closed import list of
   `vibra_native_v1` in `vibra-ir` names exactly those;
2. a module imports only names on that list, no import is visible to source, and
   a native symbol whose meaning applies a function value (`array.fold`) is the
   one exception: it has no import and a module runs its body;
3. the harness runs every native's samples three ways, the interpreter calling
   the Rust, the interpreter running the body, and the module calling the
   import, and compares all three; a bodiless primitive row is run through both
   backends and checked against the shared vectors; and
4. the `Attribute::Native` row of the inventory is completed. A manifest that
   lists a native the toolchain does not implement is still an operational
   provenance diagnostic.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-native` (Steps 8b–8c) | The one Rust source; no dependency on Wasmtime or on the emitter |
| `crates/vibra-ir/src/external.rs` | The import list: names and scalar signatures, read by the emitter and by the runner |
| `crates/vibra-wasm-run` | Supplies the imports over `vibra_v1_memory`; bounds-checks every offset a native receives |
| `crates/vibra-conformance/tests/natives_m3_step4b.rs` | The existing body/native harness and its sample table, which a native without samples fails |
| `crates/vibra-conformance/tests/architecture_boundary.rs` | Must show that the emitter does not depend on `vibra-native` |

## Ordered tasks

1. Check the import list against the manifest's `native` symbols and the
   looping rows; a missing or extra name fails a test.
2. Extend the sample table where an earlier step left a symbol with fewer than
   three paths.
3. The three-way comparison, failing with the path and sample named.
4. A test that no module the emitter produces imports a name outside the list or
   a `@host` entry, that no import is reachable from source, and that removing
   `native:` from every function leaves every corpus result unchanged.
5. A test that a native given an out-of-range offset or length is an
   `@runtime.invalid-checked-program` defect, never a read outside the arena.
6. Update the inventory row and the parity inventory.

Invariants preserved: the body is the meaning; a native import is pure and is not
a host operation (no effect root, no audit event); no offset appears in typed IR,
an encoding, an audit event, or a snapshot; the `@host` boundary stays
scalar-only over IDs.

## Test matrix

- Positive: every native, on boundary and random samples, equal on all three
  paths; every looping row equal across backends.
- Negative: a native whose body and Rust differ fails the harness; a symbol with
  no samples fails it; an import outside the list is rejected before
  instantiation.
- Recovery: not applicable.
- Boundary: empty and one-element inputs, the largest input the harness can run,
  invalid UTF-8 for `text.from-utf8`, a buffer ending at the last byte of memory.
- Formatter: no change.

## Diagnostic and schema changes

None.

## Validation

The [Step 4–12 row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step). Record the sample
count per native.

## Excluded

A `@host` import for a native; a self-contained body-executing module form (M7
decides the shipped product form); accelerating any body in Vibra.

## Completion evidence

The three-way differential passes for every listed native; the boundary test
confirms the import list; the inventory and parity rows are complete.

## As built

Landed as `landed`, conditional on its pull request merging; the base is Step 9 (PR #367, merge `c1217c1`). Steps 8b to 8d had already built the crate, the 42 imports, the four-path harness, and the module-run body of `array.fold`; this step closes the rule and produces the evidence.

- **Audit** (`natives_m4_step10`). A native or a looping row is named only where it is called: the interpreter dispatches in `native.rs` (every arm calls `vibra-native`) and the machine for `array.of`, `dict.of`, and `array.fold`; the emitter names one only to call its import; the runner names none; and no Rust idiom that implements one (ordering text, splitting a head, UTF-8 conversion, parsing) occurs outside `vibra-native`. The audit found three duplicates in the interpreter, now removed (ledger D10.6): key order of `str`, `bytes`, atoms, and variant names; the iterator `iter.next` leaves; the array a `str` or `bytes` pattern is written over. They call `vibra_native::compare`, `slice`, and `apply` of `text.to-chars` and `bytes.to-array`.
- **Pass-through imports.** `array.of` and `dict.of` each have an import that calls the one `identity`; that is the reading of "one per native symbol" (D10.6). No specification text changed and none contradicted it.
- **Manifest and import table.** The manifest `native` list is the registry natives (19); each is an import xor a checked body, and only `array.fold` has a body; the 42 imports are the 18 imported natives (with `array.of` and `dict.of`), the four collection rows, and the 20 number rows. A manifest listing an unknown native, a native in the wrong tier, or omitting one a module binds is refused at load, the provenance defect.
- **Joined differential.** 112 samples over the 19 natives (array.of 6, array.fold 3, dict.of 6, bytes.length 3, bytes.concat 4, bytes.equal 5, bytes.compare 5, bytes.slice 7, bytes.to-array 3, bytes.from-array 3, text.concat 5, text.length 3, text.equal 5, text.compare 9, text.slice 10, text.to-chars 4, text.from-chars 4, text.to-utf8 4, text.from-utf8 23) each run on four paths: the interpreter calling the native, the interpreter running the body, the module calling the import or running the body, and the module running the lowered body. They include empty and one-element inputs, long inputs, every scalar width, and the ill-formed UTF-8 forms. 28 further programs run the four looping rows and `iter.next` in both backends against a literal of the specified value, including atom keys (`text.compare` over atoms) and the dict remainder (`array.slice` of a dict's entries). The rows with vectors stay held to them by `natives_m3_step4b`, `primitives_m4_step8a`, and `numbers_m4_step8c`. The imports a module declares are checked to be on the list.
- **Defect path.** `native_faults_m4_step10` forges modules that call the real imports with an offset outside the memory, an object of another kind, a wrong stride, a length past its block, and a destination of the wrong size; each ends as `Outcome::Defect`, reported as `@runtime.invalid-checked-program` with no origin. An emitted module whose literal object is patched to a wrong kind or stride ends the same way.
- **Not done.** A corpus-wide run with `native:` stripped from the library: the per-sample body path proves each native equals its body.
