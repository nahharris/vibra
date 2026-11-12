# Nexus design

Status: planning record, non-normative. The reasons behind each choice are in
the [decision ledger](decisions.md), cited here as N*n*. Names of crates,
functions, and symbols that do not exist yet are proposals.

## 1. Pipeline and crates

```text
syntax -> resolved AST -> checked IR ──────────────> reference interpreter (oracle)
                              │
                              └─ vibra-lower ─> execution IR ─ verify ─┬─> vibra-nexus-eval (reference stepper)
                                                 (vibra-exec)          └─> vibra-codegen ─> object ─┐
                                                                                                    ├─ link ─> executable
                              vibra-nexus (+ vibra-host, vibra-native) ─ vibra-nexus-bridge ─ archive┘
```

| Crate | Responsibility | Depends on | State |
| --- | --- | --- | --- |
| `vibra-exec` | Execution IR, verifier | `vibra-ir` | Exists (foundation) |
| `vibra-nexus` | Contexts, value store, frames, transfers, dispatcher, traps, host dispatch, admission; later actors' executor and workers | `vibra-host`, `vibra-ir`, `vibra-native` | Core exists |
| `vibra-nexus-eval` | Reference stepper: execution IR run on Nexus by a loop | `vibra-exec`, `vibra-nexus`, `vibra-ir` | Exists for the foundation's instruction set |
| `vibra-lower` | Checked IR to execution IR. The only crate that knows both | `vibra-ir`, `vibra-exec` | X3-03, then X3-10 to X3-18 |
| `vibra-codegen` | Execution IR to a target object with Cranelift. The only crate that depends on `cranelift-*` | `vibra-exec` | X3-06 |
| `vibra-nexus-bridge` | The C ABI between generated code and Nexus, the launcher, and the static archive. **The only crate with `unsafe`** | `vibra-nexus`, `vibra-host` | X3-05 |
| `vibra-build` | Toolchain discovery, the link driver, the build manifest | `vibra-codegen`, `vibra-lower`, `vibra-ir` | X3-07, X3-23 |
| `vibra-actors` | The actor kernel, written once for both executors | `vibra-ir`, `vibra-host` | X4-01 |

The arrows respect the roadmap's architecture boundary: nothing in language
semantics depends on a backend, and the checker, workspace, CLI, and MCP are
untouched until X3-23 exposes `build`. `vibra-conformance` gains the new
crates as dependencies; the CLI does not until X3-23.

Three separations carry most of the design:

- **Lowering is separate from code generation** (N4, N6). A lowering defect
  and a code-generation defect are found by different comparisons, and the
  first comparison needs no linker.
- **Transfers are written once, in Nexus** (N5). The stepper and generated
  code decide which transfer comes next; neither pushes or pops a frame.
- **Layouts are private to Nexus** (N7, N11). Baseline generated code calls
  the runtime for every value operation, so a layout change touches one crate.

## 2. Execution IR

`crates/vibra-exec/src/lib.rs` is the definition. A `Program` is a flat list
of `Function`s, module-value initializers, constants, and an origin table. A
`Function` is a list of `Block`s over the **slots** of one frame. A `Block` is
straight-line `Inst`s and one `Terminator`.

### Checkpoints

Five terminators hand control to the runtime, and they are the only places
where a frame is pushed, replaced, or popped and the only places execution may
yield (N3):

| Terminator | Transfer in `vibra_nexus::Context` | Frame effect |
| --- | --- | --- |
| `Call` | `call`, `call_value` | Pushes the callee. The caller's `resume` names the block that continues it |
| `TailCall` | `tail_call`, `tail_call_value` | Replaces the top frame, keeping its return link |
| `Return` | `ret` | Pops, writes the value to the caller's `dst`, or ends the program |
| `Host` | `host` | None. Performs the operation once, then sets `resume` |
| `Global` | `global` | Pushes the initializer on the first read; no transfer later |

Vibra has no loop form, so every unbounded computation passes a checkpoint.
`Jump`, `Branch`, and `Switch` stay inside a step. A value that must survive a
checkpoint is in a slot, so a frame plus its `resume` block is the whole state
of a suspended activation. That one property gives bounded native stack,
tail transfers to any callee, yields, and (in v0.0.4) parking.

### Ownership

A slot is empty or full. Writes need an empty slot and reads a full one.
`Move`, `Build`, `Function`, `Call`, `TailCall`, and `Return` **consume**
their operands; every other use **borrows**. `Drop` accepts any state and
leaves the slot empty. A frame that ends releases what it still owns. These
are the count rules of M4 (ledger rows D3.9 and D3.10 of
[the M4 ledger](../v0.0.1/m4/decision-ledger.md)) made checkable.

