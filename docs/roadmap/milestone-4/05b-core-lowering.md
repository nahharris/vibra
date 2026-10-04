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

## As built

The step landed as one change, with these choices, which Step 6 builds on. The
design is written once in the documentation of `crates/vibra-wasm/src/layout.rs`
("Cells" and "Activations and the dispatcher") and in `lower.rs`, and the ledger
records it ([D2.10–D2.13, D3.9, D4.5](decision-ledger.md)).

- **Frames and the dispatcher.** Where the plan said "a call pushes a frame in
  the arena and returns to the dispatcher loop", that is what is built, and the
  5a as-built remark that every function is a Wasm function of its result class
  is replaced: every language function and module-value initializer is one Wasm
  function `(frame) -> ()` in one table, made of numbered basic blocks entered
  through a `br_table` on the frame's `resume` word. A call stores `resume`,
  pushes the callee's frame, moves the operands into it, and returns to the
  dispatcher; a return hands its cell to `ret` and leaves the frame. No Wasm
  call is made per language activation, so a recursion is bounded only by
  memory.
- **Ownership.** One class byte per slot says whether it owns a reference;
  moving out clears it, and leaving a frame drops what still owns. Every
  intermediate value is a frame slot, and nothing is elided.
- **Lowered.** Literals, sequences, `let`, `if`, `return`, module values, fixed
  and labelled parameters, direct calls in non-tail position, `Record`,
  `Variant`, `Wrap`, `Tuple`, both projections, and `Widen` into a union (an atom
  widening is erased). A tail call (`call:tail-direct`), a call of any other
  kind, an omitted labelled operand, and every type with a generic parameter, a
  function, an `array`, a `dict`, or an interface are named by `NotLowered`
  (`type:param`, `type:function`, `type:array`, `type:dict`, `type:interface`).
  A function that implements a contract member is an ordinary function.
- **Discriminants.** Declared enum variants, union members, and record fields
  are in declaration order, anonymous ones in canonical order; a host test
  reads each through `vibra_v1_variant`.
- **The host reader.** `Runner::run_observed` and `Instance::observe` take the
  program's declared types and read a value by its type with an explicit
  worklist, so a value nested to any depth is read and released on a bounded
  host stack.
- **Evidence.** Matched: 20 cases (the 12 owned by 5b, `V1-RUNTIME-generic-void-payload`,
  `V1-RUNTIME-never-type-encoding`, `V1-RUNTIME-tail-negative`, and the six cases
  this step adds). A recursion with no base case reaches 1,670,760 nested
  activations under 64 MiB on a 64 KiB engine stack; a chain of 5,000 nested
  non-tail calls building a value 10,000 levels deep holds 640,160 bytes with
  its result and 0 after release. The checker's validation of a program is
  quadratic in its function count, so a hundred-thousand-deep recursion that
  completes needs Step 8a's arithmetic and is Step 6's case.
- **Found.** The reference interpreter reports `InvalidBody` for a module
  initializer that calls a function with an operand (`(def v i32 (one 1i32))`);
  the Wasm backend runs it, and the case waits for a fix to the oracle.
- **For Step 6.** Tail calls rewrite the frame in place ([D2.12](decision-ledger.md));
  indirect calls and closures reuse the table and signature; type arguments are
  slots; `Default` and `Closure` are the first forms to lower.
