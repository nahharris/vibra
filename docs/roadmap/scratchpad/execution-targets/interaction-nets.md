# Interaction nets

Status: exploratory reasoning; not normative, not a proposal, and not
scheduled

This note asks two questions. Could [Nexus](nexus.md) use interaction nets
in place of, or underneath, the [local actor model](../../v0.0.4/README.md)?
And could nets reshape Vibra itself? The current read:

- Nets and actors answer different questions, so nets do not replace
  actors.
- As the Nexus evaluator, a net reducer conflicts with several runtime
  contracts, and the implementations measured so far are slow on one
  thread.
- The part of a net-shaped language that serves Vibra's thesis is its
  use-once discipline for values, and that part does not need nets.

The read can change. [What would change it](#what-would-change-this-read)
lists the evidence.

## What a net is

A net is a graph of cells. Each cell has one principal port, and a rewrite
happens only where two cells meet at their principal ports. A cell can be
in at most one such pair, so rewrites never overlap. Any order of rewrites,
including a parallel one, reaches the same result in the same number of
steps.
[Lafont 1990](https://doi.org/10.1145/96709.96718) ·
[HVM2 paper](https://github.com/HigherOrderCO/HVM2/blob/main/paper/HVM2.pdf)

Two consequences matter here.

- **Values are used once.** A wire joins exactly two ports. Copying and
  discarding a value are explicit cells with their own rewrites.
- **A net cannot express a race.** Lafont's nets "have been explicitly
  designed to be strongly deterministic, so no truly concurrent behavior
  can be expressed within them."
  [Mazza 2005](https://www-lipn.univ-paris13.fr/~mazza/papers/mINSAndConcurrency-CONCUR05.pdf)

## Nets and the actor model

The actor model decides who owns state, who may send to whom, and in what
order messages arrive. A mailbox with two senders is a merge whose result
depends on arrival order, which is what a net rules out. The extensions
that can encode process calculi add cells with several principal ports or
several rules per pair, and they give up the single result.
[Alexiev 1999](https://doi.org/10.7939/r3mw28r67)

A net-based Vibra would therefore still need a second mechanism at every
mailbox. The subject of v0.0.4 would also remain to be designed: scoped
send addresses, ownership and transfer, actor exit, and timeouts.

The two models do line up in one useful way. A message meeting a handler
is a single local rewrite: message and state in, next state and outgoing
messages out. v0.0.4 already specifies one handler at a time per actor and
immutable state between turns. In that picture the only nondeterminism is
where independent chains meet: arrival order at a mailbox, a send against
a full mailbox, a timeout against a reply, an exit against a waiting
caller, and the order in which actors reach the host. A replay record that
captures those points may be sufficient, with park and wake events derived
from it. The v0.0.4 simulator can test that.

## Three places a net could go in Nexus

| Placement | Read |
| --- | --- |
| Instead of actors | Does not apply. A net has no merge. |
| As the evaluator for all code, underneath actors | Conflicts with the contracts in the next section. |
| As one evaluator for checked pure regions | The only candidate. It competes with the ordered CPU fork/join track in [nexus.md](nexus.md#follow-on-research), not with actors. |

## Fit against the current contracts

The middle column mostly describes HVM2, whose paper states its design and
limits. A Vibra reducer could differ in each row.

| Contract | What a net reducer does | Read |
| --- | --- | --- |
| Strict order, and only the selected branch runs ([Evaluation](../../../spec/06-runtime.md#evaluation)) | Reduces every available pair eagerly. A recursive call in an unselected branch unrolls forever unless each branch is split into its own lazily expanded definition. | Workable with a lowering rule. |
| The interpreter's trap code and origin ([Traps](../../../spec/06-runtime.md#traps)) | Two operands reduced in parallel can both trap. The reducer needs an ordering rule on top, the same one [#408](https://github.com/nahharris/vibra/issues/408) states for fork/join. Cells have no activation, so each needs its own origin. | Extra machinery. The HVM2 paper describes none. |
| Ordered, exactly-once host events ([audit trace](../../../spec/06-runtime.md#the-audit-trace)) | Nets are pure. [Vine](https://vine.dev/docs/features/io) orders IO by passing one handle through every operation. | Works. Host operations stay in one sequence. |
| No host stack per activation, tail calls in constant space ([Tail calls](../../../spec/06-runtime.md#tail-calls)) | No stack at all. Lazily expanded definitions give constant-space tail recursion. | Fits. |
| Bounded live memory for an allocating tail loop ([Reclamation](../../../spec/06-runtime.md#reclamation)) | Frees a cell when it is consumed, with no collector. Eager reduction builds a whole structure even when one branch is read. | Fits in principle. Needs the conformance measurement. |
| 8 to 64-bit integers, `f32`, `f64`, `str`, `bytes`, flat arrays, ordered dicts | 24-bit numbers and no arrays. Each type would need a native cell kind. | Design work per type. |
| Unrestricted function values | Unsound when a function that copies its parameter is itself copied. Bend 1 documents a wrong result for mapping `lambda x: (+ x x)` over a list. An `iter.map` callback has the same shape. | Blocks direct use. The candidate fixes are bookkeeping cells (about 10x slower by the paper's estimate), usage typing, or lowering closures to first-order data. |
| Interpreter is the oracle, including any [reduction budget](../foundations/README.md#execution-budgets-and-reductions) | The total rewrite count does not depend on the schedule. It is a different unit from function applications, and a budget that stops a parallel run leaves a schedule-dependent state. | The invariant count is attractive. It does not match the sketched budget, which counts function applications in the interpreter. |
| Logical frames for traces and [live debugging](../agent-tooling/debugger.md) | No frames. A trace is a sequence of rewrites. | Needs a new source-level view. |
| Single-thread cost | About 5x slower than GHC, and up to 100x on loops and arrays, by the paper's own account. | Vibra walks arrays through `iter` and tail recursion, so the worst case is the common case. |

## What a net-shaped Vibra would be

Adopting nets as the language model, and not only as a runtime, would
change each surface the charter names.

- **Types.** Every binding gets a use count. Today values are immutable
  and freely shared, and the charter excludes a general ownership system.
- **Function values.** A closure is callable once, or restricted in what
  it captures. Callbacks applied per element need a copyable kind.
- **Effects.** Rows stay static, but ordering comes from a handle value.
  An effect root could become a single-use capability, which overlaps the
  v0.0.4 send address.
- **Evaluation.** The order of pure evaluation becomes unobservable, so
  the strict left-to-right rule is replaced by a trap-precedence rule.
- **Tooling.** Queries and edits carry over. Traces and debugging lose the
  call stack.
- **Runtime.** A graph reducer with a per-rewrite cost and parallelism by
  default.

Against the charter's [decision order](../../../spec/00-charter.md#decision-order):

- **Locally diagnosable errors: a real gain.** A use count turns a
  duplicate reply, a use after transfer, and a use after close into check
  errors that name both uses. The v0.0.4 gate exercises a duplicate reply,
  and the [foundations](../foundations/README.md#resources-and-handles) and
  [type-system](../type-system/README.md#stronger-guarantees-for-agents)
  notes already list these checks as study items.
- **Determinism: a gain for pure code only.** Parallel pure evaluation
  needs no recorded schedule. The charter's programs are small services,
  command-line tools, and local automation. They mostly wait on host
  operations and messages, which a net does not parallelize.
- **Predictable cost: a loss.** The [concurrency note](../concurrency/README.md#data-parallelism)
  prefers explicit parallel points for their cost model. Implicit
  parallelism hides where the work goes.
- **Small enough to finish: a loss.** The shipped checker, interpreter,
  standard library, and corpus assume freely shared values.

The gain and the mechanism are separable. Bend followed this path: its
first version compiled to a net evaluator, and its second is affinely
typed and has no graph. Its runtime paper says "affinity gives unique
ownership directly, so the graph and its duplication machinery become
unnecessary. Lost is optimal reduction of shared redexes; gained are
native-speed sequential code, flat memory and a cost model a programmer
can read."
[BendRT paper](https://github.com/bendlang/bend/blob/main/paper/BendRT.pdf)

That runtime has frames off the native stack, tail calls as jumps, and
explicit fork/join. That is close to the boundary
[v0.0.3 M1](../../v0.0.3/m1/README.md) already plans. The wider question
therefore reduces to a narrower one: should Vibra adopt use-counted
values, and for which of them? Handles and replies only is the existing
study item. Every value is a charter change.

Nets keep an advantage Vibra is not looking for: sharing work inside
copied higher-order functions. It matters most for programs built from
such functions.

## Ideas worth keeping without nets

- **Use-counted handles and replies.** The checks above, scoped to the
  values where a second use is a logic error.
- **A minimal replay record.** Record where independent chains meet and
  derive the rest, as sketched under the actor comparison.
- **A trap-precedence rule for pure operands.** Stating which trap wins
  lets any later backend evaluate pure operands in parallel.
- **A schedule-independent work count.** Any budget for parallel pure
  work should count a total that does not depend on the schedule.

## External evidence

| Source | What it shows |
| --- | --- |
| [HVM2 paper](https://github.com/HigherOrderCO/HVM2/blob/main/paper/HVM2.pdf) | 400 million rewrites per second on one thread, 5,200 million on 16 threads, and 74,000 million on an RTX 4090. The limits used in the table above come from its own limitations section. |
| [Bend 1 docs](https://github.com/HigherOrderCO/Bend1/blob/main/docs/dups-and-sups.md) | A correct program needs that "a variable should not duplicate another variable that itself duplicates some variables." The runtime does not always catch a violation. |
| [Bend 1 launch thread](https://news.ycombinator.com/item?id=40390287) | A user's recursive sum took 4.5 s in PyPy and 1 min 42 s in CPython. Single-thread Bend had not finished after 42 minutes. The author called that a probable bug and noted two cells allocated per arithmetic operation. |
| [Breaking Bend](https://blog.speedfox.co.uk/) (2024-09-03) | An 11x11 determinant took over 2 hours on one thread and 20 minutes on four cores in Bend, against 22 s on one thread in Go. |
| [BendRT paper](https://github.com/bendlang/bend/blob/main/paper/BendRT.pdf) | Bend 2, released in September 2026, runs without nets. It reports sequential code at 0.8 to 1.5 times the time of a C version written one function per Bend definition, and 9 to 12 times speedup on 16 threads when forks are balanced. |
| [Vine](https://vine.dev/docs/ivy-ivm/ivm) | An active, experimental net language. Its runtime has a parallel mode, native value cells, and IO ordered through one handle. |

Almost every figure comes from the project that produced it. The BendRT
paper discloses that an AI model wrote it from the author's notes and that
the author reviewed it. The two independent measurements cover Bend 1
only, and none uses a Vibra-shaped workload.

## What would change this read

For a net reducer over pure regions:

1. A named pure subset reduces soundly. Every executable corpus case in
   the subset matches the interpreter, including a callback that copies
   its parameter.
2. It is faster than the native single-thread baseline recorded at the
   [v0.0.3 M2](../../v0.0.3/m2/README.md) exit, on array walks and text
   building, and faster than explicit fork/join on the same workload.
   Speedup over its own single thread does not count.
3. Trap code and origin match the interpreter when both operands trap,
   and an allocating tail loop keeps the bounded live size.

For the wider question, a v0.0.4 protocol where a duplicate reply or a
use after transfer keeps reaching tests is evidence for use-counted
handles. The type-system note already sketches that experiment.

## Keeping the option open

These are already in the plans and cost nothing to keep.

- Checked IR carries a closed effect row per function, so pure regions
  stay identifiable.
- Handle representation stays private to the execution context.
- Actor addresses stay independent of worker placement.
