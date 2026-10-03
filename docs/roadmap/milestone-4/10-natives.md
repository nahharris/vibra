# Step 10 — natives and the joined body/native differential

Prerequisite: Step 9 merged. Stage 4A behavior step, WebAssembly backend and
harness. It closes the roadmap deliverable that native implementations are
"lowered from the single source" by making the body that source and by joining
the existing differential to the Wasm backend.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Native implementations**, **M3 compiler
  intrinsic registry**, **Determinism and observability**.
- [Diagnostics](../../spec/07-diagnostics-and-conformance.md): **Differential
  execution** (the last paragraph).
- [Projects](../../spec/04-programs-and-packages.md): **Toolchain standard-library
  input** (the manifest `native` list and its provenance rule).
- [M4 ledger](decision-ledger.md) rows D10.1–D10.3 and the options table.

## Scope

Most natives are lowered already as ordinary bodies by Steps 8b and 8c. This step
finishes and proves the rule:

1. the Wasm backend lowers the body of every `native:` function and never a
   native symbol, and a test fails if an emitted module names one;
2. the harness runs every native's samples through the interpreter's native,
   the interpreter's body, and the module's body, and compares all three;
3. a manifest that lists a native the toolchain does not implement is still an
   operational provenance diagnostic, and a toolchain that implements it only as
   a body is conforming; and
4. the `Attribute::Native` row of the inventory is completed.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-conformance/tests/natives_m3_step4b.rs` | The existing body/native harness and its sample table, which a native without samples fails |
| `crates/vibra-wasm`, `crates/vibra-wasm-run` | Run the sample inputs against the module's body |
| `stdlib/manifest.vibon`, `crates/vibra-types/src/stdlib.rs` | The listed natives and their validation |

## Ordered tasks

1. Extend the sample table where Steps 8b and 8c left a native with only
   interpreter samples, so every listed native has module samples.
2. A harness function that instantiates one module per sample batch and calls
   the lowered body with the sample operands, returning canonical encodings.
3. The three-way comparison, failing with the backend and sample named.
4. A test that no module the emitter produces imports, names, or exports a
   native symbol, and that removing `native:` from every function leaves every
   corpus result unchanged.
5. Update the inventory row and the parity inventory.

Invariants preserved: the body is the meaning; checking and effects come from
the body; a native reaches no host operation.

## Test matrix

- Positive: every native, on boundary and random samples, three-way equal.
- Negative: a native whose body and implementation differ fails the harness; a
  manifest symbol with no samples fails it.
- Recovery: not applicable.
- Boundary: empty and one-element inputs, maximum-length samples the harness
  can run, invalid UTF-8 for `text.from-utf8`.
- Formatter: no change.

## Diagnostic and schema changes

None.

## Validation

The [Step 4–12 row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step). Record the sample
count per native.

## Excluded

A Wasm-native implementation of any symbol; accelerating any body; changes to
the manifest's trust boundary.

## Completion evidence

The three-way differential passes for every listed native; the inventory and
parity rows are complete.
