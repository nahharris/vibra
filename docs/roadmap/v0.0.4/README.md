# Vibra v0.0.4

**Planned pre-alpha release.** Ship typed local actors as the model for
long-lived private state and effectful services. Separate the authority to
send a request from the effects its handler can execute. Start after
[v0.0.3](../v0.0.3/README.md) qualifies Nexus and completes the Wasm cutover.

## Delivery order

1. **Specify local actors and message authority.** Define typed mailboxes,
   scoped send addresses, one-shot replies, ownership and transfer, and
   target effect admission. Define ordering, full and closed mailboxes,
   actor exit, and timeout semantics. Update the affected charter,
   language, effects, runtime, tooling, and conformance contracts before
   implementing them.
2. **Prove the local model in simulation.** Run one message handler at a
   time per actor. Carry immutable state between turns and share immutable
   snapshots. Use bounded mailboxes and deterministic scheduling with
   recorded park, wake, delivery, reply, and exit events. Surface source
   origins for the send, handler, and associated host operation.
3. **Run local actors on Nexus workers.** Use the same actor semantics and
   replay cases with native execution. Demonstrate that two actors can run
   concurrently on separate CPU workers while preserving single ownership
   of each actor's state. Keep actor addresses independent of worker
   placement. Select placement before initialization and expose actual
   placement through tooling.

## Capabilities and effects

Define the authority rules during the Nexus design review and implement them
with actors. The first capability is a typed, unforgeable send address. A
holder can request the operations exposed by that address. Passing the
address delegates the same authority. Specify how an actor can issue a
narrower address and require the dispatch boundary to enforce its operation
and resource scope.

Use a persistence actor that issues a save address for one document and keeps
its broader administrative protocol separate. The sender's performed row
contains the messaging effect. The handler's row contains the filesystem
write, and the target still declares the host effects of the actor bodies
it can start. Show both the sender's direct effect and the causal path
through the handler in tooling. An address scopes the request. The host
registry and its platform adapters retain their own execution contract.

Holder binding, revocation, and general host-resource capabilities are
separate designs. Reopen them when a concrete protocol requires authority
that cannot be forwarded, must be withdrawn, or must constrain the host
operation itself. Distribution, live migration, general mutable references,
and a separate CPU fork/join API are also follow-on work.
[Live debugging](../scratchpad/agent-tooling/debugger.md) will use the
execution and actor contracts proved by this release.

## Release gate

Use one local document service fixture with a state actor and a persistence
actor. Send a save for one revision, edit the document while the write is
pending, and show that the reply cannot mark the newer revision as saved.
Exercise an attempted out-of-scope save, a full mailbox, a duplicate reply,
and an actor exit while a caller waits. Reject a direct write outside the
caller's effect ceiling and reject a target that omits the handler's
required effect.

Replay the same recorded fixture schedule in the interpreter and native
execution on Windows x64, Linux x86-64, and macOS arm64. Compare outcomes,
message and host events, and source origins. In a separate native fixture,
show overlapping progress on two CPU workers. Run both the host-language
suite and the independent Vibra conformance corpus before tagging.
