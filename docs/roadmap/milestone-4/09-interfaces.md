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

## As built

Landed as `landed`, conditional on its pull request merging; the base is Step 8d (PR #366, merge `17fe1ea`). The step was one pull request; it is large, and the natural seam for review is the files below.

- **Descriptors** (`desc.rs`, ledger D9.6). Heads carry a class byte, so code that needs only the kind of a type tests the class. `Type::with_components` (in `vibra-ir`) lets the host rebuild a type from a head and its arguments.
- **Dispatch** (`lower/dispatch.rs`). Candidates are tried in order; each is a run of blocks that match the receiver and interface-argument descriptors and bind generic parameters, then call the function (or tail-call it, rewriting the frame). Default members, destination dispatch (`destination`), the member's own types (`member_types` over `member_generics`), labelled operands, and a dict tail need nothing of their own: the checker resolved operands, and the callee is an ordinary function. A contract member as a value is the closure Step 2 made, with a tail contract call as its body.
- **Interface values** (D9.7). Widening writes the pair; `Direct` and `Indirect` calls open an interface operand taken by a bare generic parameter; a result is widened where the call expects an interface and the callee returns the type it is for.
- **Key unit and closed conformances** (`lower/keys.rs`, `lower/iterate.rs`, D9.8). A dict whose key is generic or nests a declared type, and `ordered.compare` or `equatable.equal` of a closed type, call the key unit; `iter.next` of the four builtin types is inline. `Form::Dict`, `Form::Lookup`, and the `interface` and `contract` details are no longer reported.
- **Host.** `Runner::run_observed_with` and `Instance::observe_with` read an interface value through the descriptor table; `run_observed` is unchanged.
- **Native.** `Layout::Chars` admits an atom, so `text.compare` can order atoms (a dict keyed by atoms was not reachable before).
- **Tests.** `interfaces_m4_step9` and `interfaces_matrix_m4_step9`: both backends agree on interface dispatch, defaults, two argument lists of one interface, interface values in a record, an array and a dict, `iter` over an empty collection and over thousands of items; a hundred thousand tail contract calls run in 256 KiB and the non-tail control exhausts it; the live size returns to its baseline; a function behind `any` is `@runtime.unobservable-function`. Host tests that used an interface program as their example of one that does not lower now use a verified test assertion (Step 11's).
- **Parity.** All 22 rows owned by Step 9 are `matched`: 107 matched, 18 not lowered (all Step 11's).
- **Not lowered.** Opening the pair of a generic-typed argument whose run-time type is an interface, for a bare generic parameter operand of a direct call; a void payload of a builtin `option` iterator; a result conversion through an `array`, `dict`, record, or union (`Form::Call` detail `interface-result`).
