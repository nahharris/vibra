# Step 5a — the value arena and its runtime

Prerequisite: Step 4 merged. Stage 4A behavior step, WebAssembly backend. It
was split from the planned Step 5 because the arena, its reclamation, and the
host accessors are one self-contained contract with its own proof (bounded
live size, bounded-stack release, exhaustion), and carrying every compound form
with it would make one change that cannot be reviewed against that proof.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **The value arena**, **Reclamation**,
  **Activations and memory**, **WebAssembly boundary**, **Canonical value
  encoding**, **Representation latitude**.
- [Types](../../spec/02-type-system.md): **Value semantics**.
- [README](README.md#fixed-implementation-decisions) (arena, reference counting,
  and activation-frame decisions) and [M4 ledger](decision-ledger.md) rows
  D2.1–D2.3, D3.1–D3.5, D4.2.

## Scope

The memory layer of every module, with scalars and literals as its first
users:

1. the arena in linear memory: allocation, size classes, a free list, and
   growth by `memory.grow`, with growth failure recorded as the memory host
   event;
2. precise non-atomic reference counting: emitted `dup` and `drop` with no
   elision, and a release routine that walks an explicit worklist;
3. the handle table: 64-bit IDs, `0` invalid, never reused, each live ID holding
   a reference;
4. the instance state: status, trap code, origin, failure record, the module
   values' lazy state, and the frame stack that holds activations;
5. the `vibra_v1_memory` export, for toolchain-owned native code only, and the exported accessors of the [boundary table](../../spec/06-runtime.md#webassembly-boundary)
   (`vibra_v1_release`, `_length`, `_variant`, the `read_*` family, `_result`,
   `_live_size`, `_status`, `_trap_code`, `_origin`); and
6. lowering of scalars, `void`, and literals, and the host-side canonical
   result observation that reads a value through the accessors.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-wasm` (Step 4) | A runtime-support module of the emitter that builds the allocation, release, and handle-table functions once and includes only those a program uses, in a fixed order, so emission stays deterministic |
| `crates/vibra-wasm-run` (Step 4) | The accessor wrappers and the host-side canonical encoder; keeps IDs out of its public results |
| `crates/vibra-ir/src/observed.rs`, `canonical_vibon` | The canonical result encoding the host-side encoder must reproduce byte for byte |
| `crates/vibra-conformance` | The harness's Wasm result path |

## Ordered tasks

1. Specify the layout in the emitter's documentation: object header (kind,
   count, size), per-kind payloads, and the frame layout, so Step 5b and later
   steps add kinds by table, not by editing every routine.
2. Allocator and `dup`/`drop`, then the worklist release, then a host test that
   a value nested 5,000 and 100,000 deep releases with bounded engine stack.
3. Handle table and the accessors, then the host-side encoder over scalars and
   `void`, so the result of a program that returns a literal matches the
   interpreter's canonical encoding.
4. Memory exhaustion: a limit smaller than the program's need records status
   `2` and stops, with no partial result; an ID counter that would wrap does the
   same.
5. `vibra_v1_live_size`: allocated minus released bytes, including the handle
   table, with a host test that it returns to its starting value after a balanced
   program.
6. Lower scalar and `void` literals and the `void` entry; move the cases that
   now match from `not-lowered` to `matched`.

Invariants preserved: no offset, address, or ID reaches typed IR, an encoding,
an audit event, or a snapshot; a stop is an engine trap preceded by a recorded
status; emission is byte-identical.

## Test matrix

- Positive: every scalar literal type round-trips through the accessors, and
  `bool`, an enum value, reads through `vibra_v1_variant`; the
  result of a literal program matches the interpreter byte for byte; `dup` and
  `drop` balance to a live size equal to the start; IDs are strictly increasing
  and never reused after a release.
- Negative: ID `0`, a released ID, an ID of another instance, an index out of
  range, and a wrong-kind read each record `@runtime.invalid-host-value`;
  exhausting memory is status `2` and never a trap.
- Recovery: after a recorded stop the accessors still answer and a fresh instance
  runs normally.
- Boundary: allocation of the largest size class and of a block the free list
  can only partly satisfy; the limit at exactly the program's need and one page
  under; a worklist that grows past one page.
- Formatter: no change.

## Diagnostic and schema changes

None: `@runtime.invalid-host-value` and `@runtime.memory-exhausted` are already
registered. The command-result and test schemas do not gain the first until
Step 13.

## Validation

The [Step 4–12 row](validation.md#focused-checks) and the full
[pre-merge list](validation.md#before-merging-each-step). Record the live size
measurements and the deep-release evidence in the handoff.

## Excluded

Compound value kinds beyond what the accessors name (Step 5b onward); any
elision, borrow inference, or in-place reuse (M7); the host's build side and
imports (Step 13); source maps.

## Completion evidence

The accessor table of the specification is implemented in full; the host tests
above pass; the cases moved to `matched` are listed; the deep-release and
balanced-live-size results are recorded.

## As built

The step landed as proposed, with these choices, which the later steps build on.
The representation is written once, in the documentation of
`crates/vibra-wasm/src/layout.rs`, and the ledger records each decision
([D2.4–D2.8, D3.6–D3.8](decision-ledger.md)).

- **The layout.** The instance state is a block of fixed addresses at the start
  of linear memory, with the free-list heads after it and the arena from byte
  256. Every arena value is one block with a 24-byte header and a payload that
  depends on its stride alone (cells with class bytes, 4-byte characters, or
  bytes); a kind table gives each of the eleven kinds a row, and the accessors
  admit kinds by masks the table computes, so a step adds a kind by adding a
  row. The handle table is a block of the arena that is freed with its last ID.
- **The routines.** `crates/vibra-wasm/src/runtime.rs` builds the allocator,
  `dup`, `drop`, the worklist release, the handle table, and the fourteen
  function exports of the boundary table that need no test, in a fixed order; the
  object constructor is the only routine a module includes conditionally. Release
  threads its worklist through the dying blocks, so the engine stack is bounded
  at any depth or width and the worklist needs no storage and cannot fail.
  `vibra_wasm::support::module_with_entry` builds a module around a
  hand-written entry body for host tests of values no lowered form builds yet.
- **What lowers.** The `void` and scalar literals, `bool` (an enum with
  variants `false` and `true`), and the `atom`, `str`, and `bytes` literals,
  which are arena objects built from passive data segments, and the sequence,
  which drops every value but the last. Every function is a Wasm function of its
  result class, none of which is called until Step 5b. The `NonVoidResult` and
  `Literal` forms are gone, and `Result` names a result type that no lowered
  value represents.
- **No case matches yet.** Every checked source program carries the prelude's
  `true` and `false` module values, which Step 5b lowers, so the parity inventory
  is unchanged at 0 matched of 100 and the step is proved by hand-built IR (the
  literals of every type against the interpreter byte for byte) and by hand-built
  modules over the runtime routines.
- **The runner.** `Runner::start` gives an `Instance` whose accessors take a
  `ValueId` that carries its instance and shows no number, and
  `Runner::run_observed` runs the entry and reads the result by its type into the
  value the interpreter's canonical encoding takes. A scalar entry result is read
  from its bits in `vibra_v1_result`, which the runtime chapter now states
  ([D2.8](decision-ledger.md)).
- **For Step 5b.** The module-value state (`module_values`) and the frame stack
  (`frame_segment`, `frame_top`, `frame_limit`, `frame_depth`) are reserved and
  zero, with their representation fixed in the layout; the object constructor
  and the cell classes are what a constructor lowers to.
