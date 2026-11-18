# v0.0.4 M2: concurrency semantics and simulation

**Planned.** Concurrency is specified, then built on the interpreter and a
shared kernel, under a deterministic scheduler whose schedules can be
recorded and replayed.

The work packages are X4-S1 and X4-01 to X4-04 of the
[Nexus plan](../../nexus/plan-v0.0.4.md). Part of the kernel (mailboxes,
lifecycle, reply slots, the scheduler core) is already in the workspace. It
is a starting point, not evidence for this milestone.

## The specification comes first

Package X4-S1 is one reviewed change to the specification. It defines:

- scoped start, join, cancel, and what a scope does at its exit;
- the bounded, ordered parallel map;
- the lifetime of a receive-loop actor;
- which values, captures, and grants may cross to another context;
- the operation set of an endpoint type;
- mailbox bounds and order; a send that is refused, or that meets a full or
  closed mailbox;
- reply slots and a duplicate reply; exit while a caller waits; timeouts; a
  run in which nothing can proceed;
- admission by the target of the host operations of every body a program
  can start;
- the report of the causal path from a send, through a handler, to a host
  operation.

No placeholder API is advertised before that change is approved.

## Scope, in slices

1. **Kernel and simulation.** Bounded mailboxes, first-in first-out per
   sender and receiver. One owner per activation. Virtual time.
   Deterministic actions. Each waiter is resolved at most once.
2. **Scoped children and join.** Nested scopes, a bound on admission,
   cleanup on failure and on exit, cancellation before and after a
   submission, and late outcomes. No child is left running after its scope
   has completed.
3. **Typed protocols and authority.** Sending an operation the endpoint
   type does not admit is a type error. A request outside the endpoint's
   scope is denied by the kernel. Host grants and endpoints are disjoint
   kinds. Narrowing never widens rights or scope, and never resets an
   inherited limit.
4. **The parallel map.** Results correspond to inputs in order. The
   configured bound holds. The sequential fallback gives the same result.
   The callback and its captures cross under the checked rule. The empty
   input and an early failure are covered.
5. **Interpreter and replay.** Real source parks at kernel actions.
   Outcomes and origins are as expected. A schedule that is impossible, cut
   short, or has decisions left over is a replay error, not a program
   failure.

## Exit evidence

- Each kernel rule has a source-level case that succeeds and one that is
  rejected or fails with a typed outcome.
- The cases include a send to oneself, a full and a closed mailbox, a
  duplicate reply, an actor exit, a timeout, a blocked run, and a program
  with no actor at all, which must behave as before.
- Seeded schedule campaigns state their finite domains and assumptions.
  They do not claim to cover every interleaving.
- Both suites, the schemas, and the diagnostics pass for the supported
  profile.

Passing this milestone says nothing about service recovery.

## Not in this milestone

Distribution, any claim about native-worker speed, a durable journal,
revocation, linear ownership of a reply, and rollback of an external effect.
