# v0.0.4 work packages

Read [README.md](README.md#how-to-delegate-a-work-package) first. Decisions
are in [decisions.md](decisions.md), and the runtime shape in
[design.md](design.md#10-the-actor-layer-v004).

The actor kernel is the substrate. The surface a program author meets first
is a scoped child with join and a bounded, ordered parallel map; long-lived
actors are receive loops with explicit lifetimes (N50). An address is a
grant of the actor-endpoint kind.

These packages are milestones M2 (stages 1 and 2) and M3 (stage 3 and the
gate) of [v0.0.4](../v0.0.4/README.md). Its M1, dependencies and tool
adapters, is not Nexus work and has no package here.

v0.0.4's concurrency is specified before it is built. Until X4-S1 is
approved, X4-01 may proceed on the parts of the kernel that every candidate
surface shares (it lists them), and nothing else starts. Packages X4-01 to
X4-04 need nothing from v0.0.3. X4-05 onward needs v0.0.3 M2 (X3-22).

| Package | Stage | Depends | Size | Agent |
| --- | --- | --- | --- | --- |
| [X4-S1](#x4-s1--concurrency-specification) | 1 | — | L | spec: draft only |
| [X4-01](#x4-01--actor-kernel-and-simulation-scheduler) | 2 | X4-S1 (partly) | M | careful |
| [X4-02](#x4-02--checker-scopes-endpoints-and-effects) | 2 | X4-S1 | L | careful |
| [X4-03](#x4-03--interpreter-executor) | 2 | X4-01, X4-02 | M | careful |
| [X4-04](#x4-04--replay-cases-and-causal-tooling) | 2 | X4-03 | M | careful |
| [X4-05](#x4-05--nexus-executor) | 3 | X4-01, X3-22 | M | careful |
| [X4-06](#x4-06--native-lowering-and-code-generation-for-concurrency) | 3 | X4-05 | M | mechanical |
| [X4-07](#x4-07--workers-and-placement) | 3 | X4-05 | M | careful |
| [X4-08](#x4-08--concurrency-fixture-and-release-gate) | gate | X4-04, X4-06 | M | careful |
| [X4-09](#x4-09--two-worker-overlap-fixture) | gate | X4-07 | S | careful |

---

## X4-S1 · Concurrency specification

**Kind:** specification prerequisite.
**Lands:** N50 and N51, the answers to O1 and O2 (receive loop, parked
waiting), N25, and N19–N22 where they are observable. O4 is already closed
(N43).

**Deliver** a draft for Hannah, as one change across the charter (the
exclusion of tasks, channels, and threads is narrowed to what is added), the
language and type chapters (only if a new form is needed: the first choice is
library types and operation interfaces over the existing forms), the effects
chapter (the operation interfaces for scopes and messaging, target admission
of every body a program can start, grants of the actor-endpoint kind and
their narrowing), the runtime chapter (scope lifetime, join, and cancel;
mailbox bound and order; full and closed mailboxes; exit; timeouts; one-shot
reply and duplicate reply; what may cross a context; the event kinds of the
audit trace; the schedule document), tooling (causal path queries, placement
facts), and conformance (the schedule-replay case form).

No keyword or placeholder function is chosen by an implementation package.
A name the draft leaves open blocks the package that needs it.

The draft must answer, with a MUST and a conformance case each:

1. What a scope guarantees at its exit: which children are joined, which
   are cancelled, and what happens to an outcome that arrives late.
2. What cancelling means before a host action is submitted and after it. A
   submitted action is not undone.
3. What the parallel map promises: its bound, the order of its results, its
   sequential fallback, and what an early failure does to the rest.
4. Which captures of a parallel-map callback may cross, and that an
   unsupported capture is rejected before the map starts (N51).
5. What a send returns when the mailbox is full, when the actor has exited,
   and when the address does not admit the operation or scope.
6. What ordering is promised (the design assumes per sender–receiver FIFO and
   nothing else).
7. What a second reply to a one-shot reply slot is.
8. What a waiting caller observes when the actor exits, and what "timeout"
   means under virtual time.
9. What exactly may cross in a message, and how a grant is delegated.
10. What is recorded in a schedule and what makes a replay *invalid*.
11. What a run reports when nothing is runnable and no deadline is pending.
    An acyclic import graph does not rule out a run-time wait cycle.

**Validation:** every numbered item has specification text, a diagnostic or
typed outcome, and a named conformance case; the charter's exclusions list
and the effects chapter's inventory agree with the new surface; Hannah
approves.

## X4-01 · Actor kernel and simulation scheduler

**Decisions:** N19, N21.

**Deliver** `vibra-actors`: actor identity and lifecycle (spawned, runnable,
waiting, exited), bounded FIFO mailboxes, addresses with operation set and
bound scope, one-shot reply slots, exit notification to waiters, virtual-time
timeouts, the event record, the `@schedule.v1` document, and a single-thread
simulation scheduler that takes each decision from a recorded schedule or a
seeded generator. It is generic over:

```rust
trait Executor { type Actor; fn run_to_action(&mut self, actor: &mut Self::Actor) -> Action; /* send | receive | reply | host | exit | trap */ }
```

The parts no open question touches, which may start before X4-S1 is approved:
mailbox, lifecycle state machine, reply slot, event record, scheduler loop.
Address scope and the schedule document wait for it.

**Validation**, all with a scripted test executor, no language involved:

- `a_mailbox_at_its_bound_refuses_the_next_send_and_accepts_after_a_delivery`.
- `messages_from_one_sender_arrive_in_send_order`.
- `a_second_reply_to_one_slot_is_refused_and_the_first_stands`.
- `a_waiter_is_woken_with_exit_when_its_actor_exits`.
- `an_address_refuses_an_operation_or_scope_it_does_not_admit`, and
  `a_narrowed_address_admits_no_more_than_its_parent` as a property test over
  random narrowings.
- `one_actor_runs_one_handler_at_a_time`: the kernel never calls
  `run_to_action` on an actor that is mid-turn.
- `a_recorded_schedule_replays_to_the_same_event_record`, and
  `a_schedule_that_names_a_non_runnable_actor_is_an_invalid_replay`.
- `ten_thousand_seeded_schedules_keep_the_invariants`: no lost message, no
  message delivered twice, mailbox bound never exceeded, every waiter woken
  exactly once.
- The crate has no `unsafe`, no thread, and no clock: an architecture test.

**In the workspace:** the part no open question touches exists
(`crates/vibra-actors`, 15 tests). Address scope, narrowing, and the schedule
document are not started. The kernel made these choices where the plan was
silent. Each is provisional, and X4-S1 confirms or replaces it:

1. A reply slot travels as a field of the send, never inside the payload; the
   kernel never inspects a payload.
2. An actor holds a slot only once a message carrying it is delivered to it. A
   reply to a slot one does not hold, or a wait on a slot one does not own, is
   an invalid request (a defect of the executor), not an outcome.
3. A slot is attached to at most one accepted send; a refused send does not
   attach it.
4. A mailbox bound of 0 is refused at spawn. A self-send is allowed.
5. A send to a receiver parked in a receive is queued and delivered at once:
   send-accepted, deliver, wake, in that order.
6. A reply to a slot whose owner has exited is the outcome "owner exited" and
   the payload is dropped. A reply after the waiter timed out still fills the
   slot, and the owner may wait again.
7. A wait on a filled or closed slot returns at once with no park or wake. A
   successful wait consumes the slot; a timeout does not.
8. On exit or trap, the actor's queued messages are dropped with no event per
   message, and every unanswered slot it held or had queued is closed; an
   owner parked on one is woken with "exited".
9. Virtual time moves only on a scheduler decision. Waits due at or before the
   new time fire in (deadline, actor) order. A timeout of 0 fires at once.
10. A run ends completed (all exited) or blocked (none runnable, no deadline).
    A replay with decisions left over, or cut short, is invalid.
11. An actor cannot spawn an actor by a kernel action: spawn is not in the
    plan's action list. X4-S1 must say whether it is.
12. The executor is handed the result of its last action when it is resumed,
    and "create a reply slot" and "wait on a slot" are kernel actions beside
    the six the sketch above lists.

## X4-02 · Checker: scopes, endpoints, and effects

Split by the specification's sections: (1)
scopes and the parallel map, with the capture rule; (2) endpoint types and
narrowing; (3) the scope and messaging operations in performed rows; (4)
target admission of the host operations of every startable body; (5) query
metadata for the causal path.

**Validation:** per sub-item, positive, negative, recovery, and formatter
cases addressed by rule ID, as the [execution model](../execution.md)
requires, and in particular:

- a direct file write outside the caller's ceiling is rejected with the
  existing `@effect.outsideCeiling` and its witness;
- a target that can start a body that writes files, and whose admission
  ceiling omits `DirWriteText`, is rejected, with that body named in the
  witness;
- a sender whose row lacks the messaging operation is rejected;
- sending an operation the endpoint type does not admit is a type error, not
  a run-time refusal;
- a parallel-map callback that performs an operation is rejected, and one
  with an unsupported capture is rejected before any run.

## X4-03 · Interpreter executor

**Deliver** the interpreter's `Executor`: an actor is a `Machine` whose
activation stops at a kernel action and continues with the kernel's answer;
`run` and `test` drive the simulation scheduler.

**Validation:**

- Each kernel test of X4-01 has a Vibra conformance case with the same
  outcome through real source.
- `a_handler_that_traps_exits_its_actor_and_no_other`.
- `a_scope_exit_leaves_no_running_child`, for normal exit, early failure,
  and cancellation, nested two deep.
- `a_parallel_map_returns_results_in_input_order_within_its_bound`, and
  `…_equals_its_sequential_fallback` on the same input, the empty input
  included.
- `cancelling_after_submission_keeps_the_late_outcome_visible`.
- Every message and host event in the trace carries the origin of its send,
  its handler, and its host call, checked against spans in the source.
- A program with no actor has a byte-identical trace to the one it had
  before this package.

## X4-04 · Replay cases and causal tooling

G4 (interpreter half).

**Validation:** a `schedule` input on `workspaceRun` cases, valid only with
a host world; one case per failure of the gate list (an out-of-scope
request, a full mailbox, a duplicate reply, exit while a caller waits, a
timeout, cancellation before and after submission, a blocked run), each
replayed from a checked-in schedule; `vibra query` returns, for a send whose
handler writes a file, the sender's direct operation and the path send →
handler → `fs.writeText`, validated against its schema; a schedule edited
to an impossible order, cut short, or with decisions left over is reported
as an invalid replay, not as a program failure.

## X4-05 · Nexus executor

**Decisions:** N20, N23, N51.

**Deliver** in `vibra-nexus`: `Step::Wait` and `Outcome::Waiting`; a context
per actor or child; the transfers for the kernel actions; the copy of a
value out of a store into the kernel's neutral form and back; grants of the
actor-endpoint kind, checked at kernel dispatch; delegation of a grant
through the broker; the transfer of a parallel-map callback's code identity
and captures; `ABI_VERSION` bumped.

**Validation:**

- `a_message_copied_between_contexts_shares_no_block`: after delivery,
  releasing the sender's context leaves the receiver's value intact, and the
  two live sizes are independent.
- `a_value_nested_a_hundred_thousand_deep_crosses_without_stack`.
- `a_waiting_context_holds_only_its_frames_and_values`: parked size is
  recorded per actor for the baseline.
- `a_function_value_cannot_cross_in_a_message`: the copy refuses it with the
  outcome the specification names. `a_map_callback_crosses_with_its_captures`
  is the one admitted exception, and it keeps the callback's type and
  contract.
- `a_delegated_grant_shares_its_parents_accounting`, and
  `a_grant_copied_as_bytes_is_not_a_grant`.
- `an_endpoint_is_never_dispatched_to_a_host_adapter`.
- Every kernel test of X4-01 passes with the Nexus executor over hand-built
  execution IR on the stepper, as the foundation's demo does.

## X4-06 · Native lowering and code generation for concurrency

Follows the four-part extension rule
for each actor instruction.

**Validation:** every replay case of X4-04 is `matched` in the `stepper`,
`windows`, `linux`, and `macos` columns of the parity inventory, compared on
outcomes, message events, host events, and origins, from the **same**
checked-in schedule the interpreter replays.

## X4-07 · Workers and placement

**Decisions:** N22.

**Deliver** the production scheduler: a fixed pool, placement chosen before
an actor's initialization, mailboxes over `std` synchronization, and the
placement fact exposed in the run's structured output.

**Validation:**

- `a_context_is_touched_by_one_thread_at_a_time`: a debug-build owner check
  in the context, exercised by a stress test of 1,000 actors on 4 workers for
  1,000,000 messages with no failure.
- `addresses_do_not_change_with_placement`: one program run with two
  placements sends to equal addresses and ends with equal results.
- The stress test passes under ThreadSanitizer on Linux, or the record says
  it was not run and why.
- No `unsafe` outside the bridge: the architecture test still passes.
- `the_simulation_and_the_pool_agree_on_a_deterministic_program`: a program
  whose result does not depend on interleaving gives the same result under
  both schedulers.

## X4-08 · Concurrency fixture and release gate

**Kind:** evidence. **Decisions:** N52.

The document-service case (a late reply must not mark a newer revision
saved) is the gate of [v0.0.5](../v0.0.5/README.md) and is not claimed here.

**Validation**, one row per gate clause, from the fixture
`examples/parallel-validate/`: a command-line program that validates the
files of a directory with a bounded parallel map, and reports through one
long-lived logging actor.

| Clause | Evidence |
| --- | --- |
| Bounded fan-out | Replay: nested bounded fan-out, with results in input order and equal to the sequential fallback. An early failure cancels the rest, and no child outlives its scope |
| Failure paths | Replays, each with its typed outcome: an out-of-scope request refused at dispatch with no host event; a full mailbox; a duplicate reply; exit while a caller waits; a timeout; cancellation before and after submission, with the late outcome visible |
| Static rejection | Rejected programs: a direct write outside the caller's ceiling, a target whose ceiling omits a startable body's operation, and an effectful map callback |
| Replay parity | The same schedule file, the interpreter and three native targets, equal on outcomes, message and host events, and origins. An invalid, truncated, or over-long schedule fails replay explicitly |
| Suites | Both suites, counts recorded per target |

## X4-09 · Two-worker overlap fixture

**Kind:** evidence.

**Validation:** a native fixture with two CPU-bound actors on two workers
records, per actor, monotonic-clock intervals of progress; the test asserts
the intervals overlap for most of the run and that total wall time is below
1.6 times the single-actor time on a machine with at least two cores. The
record states the hardware. The assertion is about overlap, never about
event order.
