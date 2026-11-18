# Vibra v0.0.4

**Planned pre-alpha release.** Vibra programs use real dependencies, agents
reach the toolchain through thin adapters, and programs run bounded local
concurrency on several CPU workers.

| Order | Milestone | Evidence |
| --- | --- | --- |
| M1 | [Libraries and tool adapters](m1/README.md) | A real typed dependency is pinned, inspected, integrated, upgraded, and repaired. The adapters return the same facts as the machine CLI. |
| M2 | [Concurrency semantics and simulation](m2/README.md) | Scoped children, a bounded parallel map, and receive-loop actors run on the interpreter under recorded, replayable schedules. |
| M3 | [Native workers](m3/README.md) | The same semantics on native code, with two workers making progress at the same time. |

## What concurrency looks like

A shared actor kernel is the substrate. It is not what most programs meet
first. The first two library paths are:

- **A scoped child with join.** A scope bounds the children started in it,
  and defines what join, cancel, and exit do.
- **A bounded, ordered parallel map.** It has an explicit bound, returns
  results in input order, and has a sequential fallback with the same
  result. Its callback is pure.

A long-lived actor is a receive loop with an explicit lifetime. The author
of a command-line program is not required to design a mailbox protocol to
use two cores.

## Starting point

Host grants and closed effect ceilings exist from
[v0.0.2](../v0.0.2/README.md). An actor endpoint is one more kind of grant,
checked by the kernel when a message is dispatched; it is not the language's
first capability.

M1 needs only the snapshots and tooling of v0.0.2 and does not wait for
native code. M2 runs on the interpreter and the backend-neutral kernel. M3
needs the qualified native execution of [v0.0.3](../v0.0.3/README.md) M2.
Work that overlaps an unfinished release is recorded as such, and nothing
incomplete is advertised.

The concurrency design and its work packages are in the
[Nexus plan](../nexus/plan-v0.0.4.md).

## Specification first

Before any concurrency code, the specification gains the exact library and
source shapes, diagnostics, and schemas, in one reviewed change (package
X4-S1). The first choice is library types and operation interfaces over the
existing forms; a new keyword needs its own justification. An implementation
package does not invent a name the specification left open.

## Boundaries

- **One kernel.** It owns lifetimes, mailboxes, protocol checks, replies,
  timers, and events. The interpreter and the native runtime both drive it.
  A parked activation has one owner at a time.
- **Cancellation is not rollback.** Cancelling after a host action was
  submitted does not undo it. A late or unknown outcome stays visible.
- **The parallel map promises order, not speed.** Purity does not promise a
  speedup or bounded memory. Its callback crosses to another context under
  its own checked rule for code and captures; the general rule that a
  function cannot cross in a message does not disable it.
- **Workers.** A fixed pool of operating-system threads. Placement is chosen
  before initialization. A context is pinned and has one owner. No work
  stealing and no migration. Reference counts stay non-atomic.
- **Transfer.** A message is copied as neutral data. A grant is delegated
  through the broker. No arena offset crosses a context.
- **Wait cycles.** An acyclic import graph does not rule out two contexts
  waiting on each other at run time. A run in which nothing can proceed is
  reported.

## Release gate

**Libraries.** A real dependency is integrated and then upgraded. The record
states the costs of conversion, of modelling its behavior, and of the
assurance it needed, and where its authority begins and ends.

**Concurrency.** Cases cover nested bounded fan-out, an early failure,
cancellation before and after dispatch, a late reply, backpressure, a
timeout, and exit while a caller waits. A full mailbox, a closed mailbox,
and a duplicate reply are typed outcomes. Nothing proves statically that a
handler replies.

**Replay.** A recorded simulation schedule replays on the interpreter and on
all three native targets, with equal outcomes, message and host events, and
origins. An invalid, truncated, or over-long schedule fails replay
explicitly. A production run on several cores need not reproduce the
simulation's event order; it must keep the promised ordering and safety.

**Overlap.** A separate fixture on real hardware shows two workers making
progress at the same time.

Both suites and the schema, diagnostic, and approval regressions pass before
tagging.

## Not in this release

Distribution, live migration, work stealing, implicitly shared heaps, a
linear type that forces a reply, a proof of deadlock freedom, exactly-once
external effects, and crash recovery. [v0.0.5](../v0.0.5/README.md)
qualifies service behavior and recovery. Native user interfaces remain
future work.

Each milestone needs an approved contract and a step plan under the
[execution model](../execution.md) before its implementation begins. No gate
of this release is evidenced yet.
