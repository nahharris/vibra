# Async host operations and structured concurrency

Status: design contract for issue #105. This document defines observable
semantics; it does not select an executor, native threads, Wasm threads, or a
shared-memory synchronization model.

> **Note:** `CapabilityGrant` is shared infrastructure for structured
> concurrency and the #253 runtime authority model. Async scopes attenuate
> parent grants monotonically; #251's resource budgets remain a separate
> concern and are not part of this contract.

The machine-readable trace format is
[`async-task-trace.schema.json`](../../schemas/async-task-trace.schema.json). The
typed host-adapter boundary is
[`async-host-operation.schema.json`](../../schemas/async-host-operation.schema.json).
The
normative examples are
[`async-task-vectors.json`](../../tests/conformance/async-task-vectors.json).

## Goals and non-goals

The first implementation must permit several host I/O operations to be in
flight concurrently, remain deterministic under a test scheduler, and make
task/resource lifetime explicit. Parallel execution, preemption, mutexes,
atomics, unscoped task detachment, and cross-task mutable aliasing are out of
scope. An implementation may initially use one thread and a cooperative event
loop.

## Terms and state machines

A **scope** owns tasks and resources. The root invocation is a scope. Opening a
child scope creates a cancellation tree. A scope cannot complete until every
child task has reached a terminal state and every scope-owned resource has been
closed or transferred to its parent by an explicit API.

A task moves exactly once through:

```
created -> runnable -> waiting -> runnable -> completed(value)
                                 |          -> failed(error)
                                 +----------> cancelled(reason)
```

`waiting` and `runnable` may repeat. `completed`, `failed`, and `cancelled` are
terminal. A join handle is affine: it may be joined once. Dropping an unjoined
handle does not detach its task; the scope still owns and joins it at scope
exit. Implementations should diagnose an explicitly discarded handle.

Cancellation tokens move monotonically from `active` to `cancelled(reason)`.
The first cancellation reason wins. Cancellation is a request until the task
reaches a cancellation point: host-I/O suspension, channel send/receive,
explicit `yield`/`check-cancelled`, or child join. Pure computation is not
preempted in v1; linting or a later fuel mechanism may address starvation.

## Propagation, joins, and failure

- Cancelling a scope cancels all non-terminal descendant tasks and pending
  operations, depth first in task creation order for trace determinism.
- Child failure does not silently cancel siblings. It is retained by the scope
  until joined. On scope exit, the earliest unobserved failure by creation
  sequence becomes the scope failure and remaining live children are cancelled
  with `sibling-failed`.
- `join` returns `result<T, task-error<E>>`, distinguishing `failed(E)`,
  `cancelled(reason)`, and `resource-limit`. Joining is not an exception edge.
- Joining an already-terminal task completes immediately but still emits one
  `join-completed` event. A task may not join itself or an ancestor.
- Scope cancellation outranks a simultaneous successful wakeup at the same
  logical instant. An already-recorded terminal result is never rewritten.
- A parent may explicitly implement fail-fast behavior by cancelling its scope
  after a failed join; this is library policy, not implicit scheduler policy.

## Deadlines and clocks

A deadline is an absolute instant from the scope's injected monotonic clock.
It is inherited as `min(parent, requested)`, so a child cannot extend its
parent's lifetime. At the deadline the scope is cancelled with `deadline`.
Equal-time events are ordered: existing cancellation, deadline expiry, I/O
completion, task creation sequence, then operation sequence. Wall time and
timezone never participate. Production hosts map their monotonic clock into
this contract; tests advance a fake clock explicitly.

## Capabilities

A child begins with an immutable snapshot no greater than its parent's current
capability set. Spawn may attenuate this set by intersection and narrowing of
resource constraints; amplification is rejected before task creation.
Revocation of a scope grant cancels affected pending host operations with
`capability-revoked` and prevents new ones. Handles are not authority: using a
resource still checks the task's grant and the resource's scope ownership.
Capability values are immutable and freely shareable; policy mutation remains
owned by the host/scope.

## Mutable references and ownership

No mutable reference may cross a task boundary in v1. Spawn arguments must be
immutable/shareable values or values moved into the child. A moved value is no
longer accessible to the parent until returned by join. Resource handles follow
the same affine rule unless their type explicitly declares host-serialized
sharing. This restriction is required even in a single-thread executor: it
preserves the option of parallel execution without retrofitting data races.

Compiler acceptance should reject captured `&mut`, mutable cells, and mutable
container aliases. Immutable snapshots, frozen persistent values, capability
snapshots, channel endpoints, and explicitly shareable host resources are
accepted. This design does not define `Send`/`Sync` or shared-memory locks.

## Async host I/O

Starting an operation validates capability, ownership, deadline, and resource
limits synchronously, then returns a task-local operation token. Completion is
delivered exactly once. Cancelling a pending operation asks the host to cancel;
late host completion is consumed and discarded, never delivered to a reused
token. Tokens contain a generation to prevent ABA reuse.

