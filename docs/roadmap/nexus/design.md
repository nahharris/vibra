# Nexus design

Status: planning record, non-normative. The reasons behind each choice are in
the [decision ledger](decisions.md), cited here as N*n*. Names of crates,
functions, and symbols that do not exist yet are proposals.

The architecture: frames in runtime memory, a verified execution IR, a
reference stepper, Cranelift, and a static runtime archive. Section numbers
and titles are stable, because crate documentation cites them.

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
| `vibra-lower` | Checked IR to execution IR. The only crate that knows both | `vibra-ir`, `vibra-exec` | A slice of the core forms exists. Rebased by X2-01 and X2-02, then X3-10 to X3-19, X3-28, X3-29 |
| `vibra-codegen` | Execution IR to a target object with Cranelift. The only crate that depends on `cranelift-*` | `vibra-exec` | Exists for the slice's instructions, x86-64 only |
| `vibra-nexus-bridge` | The C ABI between generated code and Nexus, the launcher, and the static archive. **The only crate with `unsafe`** | `vibra-nexus`, `vibra-host` | Exists. Its descriptor and launcher change in X2-03 |
| `vibra-build` | Toolchain discovery, the link driver, the build manifest | `vibra-codegen`, `vibra-lower`, `vibra-ir` | The Windows link driver exists. The manifest is X3-23 |
| `vibra-actors` | The actor kernel, written once for both executors | `vibra-ir`, `vibra-host` | The mailbox, lifecycle, reply slot, and scheduler core exist |

"Exists" means the code is in the workspace. It was built before the
language was redesigned, so it is a starting point and not evidence (N41).

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
tests, its execution in the stepper, and its code
generation. A lowering may not emit an instruction the stepper rejects.

### From checked IR

