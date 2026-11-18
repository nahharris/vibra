# v0.0.5 M1: service lifecycle and slow outcomes

**Planned.** What is qualified here is a service protocol, not the fact that
an actor can be declared. It needs the scopes, typed protocols, and kernel
of [v0.0.4](../../v0.0.4/README.md).

## Scope, in slices

1. **The protocol contract**, specified first. A revision and a correlation
   identity for each request. The states accepted, in flight, succeeded,
   failed, and unknown. A bound on pending requests. Startup, shutdown, and
   cleanup. Versioned messages and events.
2. **The document-save workflow.** Save revision 1, edit to revision 2
   during the write, and a late success marks only revision 1. A mutant
   that ignores the correlation fails.
3. **Failure and lifecycle.** A request that meets a full or closed
   mailbox, or falls outside the endpoint's scope. A duplicate reply. The
   exit of a waiter and of a worker. A timeout. Cancellation before and
   after dispatch. Explicit handling of a late outcome.
4. **A real driver.** A small persistence corpus on the actual filesystem
   adapter under scoped grants: refusals, and outcomes the model covers and
   does not cover. The record states what was sampled, how often, and what
   that does and does not assure.

## Exit evidence

- Every transition has independently written source cases with typed
  outcomes.
- Pending work, queues, and mailboxes have explicit bounds.
- A timeout or a cancellation does not mean a submitted write did not
  happen. The cases show it.
- Scope cleanup states, for each local task, whether it is joined, ended, or
  kept on record, and the information about the external outcome survives.
- Fixed virtual-time schedules reproduce the outcomes and causal origins on
  the interpreter and on native code.
- Both suites, the schemas, and the approval and driver checks pass.

Nothing here implies durability across a crash. That is
[M2](../m2/README.md), for a selected profile.

## Not in this milestone

A framework for remote services, a hidden exactly-once reading of any
outcome, a detached task without a bound, general deadlock detection, and
bringing state back after its scope has exited.
