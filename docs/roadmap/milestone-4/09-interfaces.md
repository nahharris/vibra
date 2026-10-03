# Step 9 — interfaces, dispatch, conversion, and iteration

Prerequisite: Step 8c merged (and Step 2 for the contract-member forms). Stage 4A
behavior step, WebAssembly backend.

## Read before editing

- [Types](../../spec/02-type-system.md): **Interfaces and methods**,
  **Iteration**, **Conversion**, **Type ascription and widening**,
  **Generics**.
- [Runtime](../../spec/06-runtime.md): **Generic instantiation**, **Tail calls**
  (a contract member through an interface value), **Evaluation**.
- [Step 2 guide](02-contract-members.md) for the IR shapes this step lowers, and
  [M4 ledger](decision-ledger.md) rows D9.1, D9.2, D12.2.

## Scope

Static dispatch, interface values (`any` and generic interfaces) with the
run-time type of the receiver, default members at the receiver type and the
interface arguments of the call, destination-dispatched members and conversion
(`from`, `try-from`), the closed conformances (key `compare` and `equal`, and
`iter.next` of `array`, `dict`, `str`, and `option`), the `iter` contract with
its adapters and defaults, and the Step 2 forms: a member's own generics,
labelled operands, `types:`, a dict tail, and a member as a function value.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-ir/src/lib.rs`: `CallTarget::Contract`, `Implements`, `ClosedContract`, and the Step 2 additions | The selection data both backends consume |
| `crates/vibra-wasm` | An interface value as a record of the receiver's run-time type and the value; selection by comparing instantiated types, since type arguments already pass at run time |
| `crates/vibra-interp/src/lib.rs` | The reference selection to match, including the first-matching-implementation order that M3's review fixed |

## Ordered tasks

1. Run-time type descriptors: a canonical, deterministic representation of an
   instantiated type, compared structurally, used by selection.
2. Widening to an interface value at each written boundary, and the selection
   routine that finds the one implementation admitting the type.
3. Static dispatch and default members; the tail call through a contract member
   reuses the frame.
4. Destination dispatch and conversion, including the unsuffixed-literal and
   ambiguity rejections that stay compile-time.
5. Closed conformances and `iter` with its four adapters and five defaults, as
   lowered library bodies.
6. The Step 2 forms in Wasm, from the Step 2 IR.
7. Move the matched cases, and lower the `iter` cases' walks.

Invariants preserved: dispatch selects from instantiated types; an interface
adds no run-time information to a value beyond what widening writes; a union has
no method table; a destination-selected member needs no receiver.

## Test matrix

- Positive: every case in `V1-RUNTIME` and `V1-TOOL` that dispatches, converts,
  or iterates; one receiver implementing a generic interface at two argument
  lists; a default member calling a contract member at the receiver type; an
  interface value in a record field, an array, and a dict value.
- Negative: no new source diagnostic; a function hidden behind an interface that
  reaches an observation is the trap `@runtime.unobservable-function` (Step 11
  owns the trap, this step must reach it).
- Recovery: not applicable (no new source form).
- Boundary: a generic interface with no arguments, a receiver of a generic type,
  an interface value of an interface value's payload, an `iter` over an empty
  collection and over ten thousand items at constant depth.
- Formatter: no change.

## Diagnostic and schema changes

None.

## Validation

The [Step 4–12 row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step).

## Excluded

Effect ceilings on interfaces and `iter` callbacks (Step 14); effectful walks
over `iter.next` (Step 16); user-written implementations of `ordered` for dict
keys beyond what M3 supports.

## Completion evidence

Every `Lowered` row owned by Step 9 has a matched case, including I3–I6 of the
[inventory](supported-surface.md#inherited-rows) in Wasm.
