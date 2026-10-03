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
