# Vibra v0.0.5

**Planned pre-alpha release.** A Vibra service stays correct when its
requests are slow, refused, cancelled, or answered late, and a workload that
needs it can recover after a crash.

| Order | Milestone | Evidence |
| --- | --- | --- |
| M1 | [Service lifecycle and slow outcomes](m1/README.md) | A document service handles a late success, a refusal, a timeout, a cancellation, and an exit without corrupting its revisions. |
| M2 | [Journal and recovery profiles](m2/README.md) | An explicit choice of none, ring, or durable, with a crash and reconciliation contract and stated limits for external effects. |
| M3 | [Bounded exploration and release](m3/README.md) | Evidence over a stated state space, replay across executors, a corpus on a real driver, and the release audit. |

## Starting point

[v0.0.4](../v0.0.4/README.md) supplies typed protocol outcomes, scopes and
lifetimes, bounded mailboxes, simulation, and qualified native workers.
Authority and approval, parity between the interpreter and native code, and
build provenance stay as regression gates. New behavior is specified, with
its diagnostics and schemas, before it is built.

Recovery is opted into for a stated workload. A journal is not imposed on
every effect.

## Release gate

**The document service.** One context owns the state and another persists
it. A save of revision 1 is requested. The document is edited to revision 2
while that write is pending. Then the old success arrives. The saved marker
may become 1; revision 2 stays unsaved. A mutant that applies a late
acknowledgement to the current revision must fail the case.

**Failure paths.** A full mailbox, a closed mailbox, and a refused request;
a duplicate reply; exit while a caller waits; cancellation before and after
dispatch; a timeout; and a completion whose outcome is unknown.

**Recovery.** For one selected durable workflow, the process is crashed at
named points and restarted with pinned code, policy, and driver identities.
Outcomes that are unknown are reconciled explicitly.

**Replay.** Recorded simulations are compared on the interpreter and on
native code. Real runs on several cores satisfy the protocol's properties;
they are not required to follow one event sequence.

Before tagging, the record states the commands, versions, states explored,
gaps, and the counts of both suites. Expectations stay independently written
and the oracle's code stays protected.

## Not in this release

Exactly-once external effects, a proof of deadlock freedom, distributed
consensus or deployment, automatic migration of a schema or a nominal type,
a hidden retry, and a general proof that a service is correct. User
interfaces remain future work.

Each milestone needs an approved contract and a step plan under the
[execution model](../execution.md) before its implementation begins. No gate
of this release is evidenced yet.
