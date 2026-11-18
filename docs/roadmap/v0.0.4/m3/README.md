# v0.0.4 M3: native workers

**Planned.** The semantics of [M2](../m2/README.md) run on native code, and
contexts run on several CPU workers.

It needs M2 and the qualified native execution of
[v0.0.3](../../v0.0.3/README.md) M2. The work packages are X4-05 to X4-09 of
the [Nexus plan](../../nexus/plan-v0.0.4.md). The runtime already has
contexts and a reserved waiting status; those are where the work starts, not
a qualified scheduler.

## Scope, in slices

1. **The kernel on the native runtime.** A parked activation keeps its
   state in its context's frames. It resumes once, with the kernel's answer.
   Origins are kept. The ABI version is bumped and an older descriptor is
   refused.
2. **Transfer and delegation.** A copied message shares no arena block with
   its source: releasing the sender leaves the receiver intact. A deep copy
   uses no native recursion. A grant is delegated through the broker, never
   as a stored offset. The parallel map's callback crosses with its code
   identity and captures.
3. **Fixed workers and placement.** Placement is chosen before
   initialization. Contexts are pinned. Debug builds check that a context
   has one owner. No migration and no stealing. Placement is visible to
   tooling, and an address does not depend on it.
4. **Parity and stress.** The same simulation schedule on the interpreter,
   the stepper, and the native targets. Bounded actor and mailbox stress.
   Race instrumentation where the platform supports it; a check that could
   not run is reported as not run.
5. **Overlap and cost.** CPU-bound work makes progress on two real workers
   at once. The record states the hardware, the intervals, the comparison
   with one worker, the overhead of copying and of parked memory, and the
   bounds.

## Exit evidence

- Both suites pass.
- Scope cleanup, lifetimes, ordering, backpressure, typed outcomes, and
  authority hold on the interpreter and on native code.
- A context has exclusive access, and no arena offset crosses the broker.
- Replay is defined at kernel actions, not at instruction or quota counts.
  A production run on several workers need not have one global event order.
- The runtime, `unsafe`, and broker boundaries are reviewed. The event and
  schedule schemas are versioned.
- Instrumentation that was not run is not a proof of race freedom.
- The release gate needs a useful library integration and bounded
  concurrency evidence. Neither follows from the fact that workers compile.

## Not in this milestone

A shared arena, migration, stealing, a remote transport, and hard real-time
scheduling. Durable and slow service behavior is
[v0.0.5](../../v0.0.5/README.md).