### Verification rules

`vibra_exec::verify` implements these, and `crates/vibra-exec/tests/it` has an
accepted and a rejected program for each:

| Rule | Holds of every program |
| --- | --- |
| V1 | It has a function; the entry and every initializer exist and take no operand and no environment |
| V2 | Every function has a block, every block is reachable from block 0, and the frame holds the entry slots |
| V3 | Every block, slot, function, constant, module value, and origin named exists |
| V4 | A direct call passes exactly the callee's parameters and names a function without an environment; a function value names one with an environment; scalar operations and host calls pass exactly their operands |
| V5 | No consumed operand list names a slot twice, and an indirect callee is not also one of its operands |
| V6 | On every path a slot is full where read and empty where written (forward data-flow; a slot the paths disagree on is unusable until dropped) |

**Extending the IR** is one change with four parts, never fewer: the new
instruction or terminator, its verification rule with accepted and rejected
tests, its execution in the stepper, and (once X3-06 exists) its code
generation. A lowering may not emit an instruction the stepper rejects.

### From checked IR

What `vibra-lower` produces for each `vibra_ir::Expr`. The Wasm lowering
(`crates/vibra-wasm/src/lower.rs` and its submodules) solved the same problems
against the same checked IR and is the reference for evaluation order and
count placement; it is active code, not the archive.

| Checked IR | Execution IR | Package |
| --- | --- | --- |
| `Literal` | `Scalar`; `Constant` for `str`, `bytes`, atom; `bool` is `Build Enum` with variant 0 or 1 and no cell | X3-03 |
| `Sequence` | Straight line; each discarded value is dropped | X3-03 |
| `Variable`, `Captured` | `Copy` from the slot; `Cell` of the environment at `3 + index` | X3-03 |
| `Let` | Evaluate into the binding's slot; `Drop` when the body ends | X3-03 |
| `If` | `Variant` of the `bool`, then `Branch` | X3-03 |
| `Call` direct, indirect; `tail` | `Call` or `TailCall` with `Direct` or `Indirect` | X3-03 |
| `Function`, `Closure` | `Function` over a body with an environment. A module function used as a value gets a wrapper body that is one tail call (M4 D2.14) | X3-03 |
| `Global` | `Global` | X3-03 |
| `Host` | `Host`, which ends the block | X3-03 |
| `Return` | `Return` | X3-03 |
| `External` | `Intrinsic` | X3-10, X3-16 |
| `Record`, `Tuple`, `Variant`, `Wrap`, `Array`, `Widen` | `Build` | X3-11 |
| `Project`, `TupleProject` | `Cell`, then `Drop` of the aggregate | X3-11 |
| `Match`, `Try` | `Variant`, `Switch` or `Branch`, `Cell` for binders | X3-12 |
| Generic activations | Type descriptors in slots after the environment (M4 D9.5) | X3-13 |
| `Default`, labelled and variadic operands | Defaults in the function value (M4 D2.14) | X3-14 |
| `Call` contract | A new resolve instruction that yields a function value, then `Indirect` | X3-15 |
| `Dict`, `Lookup` | Intrinsic and runtime rows | X3-16 |
| Test assertions | A new assertion instruction | X3-18 |

## 3. The runtime

### Context

A `Context` is one isolated execution: a `Store`, a stack of frames over one
cell stack, the module-value states, a trap record, an audit trace, and a
host provider. Nothing of one context is visible to another. v0.0.3 has one
context per process; v0.0.4 has one per actor (N20).

### Frames

```text
frame   = [parameters][environment][type arguments][bindings][temporaries]
header  = function, resume, base, slots, dst, global
```

Frames live in a cell stack the context owns, not on the native stack and not
inside the value store (N8). A frame and its slots are charged against the
context's memory limit, so recursion exhausts memory as the host event
`@runtime.memory-exhausted` and never as a native stack overflow.

### The step protocol

```rust
pub trait Code { fn step(&self, ctx: &mut Context<'_>) -> Step; }
pub enum Step { Continue, Done, Trap, Exhausted }       // `Wait` joins in v0.0.4
pub fn run(ctx: &mut Context<'_>, code: &dyn Code, quota: u64) -> Outcome;
pub enum Outcome { Completed, Yielded, Trapped(Trap), MemoryExhausted }
```

One step runs the top frame from its `resume` block up to and including its
next checkpoint, and keeps nothing outside the context. The dispatcher `run`
loops over steps. A checkpoint costs one unit of quota plus whatever the step
charged with `Context::add_charge` for proportional work; when the quota is
used up `run` returns `Yielded`, and a later `run` continues.

