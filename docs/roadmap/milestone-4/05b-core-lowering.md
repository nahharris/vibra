# Step 5b — data and core lowering

Prerequisite: Step 5a merged. Stage 4A behavior step, WebAssembly backend. It
was split from the planned Step 5 so the memory contract of Step 5a is proved
before any compound form uses it.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **Evaluation** (order, body sequences,
  `let`, `return`, module-value initialization), **The value arena**,
  **Canonical value encoding**, **Representation latitude**.
- [Types](../../spec/02-type-system.md): **Nominal declarations**
  (records, enums, tuples, wrappers, unions and written-order discriminants),
  **Application**, **Namespaces and resolution**.
- [M4 ledger](decision-ledger.md) rows D2.1, D4.1, D4.4, D12.1 and the
  [M4 inventory](supported-surface.md) rows owned by Step 5b.

## Scope

Lowering of the data forms and core control forms the arena now supports:
declared and anonymous records, enums, tuples, wrappers, and unions with
discriminants in written member order; constructors and `recordof`, `enumof`,
and `tupleof`; record and tuple projection; module values with lazy once-only
initialization; `let` with binder patterns, body sequences, `if`, `return`;
direct calls of module functions as non-tail calls on the frame stack; and the
host-side canonical result observation for all of them.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-ir/src/lib.rs`: `Expr`, `Type`, `Value`, `nominal.rs` | The forms to lower; every shape the emitter cannot express for both backends is fixed in the IR first, never worked around in the emitter |
| `crates/vibra-wasm` | One lowering function per `Expr` variant; the frame stack and the dispatcher that lets a non-tail call not nest a Wasm call |
| `crates/vibra-wasm-run` | The host-side encoder grows per kind |
| `conformance/parity.tsv` | The rows this step moves |

## Ordered tasks

1. Per-kind layouts and constructors, tuples and records first, then enums,
   wrappers, and unions. Union discriminants take the written member order and
   an anonymous union takes its canonical order; a host test asserts both.
2. Projection: a direct component read with a `dup` of the component and a
   `drop` of the aggregate, so ownership is explicit in one place.
3. `let`, body sequences, `if`, and `return`: locals live in the frame, and a
   binding that leaves scope is dropped.
4. Module values: a once-only guard, evaluated lazily on first read in the same
   order as the interpreter, with fresh state per instance.
5. Direct non-tail calls: a call pushes a frame in the arena and returns to the
   dispatcher loop, so the Wasm call depth does not follow the language depth.
   The tail-call path belongs to Step 6, but the frame layout must already admit
   replacing a frame.
6. Extend the host-side encoder to every kind lowered here and compare against
   the interpreter's encoding; move the matched cases.

Invariants preserved: evaluation order; immutability; no index or offset in the
IR or any observation; byte-identical emission; every unlowered form still
returns `NotLowered`.

## Test matrix

- Positive: each kind constructed, projected, and observed; a recursive record
  through an array is Step 8b's, so here a record through an `option`; nested
  aggregates; union injection and `as` narrowing is Step 7's, so here injection
  and observation only; a module value read twice evaluates once; a function
  that returns early.
- Negative: no new source diagnostic; a program using an unlowered form reports
  `NotLowered` and its case stays `not-lowered`.
- Recovery: a module value whose initializer reads another, in forward order.
- Boundary: zero-field record, single-component tuple, an enum with `void`
  payloads only, a union of two and of many members, a wrapper of a wrapper,
  and a value nested deeply enough to need the worklist release.
- Formatter: no change.

## Diagnostic and schema changes

None.

## Validation

The [Step 4–12 row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step). The handoff lists the
cases moved to `matched`, with counts by profile and by backend.

## Excluded

Calls through function values, closures, generics, and tail calls (Step 6);
`match`, patterns, `try`, `let-else`, `as` (Step 7); arrays, dicts, `str`,
`bytes` (Step 8b); interfaces (Step 9).

## Completion evidence

Every `Lowered` row owned by Step 5b in the [inventory](supported-surface.md)
is exercised by a matched case; the determinism and validation tests still
pass; the parity inventory count of matched cases only grew.
