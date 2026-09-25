# Concurrency and distribution

Status: design direction; not normative
Lines: processes, supervision, simulation, and data parallelism in
[Vibra 2](README.md#vibra-2--concurrent-services); distribution in
[Vibra 3](README.md#vibra-3--distributed-and-proven)
Prerequisites: [effect-row polymorphism](foundations.md#effect-row-polymorphism),
[process-owned resources](foundations.md#resources-and-handles),
[reductions](foundations.md#execution-budgets-and-reductions)

## Why the BEAM model

Erlang and Elixir showed that fault-tolerant services come from a few
composable ideas: isolated lightweight processes, message passing, "let it
crash", supervision trees, and location transparency. Vibra v1 already has the
conditions those ideas need:

| BEAM idea | Vibra v1 property it builds on |
| --- | --- |
| Share-nothing processes | Immutable values; no references, identity, or shared mutable state |
| State threaded through a receive loop | Mandatory tail calls, so a process loop is an ordinary tail-recursive function |
| Crash on unexpected input | Traps are uncatchable, and `result` errors from a process body become exit reasons |
| Pattern-matching receive | Exhaustive `match` over nominal enums and unions |
| Explicit side effects | Static effect rows, so spawning, sending, and receiving are checked roots |
| Reproducible debugging | Deterministic semantics given ordered host responses |

Vibra adds what the BEAM lacks: **typed mailboxes, statically checked effects
in every process, and deterministic simulation as a first-class test mode.**

### What Vibra does not take

- **Dynamic typing of messages.** Every mailbox has one nominal message type.
- **The process dictionary and ETS-style shared mutable tables.** Shared state
  is a process. A read-mostly value can be published as an immutable snapshot
  (like `:persistent_term`), never mutated in place.
- **Shipping closures between nodes.** Code does not travel. Only `wire` data
  does.
- **Macros** such as `use GenServer`. Behaviours are interfaces.
- **Hot code loading**, until the [horizon](#hot-code-upgrade-horizon).
- **`async`/`await`.** Processes block cheaply, so functions have no colors.
  The effect row already records which functions can suspend: any function
  whose row contains `process.receive`.

## Processes and typed mailboxes

A process runs one function to completion in isolation. It receives its own
**inbox**, a process-owned handle typed by its message type, as a parameter:

```vibra
(deftype counter-msg
  (enum
    add u64
    get (reply-to u64))
  visibility: @public)

(defn counter (inbox (inbox counter-msg) total u64) (result void counter-error)
  effects: (process.receive process.send)
  (match (inbox.receive inbox)
    (envelope.message (counter-msg.add n))
      (counter inbox (try (checked-add total n)))
    (envelope.message (counter-msg.get reply))
      (do (reply.send reply total) (counter inbox total))
    -
      (counter inbox total)))
```

- `(process.spawn body)` accepts a function `(fn ((inbox m)) (result void e)
  effects: (r))` and returns `(pid m)`, a typed address. Spawning performs
  `process.spawn` plus the body's row `r`, so a target's effect array still
  covers every process it can start. This is why effect-row polymorphism is a
  prerequisite.
- `(process.send pid message)` is asynchronous and never blocks. A message is
  an immutable value, which the runtime may share or copy freely.
- `(inbox.receive inbox)` blocks until the next envelope. `timeout:` bounds the
  wait. `inbox.receive-select` takes a pure selector
  `(fn (m) (option r))` and leaves non-matching messages queued, which is
  selective receive without scanning semantics in the language.
- Only the owning process can receive from an inbox, by the
  [resource ownership rule](foundations.md#resources-and-handles). The
  inbox parameter, not ambient state, gives a function the right to receive.
- Messages from one sender to one receiver arrive in send order. No other
  ordering is promised.

### Envelopes

A receive returns `(envelope m)`, a standard enum, so system signals stay
typed without polluting user message types:

| Variant | Meaning |
| --- | --- |
| `message m` | An ordinary message |
| `down down-info` | A monitored process exited, with its `exit-reason` |
| `exit exit-info` | A linked process exited, delivered only when this process traps exits |
| `timeout void` | The `timeout:` elapsed |

### Typed request and reply

`(reply-to r)` is a one-shot, process-transferable handle. It is the only
channel type, and it replaces untyped `from` tuples:

```vibra
(process.call counter-pid
  (lambda (reply (reply-to u64)) counter-msg (counter-msg.get reply))
  timeout: 5000u64)
```

`process.call` returns `(result u64 call-error)`, where `call-error`
distinguishes timeout, a dead callee, and a callee that exited without
replying. The callee cannot reply with the wrong type.

## Failure, links, and monitors

A process ends with an `exit-reason`:

| Outcome of the body | Exit reason |
| --- | --- |
| Returns `(result.ok void)` | `@normal` |
| Returns `(result.error e)` | `(exit-reason.error data)`, where `data` is `e` encoded as VIBON |
| Traps | `(exit-reason.trap code origin)` |
| Killed by a link or supervisor | `@killed` |
| Exceeds a reduction, mailbox, or heap budget | `(exit-reason.budget kind)` |

"Let it crash" is therefore typed: a process's declared error type is the set of
expected failures, and a trap is the unexpected one. Neither is caught by the
process itself.

- `process.monitor` is one-way and delivers `down`.
- `process.link` is bidirectional. An abnormal exit kills linked processes
  unless they trap exits, in which case they receive `exit`.
- Both are operations under a `process.link` root.

## Supervision and behaviours

Supervisors are standard-library processes, not language features:

```vibra
(supervisor.start
  strategy: @one-for-one
  max-restarts: 3u32
  within-ms: 5000u64
  (supervisor.child id: @store start: start-store restart: @permanent)
  (supervisor.child id: @http start: start-http restart: @permanent))
```

- Strategies follow OTP: `@one-for-one`, `@one-for-all`, `@rest-for-one`.
  Restart policies are `@permanent`, `@transient`, and `@temporary`.
- The supervisor's row is the union of its children's rows. Putting children
  with different rows into one variadic array needs one more foundation rule:
  a function value widens to a *larger* written effect row at a typed boundary.
  This is sound, because a row bounds what may happen, and it follows the
  charter's "widening is declared at a written expected type" principle.
- OTP behaviours become interfaces. A `server` interface with `init`,
  `handle-call`, `handle-cast`, and `handle-info` members, parameterized by
  message type and row, lets `server.start` run the receive loop, replies,
  and timeouts for any implementing state type.
- A `@service` target kind names a root supervisor instead of an entry
  function. Its effect array is the consent for the whole tree.

## Scheduling

- The runtime runs M:N: many processes on a pool of scheduler threads.
- Preemption happens at function applications, counted in
  [reductions](foundations.md#execution-budgets-and-reductions). The
  interpreter counts the same reductions, so budgets and fairness behave the
  same in both backends.
- A blocked receive, a call, and a timer wait suspend only the current process.
  [Execution targets](execution-targets.md#processes-over-webassembly)
  describes how suspension is implemented over Wasm.

## Determinism and simulation testing

V1 promises determinism for fixed source, inputs, and ordered host responses.
Concurrency keeps that promise by treating **every scheduling decision, timer
firing, and delivery as an ordered host response**:

- In production, the scheduler makes those choices freely. Optionally, it
  records them in the audit trace.
- In `vibra test`, a concurrent test runs under a single-threaded
  **simulation scheduler** that draws every choice from a seeded source and
  uses virtual time. Timers fire instantly in virtual time.
- A test may request exploration, for example `schedules: 1000u32`. The runner
  tries that many interleavings, using random and priority-change strategies,
  and can inject faults: kill a process, delay or drop a timer, fail a host
  operation, or exhaust a budget.
- A failure reports its seed and the schedule as VIBON. `vibra test --replay`
  reruns exactly that execution, in either backend, with identical results.
- Given one schedule, the interpreter and Wasm produce identical outputs and
  traces. That is the concurrency form of the v1 parity gate.

This follows FoundationDB- and TigerBeetle-style deterministic simulation. For
agents it is the most important part of this track: concurrency bugs become
reproducible, shrinkable test failures instead of flaky runs.

## Data parallelism

Pure computation can run in parallel without any observable difference, so it
needs no effect root and no process:

- `par.map` over an array with a pure callback returns results in input order.
- `par.reduce` requires a `monoid` implementation. Its associativity is a
  documented law in line 2 and a proven one from
  [Verification V2](verification.md#v2--lemmas-and-interface-laws).
- `par.both` evaluates two pure thunks.
- If several elements trap, the trap from the lowest index is reported, so
  failure is deterministic.
- The runtime chooses chunking and may run sequentially. The interpreter
  always runs sequentially.

Parallel points are explicit in the first design. Bend and HVM show that pure
functional code can be parallelized automatically, through interaction nets.
That remains a [horizon](README.md#horizon) study. An explicit `par` keeps a
predictable cost model, which matters most when each worker is a separate Wasm
instance and values are copied between instances.

## Distribution (Vibra 3)

### Nodes and addresses

- A node is one runtime instance with a cryptographic identity. Transport is
  authenticated and encrypted with node certificates, not a shared cookie.
- `(pid m)` is location-transparent. Sending to a remote pid requires `m` to be
  a **wire type**.
- Connecting and listening are effect roots (`node.connect`, `node.listen`),
  so a program that can join a cluster says so in its target array.
- Cluster membership starts as static, declared configuration. Discovery
  mechanisms are later additions.

### Wire types

Conformance stays explicit. A `deftype` opts in with an attribute such as
`wire: @v1`. The checker verifies that every component is itself wire data:
primitives, collections, records, enums, unions of wire types, and pids.
Functions, inboxes, and other handles are never wire.

- The encoding is a binary form of VIBON, carrying each type's canonical
  identity and a structural fingerprint.
- Evolution rules state which changes stay compatible (for example adding an
  enum variant that old receivers treat as unknown, or adding a record field
  with a default). An incompatible message is rejected at the receiving node
  with a typed `down`/error. It is never decoded as a different type.
- Remote `reply-to` handles are wire, so `process.call` works across nodes.

### Failure semantics

- Delivery is at-most-once. Ordering is per sender–receiver pair while a
  connection lasts.
- A remote monitor delivers `down` with `@noconnection` when the connection
  is lost, as in Erlang.
- Distributed supervision, process groups, and a cluster-wide registry are
  standard-library processes over these primitives.
- The simulation scheduler extends to networks: partitions, delay,
  reordering, duplication at the transport layer (hidden by deduplication),
  and node crash and restart. All of these are driven by the seed and
  replayable.

### Hot code upgrade (horizon)

The BEAM upgrades code in place. With static types, a state upgrade needs a
typed migration. The natural design uses v1's `from` interface: a new state
type implements `(from old-state)`, and a release upgrade is admitted only if
every live process's state type has a migration path. It depends on build
fingerprints and stable identities, which v1 already provides, and it stays
on the horizon until services exist to motivate it.

## Open questions

- Mailbox bounds: unbounded, as in Erlang, or bounded by default with a typed
  overflow exit reason? Bounded is safer, and backpressure through `call` is
  the idiomatic remedy.
- Whether large binaries should be shared by reference across processes on one
  node, as on the BEAM, and how that is kept unobservable.
- How much of OTP's `application` concept belongs in `project.vibon`, and how
  much in source.
- Whether `receive-select` is needed at all, given that typed envelopes remove
  most of Erlang's reasons for selective receive.