Quota is **not observable** (N3): it changes no result, trap, origin, or audit
event, and the language defines no budget. It exists so a scheduler can share
a thread. A primitive is atomic with respect to yielding: it completes and
then charges its cost, so the debt is paid at the next checkpoint. The proof
obligation is the *yield-everywhere* comparison: every executable case run
with quota 1 observes exactly what it observes unbounded.

### Admission

`vibra_nexus::admit` checks a program's `Requirements` before any of it runs:
the runtime ABI version, the derived registry minor, and that every host
symbol is a registry row. With `vibra_exec::verify`, this is G3.M1.11.

## 4. Values

### The store

One growable block of bytes per context, with the header and payload layouts
of `vibra_ir::arena`: the layout the M4 Wasm arena proved and the
toolchain-owned native code already reads (N7). A `Ref` is a 32-bit offset
into its own store: a context-local handle, never a pointer. Blocks are
power-of-two size classes from 32 bytes to 1 GiB with a free list per class.
A cell is 8 bytes plus a class byte (`I32`, `I64`, `F32`, `F64`, `REF`).

Reclamation is precise reference counting with no cycle collector; the runtime
chapter's "Reclamation" gives the argument that values cannot form cycles.
Release walks a worklist threaded through the freed blocks, so it uses no
stack at any depth. Counts are non-atomic because a context is used by one
thread at a time.

A native program has **no host value IDs and no handle table** (N7). Those
exist in the Wasm boundary because the host is outside the module. In a native
executable the host operations are toolchain Rust in the same process, and
they reach values through `vibra_host::HostValues` over cells.

### Function values, module values, descriptors

- A function value is a `FUNCTION` object `[function][slots][params]
  [captures…]`. A call through it checks the operand count (a mismatch is the
  toolchain-defect trap `@runtime.invalid-checked-program`) and passes the
  value itself as the callee's environment (N9). Defaults and type arguments
  join the header in X3-13 and X3-14.
- A module value has three states: unread, evaluating, ready. The first read
  pushes its initializer as a frame; the return stores the value and gives the
  reader its own reference. Each execution starts fresh.
- Generic code receives type descriptors as ordinary values in slots, as the
  interpreter and the Wasm backend pass type arguments at run time (N15).

### Observation

A result, an assertion operand, and an audit argument are observed by Rust
code inside Nexus that walks the store by the value's checked type and builds
`vibra_ir::ObservedValue`, the structure the interpreter already encodes
(N16). No canonical-encoding walker is generated into the program.

## 5. Host calls, traps, origins

A `Host` terminator names a `&'static HostRow`. `Context::host` reads the
borrowed arguments through `HostValues`, runs `vibra_host::perform` on the
context's provider, builds the reply, writes `dst`, sets `resume`, and only
then appends `AuditEvent::completed`: the same operation code and the same
event code as the interpreter. A call that traps or exhausts memory appends
nothing. Because the operation and the advance of `resume` happen inside one
step, a yield before or after cannot repeat it (G3.M1.9).

Traps keep the closed codes of the runtime chapter. Generated code and the
stepper record a trap as code plus origin ordinal in the context and stop. The
program carries an origin table (ordinal *n* is entry *n − 1*; 0 is no
origin). In a build product the table is a data section of the object, and
its identity is the manifest's source-map identity.

## 6. Generated code and the bridge

Each execution-IR function becomes one native function, its **step**. A step is
a switch on the `resume` block it is given, over the function's blocks.
Instructions are runtime routine calls. `Jump` is a native jump. `Branch` and
`Switch` call the routines `nexus_branch` and `nexus_switch`, which read the
condition and answer the block to continue at, because generated code cannot
read a slot (N11, N33). `Trap` calls `nexus_trap`. A checkpoint calls its
transfer routine and returns the status the routine answered. No Vibra call is
a native call, so the native stack holds the dispatcher, one step, and one
runtime routine at any language depth (N2).

The contract between generated code and the runtime is owned by
`vibra-nexus-bridge` (N10, N32). Its
[crate documentation](../../../crates/vibra-nexus-bridge/src/lib.rs) defines
the `VibraProgram` layout, the routine table (`ROUTINES`), and the statuses.
This section describes the design they implement and does not restate the
tables.

- **Step.** `uint32_t step(NexusContext *ctx, uint32_t resume_block)`. A step
  keeps nothing outside the context and returns one status: *continue* (the
  context has a frame to continue), *done* (the entry returned), *trap* (the
  trap is recorded in the context), *exhausted* (the host event), or *wait*,
  which is reserved for v0.0.4 and never answered today (N23). A routine that
  answers anything but *continue* ends the step with its status.
  `nexus_global` also answers *proceed* when the module value was already
  initialized; the step then carries on at its resume block instead of
  returning. A block named to a routine must be below the function's declared
  block count, and the dispatcher checks the `resume` it reads the same way;
  otherwise the toolchain-defect trap is recorded.
