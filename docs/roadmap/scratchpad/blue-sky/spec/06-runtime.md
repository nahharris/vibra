# 06 Runtime

Status: draft specification; not normative

The runtime is the part of Vibra that acts. It delivers events, runs
steps, performs intents through drivers, and keeps the journal. The
**reference runtime** pairs the reference evaluator with a file journal
and is part of the trusted base. Every other runtime is judged against it.

## Instances

An **instance** is one running machine or procedure: an address, a cell
id, a state, and a journal. A **system** is the set of instances started
from a target's root, sharing one tag order.

## Journal format

A journal is a sequence of `.vibon` records.

| Record | Fields |
| --- | --- |
| `start` | Address, cell id, arguments, tag |
| `event` | Address, source, payload, tag |
| `intent` | Address, step, index, operation, arguments, key |
| `commit` | Address, step, hash of the new state |
| `fault` | Address, step, kind, detail |
| `upgrade` | Address, old cell id, new cell id |
| `stop` | Address, tag |
| `snapshot` | Address, step, state |

- Each record MUST carry the hash of the record before it. A journal is
  therefore tamper-evident, and a prefix is identified by one hash.
- A step's `intent` records and its `commit` MUST become durable
  together.
- A driver MUST NOT be invoked for an intent that is not yet durable.
- A `snapshot` allows replay to start at its step. A runtime MAY discard
  records before a snapshot when target policy permits. Discarding MUST
  NOT change later behavior.
- A value that carries a label ([03](03-types.md#labels)) MUST be stored
  under the protection the target assigns to that label, and MUST be
  omitted from any journal export that does not hold it.

## Drivers

A driver implements one port for one kind of target. It is host code and
is outside the language.

- A driver receives an operation, its arguments, and its key, and returns
  one value of the operation's outcome type.
- A driver for a `repeatable` operation MUST return the same outcome for
  a repeated key, or one the model allows as a repeat.
- A driver produces signals by handing typed values to the runtime, which
  tags and journals them.
- A driver MUST NOT call into program code and MUST NOT share state with
  it.

**Conformance to the model.** Every outcome a driver returns MUST be one
its port's model could have produced for the same history. A runtime
SHOULD check this while running, and MUST be able to check it over a
recorded journal. A violation is recorded as a fault of kind `model`. It
means the simulation evidence for every user of that port no longer
describes reality.

## Scheduling

- A step runs to completion. Steps are never preempted or interleaved
  within one instance.
- Each step has a **budget** in steps of the cost model, derived from the
  handler's cost bound ([04](04-claims.md#cost)). Exceeding it is a fault
  of kind `budget`, which indicates a wrong cost claim.
- Events are delivered in tag order. An instance with a deliverable event
  MUST eventually be stepped.
- Steps of different instances MAY run in parallel
  ([05](05-execution.md#logical-time)).

Because steps are total and bounded, the scheduler needs no preemption,
no time slicing, and no cancellation of running code.

## Memory

- Memory management MUST be unobservable except through cost.
- An update of an owned collection MUST be performed in place
  ([03](03-types.md#ownership)).
- Reclamation MUST NOT pause an instance for a time that grows with the
  size of the heap. Reference counting with reuse, or a region per step,
  both satisfy this. The choice is the implementation's.
- Running out of memory is a fault of kind `memory`.

## Replay

`replay` folds `step` over a journal.

| Mode | Does | Fails when |
| --- | --- | --- |
| Verify | Recomputes each step and compares intents and state hashes with the record | Anything differs. This is a determinism bug in a backend or driver |
| Resume | Rebuilds state from the last snapshot and continues | The journal is damaged |
| Against | Runs a different cell over the recorded events and reports differing intents | Never. The differences are the result |
| What-if | Replays a prefix, substitutes or inserts events, and continues under port models | Never |

Verify replay of a sample of production journals SHOULD run continuously.

## Simulation

Simulation runs a system with every port played by its model and every
choice made by a seeded generator.

- The generator chooses the order of deliverable events, the timing of
  outcomes, which permitted failure a model returns, where instances
  crash and restart, and when `once` operations answer `unknown`.
- A run is a function of its seed. The same seed MUST give the same
  journal on every platform.
- A run fails on a violated `invariant`, a reached `impossible`, a fault,
  a violated `ensures` of a procedure, or a failed replay of its own
  journal.
- A failing run is reported as its seed and its journal, reduced to a
  shorter failing journal where possible.

No harness code is written to make a program simulable. Any program whose
ports have models is a simulation target.

## Exploration

Exploration enumerates the reachable states of a system under its port
models, up to a stated depth in steps.

- States are compared by hash. A state already seen is not expanded.
- The checked properties are those of simulation.
- A violation is reported as a journal that reaches it.
- Completed exploration to depth `d` is evidence at grade `bounded` with
  bound `d`. If the reachable state space was exhausted before `d`, the
  record says so.

The explored object is the program's own step function. There is no
separate model that could disagree with the code.

## Questions of a journal

A runtime MUST answer these without stepping interactively.

| Question | Answer |
| --- | --- |
| State at step `n` | A value |
| First step where a predicate over state holds, or fails | A step number |
| Steps at which a field of the state changed | Step numbers with old and new values |
| The event and handler behind an intent | A journal position and a cell |
| Intents requested and not yet answered at step `n` | A list |
| Difference between two journals | The first diverging record and both versions |

## Targets and policy

A **target** says what to run, how ports are bound, and what evidence is
required. It is a cell written by an operator.

```text
target production
  runs transfer-service
  bind ledger to "postgres-ledger"
  bind timer to "system-clock"
  journal to "replicated-log"
  require ensures at solved
  require invariant at bounded
  require cost at tested
  require signed
  forbid assume, extern, open, weak
  fault restart
end production
```

```text
target  = "target" name NL INDENT "runs" qname NL bind+ journal
          policy* DEDENT "end" name
bind    = "bind" qname "to" text NL
journal = "journal" "to" text NL
policy  = "require" property "at" grade NL | "require" "signed" NL
        | "forbid" debt-kind ("," debt-kind)* NL
        | "fault" ("restart" | "stop") NL
```

- Building a target MUST fail if any cell reachable from `runs` has an
  effective grade below a `require`, carries a forbidden debt, or uses a
  port that is not bound.
- `require signed` demands that every exported claim reachable from
  `runs` is signed ([01](01-programs.md#roles)).
- The build MUST emit a manifest: every reachable cell id, its effective
  grades, its debts, the bindings, and the hash of the result. The
  manifest is what a person reviews before shipping.
- A development target with no `require` lines is valid. The point of
  policy is that weakness is chosen per target and written down.

## Execution forms

| Form | Purpose | Requirement |
| --- | --- | --- |
| Reference runtime | Meaning, evaluation, simulation, exploration | Trusted. Favors clarity over speed |
| Compiled runtime | Production | MUST pass verify replay of journals produced by the reference runtime, and the reverse |

A compiled runtime targets WebAssembly first. A native backend MAY exist
under the same requirement ([07](07-compiler.md#backends)).