Hosts that cannot cancel an operation must retain the backing resource until
the late completion is drained, while the Vibra task may become terminal
immediately. Such operations count against the scope's `draining-operations`
limit. Closing a resource cancels its pending operations before emitting
`resource-closed`. Concurrent reads/writes are permitted only where the
resource contract defines their ordering; otherwise the second start returns
`busy`.

## Leaks and resource limits

Each scope has host-configured maxima for live tasks, pending operations,
open resources, draining operations, and channel buffer elements/bytes. Limits
are inherited and may be attenuated. Admission failure is synchronous and does
not create a partial task/operation/resource. Scope exit cancels live work,
drains host completions, closes resources in reverse creation order, and then
reports leaks. Test mode must fail on any leak; production may additionally
emit diagnostics but may not silently detach work.

## Deterministic test scheduler

The test scheduler has a fake monotonic clock and a scripted completion queue.
It assigns increasing task and operation sequence numbers. At each step it
processes the lowest logical instant, applies the equal-time precedence above,
and runs the lowest-sequence runnable task until its next suspension point.
Tests assert the canonical event trace, not executor polling details. No test
may depend on wall-clock sleeps. The vectors include concurrent completion,
cancellation races, deadline races, attenuation rejection, cleanup, and limits.

## Typed channels evaluation

Channels are worth prototyping as two affine endpoints, `sender<T>` and
`receiver<T>`, usable only inside a common ancestor scope. Send moves `T`;
receive returns it. Bounded channels provide backpressure; capacity zero is a
rendezvous. Closing all senders makes receive return `closed`; closing the
receiver makes send return `closed`. Waiting operations are cancellation points
and are selected FIFO by operation sequence.

The prototype must prove type preservation, move/alias safety, deterministic
close and cancellation behavior, bounded-memory accounting, and select/fan-in
semantics before channels become stable. A channel is a scheduler primitive,
not evidence of native threads. Cross-instance/process transport, shared-memory
payloads, fairness beyond deterministic FIFO, and an unbiased nondeterministic
`select` are deferred.

## Independently testable implementation milestones

The first executable slice lives in `src/async_runtime.rs`. It implements the
single-threaded fake-clock scheduler, typed task outcomes, affine join handles,
scope cancellation and cleanup, deadlines, and hierarchical capability
attenuation. It is an embedding prototype rather than a stable Vibra source or
host ABI. Async host-operation tokens, compiler alias diagnostics, admission
limits, and language syntax remain subsequent milestones.

The second slice adds the typed `AsyncHostAdapter` command/completion boundary,
generation-tagged operation tokens, deterministic completion ordering, and
task wakeup. Cancellation distinguishes hosts that guarantee cancellation from
those requiring late-completion draining. Scope limits reject admission before
allocating partial state, while cleanup reports draining operations and retained
resources and closes them after the final completion. The adapter remains
single-thread compatible: production event loops and deterministic fake hosts
implement the same interface.

The source-level foundation is `$task: [capture, ...]` plus `do:`. It creates a
structured child with explicit immutable snapshots and an implicit join at the
block boundary. The compiler rejects mutable and reference-typed captures with
`E-TASK-001`, and task-local control cannot escape the boundary. This provides
an alias-safe boundary.

The next executable slice adds `$spawn: handle` with explicit `captures:` and
a typed result `value:`, plus `$join: handle` / `into: result`. Multiple handles
can overlap and can be joined out of spawn order. The compiler treats handles
as opaque affine values: they cannot be copied or captured, every control-flow
path must consume the same handles, and no handle may leave its scope unjoined
(`E-TASK-003`). The interpreter registers task creation and joins with the
deterministic `Scheduler`; the Wasm backend retains the terminal computation
result in a compiler-owned handle local.

1. **Trace and scheduler:** implement identifiers, event serialization, fake
   clock, scripted completions, and ordering. Pass vectors `ordering-*`.
2. **Structured tasks:** scope ownership, affine join, retained failures, and
   recursive cancellation. Pass `cancel-*`, `join-*`, and leak checks.
3. **Authority and values:** spawn-time attenuation and compile-time rejection
   of mutable aliases. Add compiler diagnostics plus positive move/snapshot
   tests; pass `capability-*`.
4. **Async host adapter:** generation-tagged operation tokens, close/cancel
   races, late-completion draining, and injected host. Pass `io-*` and
   `deadline-*` using at least two concurrent operations.
5. **Limits and cleanup:** admission accounting and deterministic scope-exit
   cleanup. Pass `limit-*` and `leak-*` under sanitizer/instrumented hosts.
6. **Experimental channels:** gated API with affine endpoints and bounded FIFO.
   Pass `channel-*`; publish a separate stabilization decision. This milestone
   does not block the task/I/O prototype and commits to no thread model.

Issue #105 is not implementation-complete until milestones 1–5 exist in the
compiler/runtime and a prototype demonstrates multiple concurrent host I/O
operations. This document, schema, and vectors complete only its design and
conformance-contract portion.