- **Routines.** One routine per transfer (call, call through a value, tail
  call, tail call through a value, return, host, global) and per instruction
  (scalar, constant, copy, move, drop, operation, build, cell, variant,
  function), and the three of N33: `nexus_branch`, `nexus_switch`, and
  `nexus_trap`. `nexus_branch` and `nexus_switch` answer the block in the low
  32 bits and a status in the high 32. Operands cross as a pointer to `u32`
  slot indices and a count that the running frame bounds, and are read-only to
  the routine. Values never leave the context: no pointer into the store or
  the cell stack crosses the bridge (N11).
- **Descriptor.** The object defines one data symbol, `vibra_program`, a
  `VibraProgram`: the ABI version, the registry minor, the entry,
  `entry_outcome`, and tables of functions, module-value initializers,
  constants, host symbols, and origins. A function row carries its step, its
  frame size, its parameter count, whether it takes an environment, and its
  block count (`blocks`). `entry_outcome` is `0` when the entry's result is not
  a `result`, and `n` when it is a `result` whose `err` variant has index
  `n - 1`; the launcher reports *program-failed* from it and never from the
  shape of a value. Each host symbol must be a row of the registry, and
  `nexus_host` names one by its index in the table. The descriptor carries no
  identity digest: the build identities belong to the manifest (§9).
- **Admission.** The launcher admits the whole descriptor before any step runs
  and refuses, with `@runtime.incompatible-artifact`, an ABI other than its
  own, a registry minor above its own, a symbol that is no registry row, and
  any malformed table. The execution IR was verified before code generation,
  so the bridge checks only what a descriptor can get wrong and the operands of
  each routine against the running frame.
- **No unwinding.** No unwind crosses the bridge. A routine that is misused
  records the toolchain-defect trap and answers a status. A panic in an
  `extern "C"` function aborts the process instead of unwinding into generated
  code, and the workspace lints deny the constructs that panic (N34).

## 7. Link and launch

