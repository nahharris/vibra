# 05 Execution

Status: draft specification; not normative

## The model

A Vibra program does not run. It **advances**. Its whole behavior is a
total function

```text
step : state × event → state × (list intent)
```

and a runtime that owns everything else.

| Owned by the program | Owned by the runtime |
| --- | --- |
| The state type and the step function | The event loop and the order of events |
| Which intents to request | Performing intents, retries, timeouts |
| What to do with each outcome | Time, randomness, input, output |
| | The journal |

- Program code MUST have no way to perform an effect. It can only return
  intents.
- The result of a step MUST depend only on the state and the event.
- The state of a program MUST equal the fold of `step` over its journal.

`proc` and `machine` are two ways to write a step function. Both compile
to the same core ([07](07-compiler.md#procedures-to-machines)).

## Ports

A **port** is a typed boundary to the world outside the program. It is
the only form of authority.

```text
port ledger
  op debit
    takes account: account-id, amount: cents
    returns debit-outcome
    delivery once
  end debit
  op balance
    takes account: account-id
    returns balance-outcome
    delivery repeatable
  end balance
  signal statement-ready
    carries statement
  end statement-ready
  model ledger-model
end ledger
```

- An **operation** is requested by the program and answered by one
  outcome. Its `returns` type MUST include every failure the program is
  expected to handle. There is no other error channel.
- A **signal** arrives unrequested: a request from a client, a message, a
  tick.
- A port MUST name a **model**, written in Vibra, that plays the port in
  simulation and exploration, including its failures. A port without a
  model cannot be used.
- A procedure or machine may address only the ports in its `uses`
  clause. A `fn` has no `uses` clause and cannot address any.
- Outside the language, a **driver** implements each port for a target
  ([06](06-runtime.md#drivers)).

The standard library defines ports for the console, files, environment,
network, clock timers, and randomness. They have no special status.

### Models

A model is a cell with a state and one handler per operation.

- In a handler, `state` is the model's state and `request` is a record of
  the operation's arguments.
- A handler is pure. It returns a non-empty list of **answers**, each
  written `(sim.answer next: s outcome: o)`: every outcome the real port
  is permitted to give for that request and history, with the model
  state that follows it.
- Simulation chooses one answer by seed. Exploration tries each. A driver
  conforms when every outcome it gives is among the answers
  ([06](06-runtime.md#drivers)).

The [sample](09-sample.md#a-port-and-its-model) has a complete model.
Open: how a model produces signals.

### Delivery

After a crash the runtime may not know whether a requested operation
happened. Each operation states what is then safe.

| Delivery | The runtime | The program receives |
| --- | --- | --- |
| `repeatable` | May perform the operation again with the same key | The declared outcome type |
| `once` | Never repeats the operation | `(uncertain t)`: `known` with the outcome, or `unknown` |

Every intent carries an **idempotency key** derived from its position in
the journal. A driver for a `repeatable` operation MUST treat a repeated
key as the same request.

## Intents

An intent is a value of type `(intent t)`, where `t` is its outcome type.

- Applying a port operation builds an intent: `(ledger.balance account)`
  has type `(intent balance-outcome)`. Building an intent does nothing.
- Applying a procedure builds an intent to run it.
- Intents compose as data:

| Combinator | Outcome |
| --- | --- |
| `(intent.both a b)` | `(pair x y)`, when both have answered |
| `(intent.all items)` | A list of outcomes, in the order of `items`. The parts run concurrently |
| `(intent.in-order items)` | A list of outcomes. Each part starts when the one before has answered |
| `(intent.first a b)` | The outcome that arrives first. The other is cancelled |
| `(intent.within duration a)` | `(option x)`: `none` if the duration passes first |

A pure function may build and return intents. Only a procedure can
`await` one, and only a machine can `emit` one.

## Procedures

A procedure is effectful code in direct style.

```text
proc move-money
  takes request: transfer-request
  returns transfer-result
  uses ledger
body
  let debit: (uncertain debit-outcome) = await (ledger.debit request.source request.amount)
  match debit
    case unknown
      return (transfer-result.needs-review)
    case known
      let outcome: debit-outcome = debit.value
      match outcome
        case refused
          return (transfer-result.refused outcome.reason)
        case done
          return await (credit-or-refund request)
end move-money
```

- `await e` requests intent `e`, ends the current step, and resumes in a
  later step with the outcome.
- The code between two `await` points is one step. It is pure, total, and
  atomic.
- A procedure's `uses` MUST include the `uses` of every procedure it
  awaits.
- The `unknown` case above is not optional. A debit that may or may not
  have happened is a case the type forces the author to write.
- A procedure terminates if every intent it awaits is answered.

## Machines

A machine is a long-lived step function with explicit state.

```text
machine transfer
  takes request: transfer-request
  state transfer-state
  starts (transfer-state.idle)
  accepts transfer-command
  uses ledger
  invariant (bool.implies (transfer-state.is-idle state) (int.eq (pending ledger.debit) 0))
  on idle, begin
    emit (ledger.debit request.source request.amount)
    return (transfer-state.debiting)
  on idle, ledger.debit
    impossible
  on debiting, begin
    return state
  on debiting, ledger.debit
    let outcome: (uncertain debit-outcome) = event.outcome
    match outcome
      case unknown
        return (transfer-state.review)
      case known
        return (next-after-debit outcome.value)
  // The remaining pairs are elided in this example.
end transfer
```

- The state type MUST be a variant.
- An **event source** is a case of the `accepts` type, an operation or
  procedure whose intent the machine emits, or a signal of a used port.
- There MUST be exactly one handler for every pair of state case and
  event source. A pair the author believes cannot occur is written
  `impossible`, which is an obligation and not an assertion at run time.
- In a handler, `state` is the current state narrowed to the handler's
  case, and `event` is the event. For an outcome, `event` has the fields
  `request` and `outcome`. Machine parameters are visible.
- `emit e` requests intent `e`. Its outcome returns later as an event.
- A handler is one step. It returns the next state. `return state` keeps
  the current one.
- `(pending p.op)` is the number of requested and unanswered intents of
  that operation. It is available only in claims.

### Addresses

- `(machine.start m args)` is an intent whose outcome is an
  `(address t)`, where `t` is the `accepts` type of `m`.
- `(address.send a message)` is an intent that delivers `message` as an
  event. Delivery is recorded once in the journal and is `repeatable`.
- A machine that starts others receives the source `machine.exited` when
  one stops or faults, and so MUST handle it.
- An address is a value. It can be sent, stored, and compared. It confers
  the ability to send and nothing else.

## The journal

The journal is the complete, ordered record of an instance.

```text
41 event   t-9 (begin)
41 intent  t-9 (ledger.debit alice (cents.of 500)) key: t-9/41/0
42 event   t-9 (ledger.debit outcome: (known (done)))
42 intent  t-9 (ledger.credit bob (cents.of 500)) key: t-9/42/0
43 event   t-9 (ledger.credit outcome: (known (closed)))
43 intent  t-9 (ledger.refund alice (cents.of 500)) key: t-9/43/0
```

- An event is appended before its step runs. The step's intents are
  appended with the step's commit.
- A step either commits completely or leaves no trace.
- Nothing reaches the program except through an event, and nothing leaves
  it except through an intent.

What follows from this, and MUST be provided by a conforming runtime:

| Property | Mechanism |
| --- | --- |
| Replay | Fold `step` over a journal and reach the same state and intents |
| Durability | After a crash, replay the journal and continue |
| Counterfactuals | Replay a prefix, substitute an event, and continue under models |
| Simulation | Run under port models with seeded event order and faults |
| Exploration | Enumerate reachable states of `step` under models |
| Upgrade evidence | Replay recorded journals against a new witness and compare intents |

## Logical time

- Every event carries a **tag**: a logical instant and a sequence number.
  Tags are totally ordered.
- In a handler or procedure body, `now` names the instant of the current
  step. It is a value. Within one step time does not pass, and a pure
  function that needs the time receives it as an argument.
- Physical time enters only as the instant the runtime assigns to an
  inbound signal. The assignment is journaled.
- `(timer.after duration)` is an intent answered by an event whose
  instant is the requesting instant plus `duration`.
- A machine processes events in tag order, one at a time. Events for
  different machines with equal instants are ordered by address.
- A runtime MAY run steps of different machines in parallel when the
  result equals the tag-ordered sequential result.

Randomness is a port. Each draw is an outcome in the journal.

## Failure

- A step cannot fail. Pure code has no run-time errors
  ([03](03-types.md#numbers)).
- A failed operation is an outcome value of its declared type.
- Exhausting memory or a step's budget is a **fault** of the instance,
  not a result of the program. The runtime records it, and the instance
  is restarted from its journal or stopped, by target policy.

## Upgrades

- Replacing a witness of a running machine takes effect at the next step.
- Changing the state type requires a `migrate` function from the old
  state type to the new, with the new invariants as its `ensures`.
- A target policy MAY require replay evidence before an upgrade: recorded
  journals are replayed against the new cell and every difference in
  intents is reported.

## Protocols

Layer 4. A **protocol** states an exchange among several machines once,
from a global point of view, and the machines' handlers for it are
derived.

```text
protocol two-phase
  roles coordinator, participant
  coordinator -> participant: prepare
  participant -> coordinator: vote
  if (vote.is-yes vote)
    coordinator -> participant: commit
  else
    coordinator -> participant: abort
end two-phase
```

The derived handlers cannot deadlock on the exchange or disagree about
its messages, because both sides come from one text. The exact form is a
sketch.

## Open

- **Liveness.** `invariant` states safety. A bounded form of progress,
  such as "from every reachable state, a `done` state is reachable within
  `n` steps under fair delivery", is wanted and unspecified.
- **Monotone state.** A machine whose state only grows under a merge
  needs no coordination across replicas. Marking and checking such state
  belongs to Layer 4.
- **Shared journals.** One journal per runtime instance is specified.
  Replicated journals are not.