What `vibra-lower` produces for each `vibra_ir::Expr`. Evaluation order is
that of the [runtime chapter](../../spec/06-runtime.md#evaluation): operands
are evaluated in written order. X2-01 aligns the node names of this table
with the checker of the redesigned language.

| Checked IR | Execution IR | Package |
| --- | --- | --- |
| `Literal` | `Scalar`; `Constant` for `Str`, `Bytes`, atom; `Bool` is `Build Enum` with variant 0 or 1 and no cell | exists; X2-01 |
| `Sequence` | Straight line; each discarded value is dropped | exists; X2-01 |
| `Variable`, `Captured` | `Copy` from the slot; `Cell` of the environment at `3 + index` | exists; X2-01 |
| `Let` | Evaluate into the binding's slot; `Drop` when the body ends | exists; X2-01 |
| `If` | `Variant` of the `Bool`, then `Branch` | exists; X2-01 |
| `Call` direct, indirect; `tail` | `Call` or `TailCall` with `Direct` or `Indirect` | exists; X2-01 |
| `Function`, `Closure` | `Function` over a body with an environment. A module function used as a value gets a wrapper body that is one tail call (M4 D2.14) | exists; X2-01 |
| `Global` | `Global` | exists; X2-01 |
| `Return` | `Return` | exists; X2-01 |
| `requires … else`, `ensures … else` | A branch on entry; a candidate slot, one check, and a branch on exit (see [Guards](#guards)) | X2-02, X3-28 |
| Applying a `proc`; `await` | `Function` with the plan kind; `Call` or `TailCall` through it (see [Plans](#plans)) | X2-02, X3-29 |
| `Host` | `Host`, which ends the block. Its first operand is the grant | X2-03, X3-19 |
| `External` | `Intrinsic` | X3-10, X3-16 |
| `Record`, `Tuple`, `Variant`, `Wrap`, `List`, widening to an interface value | `Build` | X3-11 |
| Construction under `where … else` | The predicate call, then `Build` of the value inside `Result.ok`, or of the constant error inside `Result.err` | X3-11 |
| `Project` | `Cell`, then `Drop` of the aggregate | X3-11 |
| `Match`, `Try`, `let … else` | `Variant`, `Switch` or `Branch`, `Cell` for binders | X3-12 |
| Generic activations | Type descriptors in slots after the environment (M4 D9.5) | X3-13 |
| `Default` and labelled operands | Defaults in the function value (M4 D2.14) | X3-14 |
| `Call` contract | A new resolve instruction that yields a function value, then `Indirect` | X3-15 |
| `Dict`, `Lookup` | Intrinsic and runtime rows | X3-16 |
| Test assertions | A new assertion instruction | X3-18 |

Removed with the redesign, so no row exists: untagged unions and widening
into them, `as` narrowing, tuple index projection, and variadic operands
(N53). A bare guard has no row either: the checker discharges it and nothing
is lowered.

### Guards

N44. The checker has classified every guard before lowering sees it.

- A bare `requires`, `ensures`, or `where` produces no instruction.
- `requires C else V` is a branch at the top of the body. The false edge
  evaluates `V` and returns it. That edge does not pass through the
  `ensures` check.
- `ensures C else V` moves every return of the body to one exit block. The
  candidate is in a slot; `C` is evaluated once; the true edge returns the
  candidate and the false edge evaluates `V`, with the candidate still
  bound, and returns it.
- A call followed by an `ensures … else` check is a `Call`, not a
  `TailCall`. A call with nothing after it stays a `TailCall`.
- A proven `… else` guard may be removed with its consequence. The result
  type does not change.

### Plans

N45. Applying a `proc` evaluates the callee and the operands and builds a
plan: a function value (N9) of a distinct object kind. It runs no part of the
procedure. `await` is a call through the plan, so it is a checkpoint like
any other call, and it may be a tail call. Each `await` pushes a fresh
activation from the same plan, so a plan is never consumed. A call through a
plan checks the object kind: calling a plan as a function, or a function as a
plan, is the defect trap `@runtime.invalidCheckedProgram`.

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
`@runtime.memoryExhausted` and never as a native stack overflow.

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
the runtime ABI version, the registry major and the derived minor, and that
every host symbol is a registry row. With N46 it also checks the entry's
contract and the driver and policy requirements of the target's bindings.
With `vibra_exec::verify`, this is the admission clause of the M4 slice.

## 4. Values

### The store

One growable block of bytes per context, with the header and payload layouts
of `vibra_ir::arena`: the layout the M4 arena proved and the
toolchain-owned native code already reads (N7). A `Ref` is a 32-bit offset
into its own store: a context-local handle, never a pointer. Blocks are
power-of-two size classes from 32 bytes to 1 GiB with a free list per class.
A cell is 8 bytes plus a class byte (`I32`, `I64`, `F32`, `F64`, `REF`).

Reclamation is precise reference counting with no cycle collector; the runtime
chapter's "Reclamation" gives the argument that values cannot form cycles.
Release walks a worklist threaded through the freed blocks, so it uses no
stack at any depth. Counts are non-atomic because a context is used by one
thread at a time.

A native program has **no host value IDs and no handle table** (N7). In a
native executable the host operations are toolchain Rust in the same process,
and they reach values through `vibra_host::HostValues` over cells.

### Function values, module values, descriptors

- A function value is a `FUNCTION` object `[function][slots][params]
  [captures…]`. A call through it checks the operand count (a mismatch is the
  toolchain-defect trap `@runtime.invalidCheckedProgram`) and passes the
  value itself as the callee's environment (N9). Defaults and type arguments
  join the header in X3-13 and X3-14.
- A module value has three states: unread, evaluating, ready. The first read
  pushes its initializer as a frame; the return stores the value and gives the
  reader its own reference. Each execution starts fresh.
- Generic code receives type descriptors as ordinary values in slots, as the
  interpreter passes type arguments at run time (N15).
- A plan is a function value of a distinct kind (N45).
- A grant is a sealed value: a reference to a broker entry, with the world
  and generation that issued it (N42). The store holds the reference; the
  broker holds the resource, the policy, and the counters. No instruction
  builds a grant from ordinary data, and observation refuses one.

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
step, a yield before or after cannot repeat it.

### Grants

N42, N43. The first operand of every `Host` terminator is a grant. Before
`vibra_host::perform`, `Context::host` does what the runtime chapter's
[host calls](../../spec/06-runtime.md#host-calls) lists, in that order:

1. validate the grant: target kind, world and provider, generation, and the
   registered operation;
2. derive the requested resource and action from the typed operands;
3. enforce the whole policy, inherited constraints included, and reserve the
   shared limits in the broker;
4. call the adapter bound to the target, and keep "not submitted",
   "completed", and "may have run" distinct;
5. advance `resume`, then append the completed event with a safe projection
   of the grant.

A valid request the policy rejects, a limit that cannot be reserved, and an
action the platform cannot enforce are values of the operation's error type.
None is a trap. A stale, foreign, or forged grant is the defect trap
`@runtime.invalidHostValue`. A refusal before the adapter appends no
completed event.

The broker is one object per world. It owns resources and counters without a
strong reference back from them, so a context's reference counts are never
used to synchronize shared counters. The filesystem adapter starts each
invocation at the held directory and keeps no intermediate handle between
invocations.

Traps are the two defect traps of the runtime chapter (N44). Generated code and the
stepper record a trap as code plus origin ordinal in the context and stop. The
program carries an origin table (ordinal *n* is entry *n − 1*; 0 is no
origin). In a build product the table is a data section of the object, and
its identity is the manifest's source-map identity.

## 6. Generated code and the bridge

Each execution-IR function becomes one native function. Its body is a switch
on the frame's `resume` over the function's blocks; a checkpoint terminator
calls the matching transfer and returns a status to the dispatcher. No Vibra
call is a native call, so the native stack holds the dispatcher, one function,
and one runtime routine at any language depth (N2).

The C ABI is owned by `vibra-nexus-bridge`, and that crate's documentation
is the contract (N10, N32):

- A step function is `(ctx, resume) -> status`. The statuses are continue,
  done, trap, exhausted, and, from v0.0.4, wait.
- The linked object exports one symbol the launcher needs, the program
  descriptor. It carries the ABI version, the registry major and minor with
  the required host symbols, the entry's contract (N46), the driver and
  policy requirements of the target's bindings, the functions with their
  block and frame-slot counts, the module-value initializers, and the origin
  table. It carries no identity field.
- There is one runtime routine per transfer and per instruction. Each takes
  the context and scalars, and returns a scalar.

No pointer into the store or the cell stack crosses the bridge in the
baseline: generated code names slots by index and the runtime does the rest
(N11). The bridge validates nothing twice: `vibra_program` is admitted once by
the launcher, and the execution IR was verified before code generation.

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

The launcher has two profiles. **system** ships: the system provider and the
target's binding recipes. **harness** exists only in the test archive: it
reads a `@hostWorld.v2` document, runs under a quota the harness chooses,
and writes one versioned observation document (result, trap and origin,
audit trace, startup record, host event) for the conformance runner to
compare (N18, N47).

Startup is the same in both (N46):

1. Admit the program.
2. Validate the binding recipes carried by the build.
3. Acquire grants in dependency order, ties in the order of their names. A required
   grant that cannot be acquired ends the run before the entry, with
   everything already acquired released in reverse. An optional grant
   refused in the way its provider specifies is bound to none.
4. Build the entry's plan and run it.
5. Release grants in reverse order.

A built executable returns the entry's `ExitCode` to the operating system.
The harness records it. Memory exhaustion, a startup failure, and a defect
trap are separate outcomes and never appear as an application code.

## 8. Parity

There is one expectation per case and the interpreter is the oracle. During
the transition a case is executed by up to three executors:

| Executor | Compares | Exists on |
| --- | --- | --- |
| Reference interpreter | The specification | Everywhere |
| Reference stepper | Lowering against the interpreter | Everywhere, no linker |
| Native executable | Code generation and link against the stepper and the interpreter | Its own platform |

`conformance/native-parity.tsv` has one row per executable case with a column
per executor (`stepper`, `windows`, `linux`, `macos`), each `matched` or
`pending <package>`. A host test fails when the table and the corpus
disagree, a package only moves cells toward `matched`, and `pending` is never
a pass: v0.0.3 M2 exits at zero pending cells (N14).

X2-04 builds this inventory for the corpus of the redesigned language
(N49).

Every native and stepper comparison runs twice: unbounded, and with quota 1.

## 9. Build products (M3)

`vibra build --target <triple>` writes the executable and
`<target>.build.vibon`. The manifest's two identities are separate by
construction: the **semantic input identity** comes from a closed source
snapshot with its pinned dependency, claim, witness, and policy identities
(N48) and does not see the target; the **artifact identity** hashes the
executable and records the triple, CPU baseline, Cranelift and linker
versions, normalized flags, `ABI_VERSION`, the registry version and symbols,
the admission ceiling and grant bindings, and the origin table's hash.
Identities are computed in prerequisite order (inputs, object, executable,
manifest payload, signature), and a payload never includes its own hash. Reproducibility is byte
equality of both files across two clean offline builds in different checkout
paths on one target.

## 10. The actor layer (v0.0.4)

The source surface is specified in X4-S1. This section fixes only the
runtime shape that surface will sit on. The kernel is the substrate. What a
program author meets first is a scoped child with join and a bounded,
ordered parallel map; a long-lived actor is a receive loop with an explicit
lifetime, and parks inside a receive ([decisions.md](decisions.md), O1 and
N50).

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
- **Addresses** are grants of the `ActorEndpoint` kind (N42). An address
  holds the actor, the set of operations it admits, and a bound scope.
  Narrowing builds a new address whose set and scope are no wider. The
  kernel checks both at the dispatch boundary, before the handler sees the
  message. An address never passes through a host adapter, and holding one
  gives the sender none of the receiver's host grants.
- **Transfer** (N51). A grant crosses a context by a delegation the broker
  or kernel checks. The callback of a parallel map crosses as a code
  identity with its immutable captures, under a rule checked before the map
  starts.
- **Scopes.** A scope bounds the children started in it and defines what
  join, cancel, and exit do. Cancelling after a host action was submitted
  does not undo it; the late or unknown outcome stays visible.
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
  the causal path from a send through a handler to its host operation.

## 11. What v0.0.4 extends (the M1 handoff)

| v0.0.3 contract | v0.0.4 extends it by |
| --- | --- |
| Checkpoints: the five transfers | Kernel actions become checkpoints too; `Step` gains `Wait` |
| Quota is unobservable | Unchanged. Fairness uses it; replay does not |
| Handles are context-local offsets, layout private | Unchanged. Messages cross as neutral values, so no handle leaves a context |
| Origins: an ordinal table per program | Message events carry origins of the send and the handler |
| Host dispatch through the closed registry | Unchanged. An address scopes a *request*; the registry and its adapters keep their own contract |
| The sealed authority envelope | The `ActorEndpoint` kind becomes usable, checked at kernel dispatch |
| `ABI_VERSION` | Bumped by the actor transfers |

The split of authority from effect: **holding a grant is the authority to
request; the effect row is what a body may perform.** Both mechanisms exist
from v0.0.2 for host operations. For actors, a sender's row gains only the
messaging operation. The handler's row carries the host operations, and the
binary target's admission ceiling must contain the operations of every body
it can start, so target consent stays complete.