The toolchain ships, per target, a static archive built from
`vibra-nexus-bridge` (Nexus, the host providers, the native code, the
launcher's `main`). `vibra build` emits one object with Cranelift and invokes
the platform linker on the object and the archive (N13).

| Target | Object | Linker | Located by |
| --- | --- | --- | --- |
| `x86_64-pc-windows-msvc` | PE/COFF | MSVC `link.exe` with the Windows SDK and UCRT | The Visual Studio installer's `vswhere` and the SDK registry keys, the way `rustc` finds them. **Never `PATH`**: on the development machine `link.exe` on `PATH` is BusyBox |
| `x86_64-unknown-linux-gnu` | ELF | The pinned `cc` driver | An explicit path in the toolchain's target profile |
| `aarch64-apple-darwin` | Mach-O | Apple `ld` through `cc`, with the pinned SDK | `xcrun` |

The launcher has two profiles (N18). **system** ships: the system provider,
the process arguments, the standard streams, and the exit codes of the tooling
chapter: `0` for a program that completed, `5` for a trap, `3` for the host
event `@runtime.memory-exhausted`, and `6` for an entry that returned `err`.
An admission refusal also exits with `3`, provisionally. The code line of the
outcome goes to standard error; a trap adds its source span from the origin
table. **harness** exists only in the test archive. Its first argument is an
optional quota, it reads a `@host-world.v1` document from standard input, and
it writes one observation document to standard output for the conformance
runner to compare: the outcome, the kind of result (`void` or `ok`, `err`, or
none), the trap and its origin ordinal, and the audit trace. It exits `0`
whatever the program did and `2` when its input is not a host world. The
document does not carry the result value, whose observation is X3-18 (N35),
and it reports no yield count and no peak counter.

## 8. Parity

There is one expectation per case and the interpreter is the oracle. During
the transition a case is executed by up to four executors:

| Executor | Compares | Exists on |
| --- | --- | --- |
| Reference interpreter | The specification | Everywhere |
| Wasm backend | Interpreter (existing, stays until cutover) | Everywhere |
| Reference stepper | Lowering against the interpreter | Everywhere, no linker |
| Native executable | Code generation and link against the stepper and the interpreter | Its own platform |

`conformance/native-parity.tsv` has one row per executable case with a column
per executor (`stepper`, `windows`, `linux`, `macos`), each `matched` or
`pending <package>`. A host test fails when the table and the corpus
disagree, a package only moves cells toward `matched`, and `pending` is never
a pass: M2 exits at zero pending cells (N14). This is the record G3.M1.12 asks
for. The Wasm backend needs no such table any more, because it lowers every
checked form; the native table reaches that state at the M2 exit and is then
removed.

Every native and stepper comparison runs twice: unbounded, and with quota 1.

## 9. Build products (M3)

`vibra build --target <triple>` writes the executable and
`<target>.build.vibon`. The manifest's two identities are separate by
construction: the **semantic input identity** comes from v0.0.2 M5's digest
(H1) and does not see the target; the **artifact identity** hashes the
executable and records the triple, CPU baseline, Cranelift and linker
versions, normalized flags, `ABI_VERSION`, the registry minor and symbols,
required effects, and the origin table's hash. Reproducibility is byte
equality of both files across two clean offline builds in different checkout
paths on one target.

## 10. The actor layer (v0.0.4)

The source surface is Hannah's to specify in X4-S1. This section fixes only
the runtime shape that surface will sit on. The surface is a receive loop
([decisions.md](decisions.md), O1), so an actor parks inside a receive.

- **One kernel, two executors** (N19). `vibra-actors` owns actor identity,
  mailboxes, addresses and their scope checks, reply slots, exit, timeouts,
  and the event record. It is generic over an executor that can run one actor
  until its next *kernel action* (send, receive, reply, host operation, exit)
  and hand that action to the kernel. The interpreter and Nexus each implement
  the executor. Semantics cannot drift between them, for the reason host
  operations cannot: there is one implementation.
- **One context per actor** (N20). An actor's state and frames are a Nexus
  `Context`, owned by one worker at a time, so single ownership is structural.
  A message is copied out of the sender's store into a kernel-owned neutral
  value at send and into the receiver's store at delivery. Sharing an
  immutable snapshot is the same copy until measurement asks for more.
- **Addresses** are a new closed value kind. An address holds the actor, the
  set of operations it admits, and a bound resource scope. Narrowing builds a
  new address whose set and scope are no wider. The kernel checks both at the
  dispatch boundary, before the handler sees the message (G4.8).
- **Waiting** is `Step::Wait`: the step parks the context with a wait token,
  the dispatcher returns it to the kernel, and a wake makes it runnable.
  Nothing about a frame changes, because a frame already is the suspended
  state.
- **A schedule** is the sequence of kernel decisions: which runnable actor
  performs its next kernel action. The simulation scheduler takes decisions
  from a recorded schedule or a seed and runs on one thread with virtual time.
  Replay needs no agreement between executors on quota or reductions, because
  decisions are taken only at kernel actions, which both executors reach in
  the same order for one actor (N21).
- **Workers** (N22). The production scheduler is a fixed pool of OS threads.
  An actor is placed on a worker before its initialization and stays there:
  no migration and no stealing in v0.0.4. Addresses name actors, never
  workers. Multicore runs are not compared event-for-event with anything; only
  replayed simulations are.
- **Events.** The audit trace format already admits new `kind` atoms as a
  minor change. Message events (`send`, `deliver`, `reply`, `park`, `wake`,
  `exit`) are new kinds carrying source origins, which is how tooling shows
  the causal path from a send through a handler to its host operation (G4.9).

## 11. What v0.0.4 extends (the M1 handoff)

| v0.0.3 contract | v0.0.4 extends it by |
| --- | --- |
| Checkpoints: the five transfers | Kernel actions become checkpoints too; `Step` gains `Wait`, which a step answers with status 4, reserved today |
| The step signature `(ctx, resume_block) -> status` | Unchanged. A step that parks answers *wait* and resumes at the block it recorded |
| Quota is unobservable | Unchanged. Fairness uses it; replay does not |
| Handles are context-local offsets, layout private | Unchanged. Messages cross as neutral values, so no handle leaves a context |
| Origins: an ordinal table per program | Message events carry origins of the send and the handler |
| Host dispatch through the closed registry | Unchanged. An address scopes a *request*; the registry and its adapters keep their own contract |
| The routine table (`ROUTINES`) and the descriptor | Each kernel action is a new routine, and any new descriptor field is a new `ABI_VERSION`; `entry_outcome` and the per-function block count stay |
| `ABI_VERSION` | Bumped by the actor transfers |

The proposed split of authority from effect (G3.M1.7): **holding an address is
the authority to request; the effect row is what a body may perform.** A
sender's row gains only the messaging effect. The handler's row carries the
host effects, and the binary target must declare the host effects of every
actor body it can start, so target consent stays complete.
