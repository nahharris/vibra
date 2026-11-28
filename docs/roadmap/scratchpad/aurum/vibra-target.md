# An Aurum target for Vibra

The goal is one language on two kinds of machine. A Vibra program keeps
running on the interpreter and on native code, and the same checked program,
or the part of it that fits, also lowers to the Aurum fabric. Where a
feature cannot be realized on the fabric, the compiler says so before
anything runs.

This note argues that the specification already has the mechanism for this,
that Aurum needs two nested fragments of Vibra rather than one, and that the
first useful Aurum target is not a whole program.

## Theory

Three ideas carry the design. They are named here so that the engineering
below can be checked against them.

- **A target is a restriction, not a dialect.** An Aurum fragment is a
  decidable predicate on checked programs. A program inside the fragment has
  exactly the meaning the interpreter gives it; a program outside it has no
  Aurum meaning at all. This is a conservative restriction of one semantics,
  and compiler correctness is the usual forward simulation between the
  interpreter and the fabric over the fragment [CompCert09].
- **What a function needs from its machine is a coeffect.** An effect row
  says what a computation does to its context. Whether a function needs an
  allocator, an unbounded stack, or a floating-point unit is the dual
  question: what the computation requires of its context
  [Coeffects14]. Like Vibra's effect rows it composes over the call graph,
  so it can be computed for every function and reported as a fact. Unlike
  effect rows it needs no syntax, because nothing in the source depends on
  it.
- **Order is unobservable where the calculus is confluent.** Interaction
  nets are strongly confluent [Lafont90] and Kahn networks are determinate
  [Kahn74], so a fabric built on either may fire in any order and still
  compute one result. Vibra observes more than results, though, and the
  section on [observations](#what-an-aurum-run-may-change) lists exactly
  what else has to be held fixed.

## What the specification already provides

No new mechanism is needed to make a target reject a program.

- The runtime chapter states the rule: an executor that does not support an
  admitted operation rejects the program before running any of it, and "the
  result is unavailable or unsupported, with the profile and the reason"
  ([Runtime](../../../spec/06-runtime.md)).
- A [conformance profile](../../../spec/07-diagnostics-and-conformance.md#conformance-profiles)
  "is not a dialect", and "the same source never means something else under
  a smaller profile". `nativeV1` already works this way: it has no cases of
  its own and runs the executable cases of `interpreterV2` on other
  machinery.
- `@tool.unavailable` already reports "a valid command or construct outside
  the tool's profile; a driver the platform does not support, at admission",
  with the whole form as its span, and `@tool.unsupportedTarget` reports a
  build for a platform the toolchain does not emit.
- The interpreter is permanent and is the reference. An Aurum executor is a
  third executor held to the same agreement rule as native code: results,
  canonical observations, the order of completed host events, typed
  failures, and source origins.

So an Aurum target is a new profile and a new platform, and its rejections
are `@tool.unavailable` with a reason. Nothing in the source language
changes, and the language stays as familiar on Aurum as it is anywhere.

## Three ways to use the fabric

Every runnable Vibra program has a `proc` entry that receives grants and
returns an exit code. A fabric that could only run whole programs would need
the complete effect and authority machinery before it could run anything.
The specification offers two smaller units, and they give a natural order of
work.

| Mode | Unit placed on the fabric | What it needs | Precedent in the specification |
| --- | --- | --- | --- |
| Kernel | One checked pure module function and its callees | A lowering for pure code only | `vibra eval`, which "calls one checked pure module function" with checked arguments |
| Co-processor | Some pure functions of a program that otherwise runs natively | Kernel mode plus value transfer between the native runtime and the fabric | The GPU placement question in the [Nexus follow-on tracks](../execution-targets/nexus.md): one checked pure region with a CPU fallback and a device kernel |
| Whole program | The entry procedure and everything it reaches | Co-processor mode plus effects, grants, and startup on the fabric side | `vibra run` |

The co-processor mode answers the worry about feature coverage directly.
Because `fn` is pure and placement is unobservable, a function the fabric
cannot take simply stays on the processor, and the program still runs. An
unavailable feature costs performance, never correctness, and never forces a
rewrite. Rejection as an error matters only in kernel mode, where the user
named one function, and in whole-program mode.

**Recommendation.** Build kernel mode first, co-processor mode second, and
treat whole-program mode as a research milestone of its own.

## Two fragments

The literature on compiling functional programs to hardware splits cleanly
by one question: can the program's storage be fixed before it runs?

**The static fragment** is the part of Vibra whose state is finite and known
at build time. It lowers to a fixed circuit or a fixed dataflow graph with
bounded channels, and it needs no allocator on the fabric.

- SAFL made the same cut: first-order, call by value, and "only tail
  recursion is permitted" so that storage can be allocated statically
  [SAFL00].
- Geometry of Synthesis starts from an affine type discipline because it
  gives the game semantics a finite representation, and it handles recursion
  by unfolding it in space or in time [GoS07] [GoS11].
- Clash refuses general recursion and every recursive type except its own
  fixed-length vector and tree, so the list type is unsupported, and floats
  are impractical [Clash].
- Wavelet, the verified dataflow compiler, accepts only tail recursion,
  forbids mutual recursion, and allows one static external call site per
  function, because its target has a static topology and bounded channels
  [Wavelet26].
- Dahlia types hardware resources affinely, so that one memory port or one
  operator is not used twice in the same time step [Dahlia20]. Its
  successors put structure and timing into the IR and the types [Calyx21]
  [Filament23].
- Shader languages make the same refusal for the same reason [WGSL].

**The net fragment** is all of pure Vibra. It lowers to a graph that grows
and shrinks while it runs, so the fabric must allocate, link, and reclaim
nodes. This is the prospectus's rewrite fabric. Graph-reduction machines
are the hardware precedent [Reduceron10] [Heron23]; interaction nets are
the calculus [Lafont90].

The static fragment is a subset of the net fragment, and a program may mix
them: a net-fragment program can contain static-fragment regions that the
compiler lowers to fixed datapaths, which is how a rewrite fabric would get
fast arithmetic kernels.

Effects are a separate axis, treated [below](#effects-and-authority-on-the-fabric).

## Feature availability

"Yes" means the fragment can realize the feature with the interpreter's
meaning. "After specialization" means the feature is available when the
compiler can remove it before lowering and unavailable otherwise. The reason
codes are illustrative; they would be a closed set of camelCase atoms
carried by `@tool.unavailable`.

| Vibra feature | Static fragment | Net fragment | Reason when unavailable |
| --- | --- | --- | --- |
| `Bool`, `Void`, fixed-width integers, `Char`, `Atom` | Yes, as fixed-width wires | Yes | — |
| Saturating and `Result`-returning arithmetic | Yes; saturation is a comparator and a multiplexer | Yes | — |
| `F32`, `F64` | No in a first profile. The canonical-NaN rule requires bit-exact IEEE units | No in a first profile | `@aurum.floatingPoint` |
| Tuples, records, wrappers, variants without recursion | Yes, bit-packed | Yes | — |
| Recursive declared types | No: unbounded | Yes, as graph nodes | `@aurum.unboundedData` |
| `List`, `Str`, `Bytes`, `Dict` | No: unbounded | Yes, with a different cost model (see [representation](#collections)) | `@aurum.unboundedData` |
| `match`, `if`, exhaustiveness | Yes | Yes | — |
| Bare `requires`, `ensures`, `where` | Yes: proved at compile time, nothing is lowered | Yes | — |
| Guards with an `else` consequence | Yes: an ordinary branch | Yes | — |
| Generics | After specialization, which the specification already makes unobservable | Yes; descriptors are specialized away where possible | `@aurum.unspecializedGeneric` |
| Function values and closures | After specialization to direct calls | Yes | `@aurum.functionValue` |
| Interface values | After devirtualization. `Iter` adapters store their source as an interface value, so iterator pipelines depend on this | Yes, as tagged nodes | `@aurum.interfaceValue` |
| Self tail calls | Yes: a loop | Yes | — |
| Mutual tail calls | Yes in principle, as one state machine; the verified precedent excludes them [Wavelet26] | Yes | `@aurum.mutualRecursion` |
| Calls that are not tail calls, without recursion | Yes: inlined or instantiated | Yes | — |
| Recursion that is not in tail position | No: it needs a stack, which is allocation | Yes | `@aurum.nonTailRecursion` |
| Module values | Yes: constants evaluated at compile time | Yes | — |
| `proc`, `Intent`, `await`, grants, host operations | Whole-program mode only | Whole-program mode only | `@aurum.effectfulCode` |
| A host operation whose driver the platform lacks | — | — | already `@tool.unavailable` at admission |
| `test` forms and fake worlds | Run on the interpreter; kernels under test also run on the emulator | Same | — |
| Actors (v0.0.4) | Out of scope until they ship | Out of scope until they ship | — |

Four properties of Vibra make it an unusually good source language for a
fabric, and none of them was designed for one.

- **No traps on user data.** Hardware has no cheap precise exception. A
  language whose arithmetic saturates or returns `Result` and whose lookups
  return `Option` needs no exception network and no rollback.
- **Contracts leave no run-time check.** Every bare guard is discharged
  before lowering, so contracts cost no gates.
- **Purity is in the type of the callable.** `fn` against `proc` is exactly
  the line between what the fabric may reorder and what it may not.
- **Memory exhaustion is a host event.** A fabric whose node arena fills up
  raises `@runtime.memoryExhausted`, which the language already defines.

## What an Aurum run may change

The runtime chapter fixes evaluation order: a callee, then every operand in
written order. A fabric evaluates independent operands at once. The question
is what a program can observe of the difference.

A pure Vibra expression can make its evaluation order visible in only four
ways.

1. **Host events.** A `fn` has none. Applying a `proc` only builds a cold
   plan; nothing happens until `await`.
2. **Traps.** No operation traps on user data. The two remaining traps are
   toolchain defects.
3. **Divergence.** Evaluation is strict, so a call whose operand diverges
   diverges in written order and in parallel order alike, provided the
   fabric waits for the operand as the interpreter does. The two orders
   then differ only if the fabric evaluates something the interpreter
   would never have evaluated.
4. **Resource limits.** Peak memory and the work counted against a tool
   limit both depend on the order.

From this the rules for an Aurum executor follow.

- **Independent operands may be evaluated in any order or together.** This
  is the parallelism the fabric exists for, and it is unobservable.
- **Strictness is enforced, not assumed.** A net does not wait for an
  operand by nature: a call whose body ignores an argument can finish while
  that argument is still being reduced. If the argument diverges, the
  interpreter never returns, and a net lowered naively would return, or
  would perform a host operation the interpreter never reaches. The
  **completion rule** closes the gap: every evaluation the interpreter
  would have finished first produces a completion signal, and the final
  result and the sequencing wire at each `await` are held until those
  signals arrive. Inside pure code the net may still run ahead, because
  nothing observable leaves it early. An unused operand is run to
  completion before it is erased.
- **No speculation into an arm that is not selected.** The specification
  says of `match` that "no other arm's body is evaluated". A fabric that
  starts both arms can diverge or exhaust memory where the interpreter
  returns. The net fragment therefore steers a value to one arm. The static
  fragment may compute both arms and select one, as circuits normally do,
  because its arms are finite combinational logic and cannot diverge.
- **A fold stays a chain.** The runtime chapter forbids reassociating or
  parallelizing a fold on the grounds that its callback is pure. A net does
  neither: each step depends on the previous accumulator, and the net
  preserves that dependency. The cost is that such code runs serially on a
  parallel machine.
- **Memory exhaustion and tool limits are compared as outcomes, not as
  points in time.** The specification already treats exhaustion as a host
  event and a tool limit as an unknown outcome, never as a program result.
  An Aurum profile needs the same differential rule that native execution
  uses, stated for a machine whose peak memory differs.

Totality would remove the third item altogether, and the
[paradigm notes](../blue-sky/paradigms.md) already list it as worth adding.
Aurum does not need to wait for it: without speculation the fabric
evaluates exactly the subterms the interpreter evaluates, and with the
completion rule it releases nothing before the interpreter would.

## Parallelism the programmer has to write

A fabric finds parallelism only where the program's data dependencies allow
it. Idiomatic Vibra walks a `List` with `Iter.fold`, which is a serial
chain. Steele's argument applies unchanged: parallel execution needs
reductions over trees with associative combination, not left or right folds
over sequences [Steele09]. Bend reached the same conclusion in practice; its
documentation tells programmers that the one thing to avoid is code that is
inherently sequential [BendDocs].

Both of these functions are valid Vibra today and mean the same on every
executor. The first is in the static fragment and runs as a loop. The second
is outside the static fragment, because `Tree` is recursive and the calls
are not tail calls, and it is the shape that a net evaluates in parallel.

```vibra
fn sumTo
  takes {count: U64 total: U64}
  returns sum: U64
  do
    match count
      case 0u64
        return total
      case _
        return sumTo(U64.sub(count 1u64) U64.add(total count))
    endmatch
endfn

type Tree
  case leaf {value: U64}
  case node {left: Tree right: Tree}
endtype

fn sumTree
  takes {tree: Tree}
  returns sum: U64
  do
    match tree
      case Tree.leaf(value: value)
        return value
      case Tree.node(left: left right: right)
        return U64.add(sumTree(left) sumTree(right))
    endmatch
endfn
```

So the language is the same on both machines, and the performance idioms are
not. That is the honest form of "familiar": code written for a processor is
accepted and correct on the fabric, and code written to exploit the fabric
is accepted and correct on a processor. A library of tree-shaped collections
and reductions whose combining function carries an associativity law would
close the gap, and it depends on interface laws becoming proof obligations,
which the [scratchpad](../README.md#milestone-dependencies) already tracks.

## Collections

`List` is the one collection the compiler owns, and the native runtime
stores it contiguously and updates unshared storage in place. A graph fabric
has no cheap contiguous array. The specification fixes what a list observes
and not how it is stored: "The name `List` promises no linked list, no
copy-on-write, and no constant-time operation"
([Representation latitude](../../../spec/06-runtime.md#representation-latitude)).
An Aurum executor may therefore represent a list as a balanced tree
[RRB11]. Indexing then costs a logarithmic number of node
visits instead of one memory access, and splitting and concatenation become
cheap. Whether that trade is acceptable is one of the
[open questions](open-questions.md), and it is what hypothesis H4 of the
prospectus measures.

The prospectus proposes hash array mapped tries for maps [Bagwell01]. Vibra
has no hashing, and a `Dict` is ordered by its keys, so the structure that
fits is an ordered balanced tree, which also keeps iteration order
independent of any hash function.

## Ownership without an ownership system

The prospectus gives its graph IR linear, affine, and unrestricted
multiplicities with explicit `Dup` and `Erase`. The charter excludes an
ownership or borrowing system from the language. These do not conflict,
because the multiplicities can be inferred below the source language, and
the native design already infers them.

The [execution IR](../../nexus/design.md#ownership) tracks every slot as
empty or full. `Move`, `Build`, `Function`, `Call`, `TailCall`, and `Return`
consume their operands, every other use borrows, and `Copy` and `Drop` are
explicit. Its [verification rules](../../nexus/design.md#verification-rules)
reject a consumed operand list that names a slot twice and any path on which
a slot is read while empty. That is an affine discipline with explicit
duplication and erasure, which is what Perceus derives from its linear
resource calculus [Perceus21].

The correspondence with an interaction net is direct.

| Execution IR | Net |
| --- | --- |
| A slot that is written once and consumed once | A wire |
| `Copy` | A duplicator node |
| `Drop` | An eraser node |
| A borrowing use | A duplication followed later by an erasure, or a read port on a node that supports one |
| A function with its environment | A node holding the captured wires |

Two consequences matter for the fabric.

- **The net fragment needs no reference counts.** Sharing is expressed by
  duplicator nodes, so there is no counter to update atomically. This avoids
  the failure Massimult reports, where unsynchronized parallel reducers
  drove counts negative and reduction had to stop for recycling
  [Massimult24]. The price is that a duplicated structure is copied
  incrementally where a counted one would be shared in constant time.
- **Function bodies are instantiated, not duplicated.** Vibra functions are
  top-level definitions or closures over them, and evaluation is strict. A
  call can instantiate the callee's body from a read-only template, as the
  Reduceron does [Reduceron10]. Duplicating a partially reduced function
  body, which is what forces optimal reducers to carry brackets, oracles, or
  non-local rules [Lamping90] [Salvadori25], never arises. Only data is
  duplicated.

Whether the Aurum lowering starts from the execution IR or branches off
earlier is an [open question](open-questions.md). The execution IR's slots,
frames, and checkpoints describe a sequential machine, so the recommendation
is to branch from checked IR and share the ownership analysis.

### Where the runtime chapter assumes one worker

The [reclamation rules](../../../spec/06-runtime.md#reclamation) are written
for the native runtime, and three of them do not carry over word for word.

- **The mechanism.** "Storage is reclaimed by precise, non-atomic reference
  counting", and "one context is touched by one worker at a time". A fabric
  has many elements working on one context and, under the proposal above, no
  counts. An Aurum profile needs the rule stated as its observable
  consequence, not as its mechanism.
- **The bound.** That consequence is already in the chapter: the runtime
  "MUST reclaim unreachable storage before a computation whose live data is
  bounded grows without bound", and a tail loop that drops each earlier
  aggregate keeps live storage bounded. With eraser nodes, reclamation is
  work that competes with computation. A loop that produces garbage faster
  than erasers consume it breaks the bound, so the fabric owes a fairness
  guarantee: erasure is never starved. This is a scheduling obligation with
  no counterpart in the native runtime.
- **The measurements.** The runtime exposes live and peak storage to the
  conformance harness. Peak storage on a parallel fabric differs from the
  interpreter's, so those observations are per executor, as they already are
  between the interpreter and native code.

Values stay acyclic on the fabric for the reason the chapter gives: a value
is built from older values. An interaction net can express cycles, so the
lowering's verifier has to reject them, as a rule of the net IR.

## Effects and authority on the fabric

In whole-program mode the fabric has to perform host operations in the order
the program awaits them, because the order of completed host events is part
of what executors must agree on.

The theory is state threading in linear logic: a single linear token stands
for the right to perform the next operation, each `await` consumes it and
produces it again when the operation completes, and causality in the net
then enforces the order without a scheduler. Everything between two awaits
is pure, including the construction of plans, and may run in parallel.

The pieces map as follows.

| Vibra | Fabric |
| --- | --- |
| A `proc` body | A net with one sequencing wire threaded through its awaits |
| `Intent` | A value: a node that holds its operands and has not been given the sequencing wire. It is a different node kind from a function value, because the runtime chapter forbids erasing plans, procedures, and functions into one callable |
| `await` | Hands the plan the sequencing wire and waits for it to come back |
| A host operation | A request node sent to a broker at the edge of the fabric |
| A grant | An opaque index into a table the broker holds; the fabric can route it and cannot forge or inspect it |
| The audit trace | Written by the broker, which sees every request in order |
| Startup order and binding recipes | Performed by the broker before the entry net is started |

The broker is the one the runtime chapter already describes, which owns "the
live resource and accounting objects that sealed tokens refer to". It stays
conventional software on a conventional core. Nothing about the host
registry, the providers, or the policy language moves onto the fabric. This
keeps authority where the specification puts it, and it means the fabric
never needs a device driver.

Concurrency is out of scope here. When the actor layer ships, each actor is
a candidate for its own sequencing wire and its own region of the fabric,
and the scheduler's choices become ordered host responses exactly as they do
natively.

## Reporting what is unavailable

A rejection has to be useful to an agent, which means it must be complete,
located, and explainable.

- **Judge the reachable program, not the package.** Availability is decided
  for the code reachable from the kernel function or the entry, after
  specialization. A library may contain functions no fabric can run without
  preventing its other functions from being placed.
- **Report every unavailable construct in one pass**, each as
  `@tool.unavailable` spanning the whole form, with the profile, the reason
  atom, and the call path from the root that makes it reachable.
- **Expose availability as a queryable fact.** For each function: the
  smallest fragment that accepts it, and the reasons it is outside the
  smaller ones. This is the coeffect computed over the call graph, and it
  lets an agent see the answer before it builds, in the way it can already
  see an effect row.
- **Add no syntax.** A function is not annotated as fabric-ready. An
  annotation would be a second spelling of something the compiler computes,
  and it would split libraries into two worlds.

In co-processor mode none of this is an error. The same facts decide
placement, and the build reports which functions were placed and why the
others were not.

## How agreement is tested

An Aurum profile follows the pattern of `nativeV1`: no cases of its own. The
emulator runs the executable conformance cases whose programs fall inside
the profile's fragment, and every other case is reported unavailable, which
is neither a pass nor a failure. Expected results come from the
specification, as they do for native code. The emulator is deterministic by
construction; a fabric with asynchronous timing has to show that its timing
is unobservable, which is the confluence and determinacy argument of the
[execution-model note](execution-models.md).

[CompCert09]: bibliography.md#verified-compilers-and-hardware
[Coeffects14]: bibliography.md#functional-and-typed-hardware-compilation
[Lafont90]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Kahn74]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[SAFL00]: bibliography.md#functional-and-typed-hardware-compilation
[GoS07]: bibliography.md#functional-and-typed-hardware-compilation
[GoS11]: bibliography.md#functional-and-typed-hardware-compilation
[Clash]: bibliography.md#functional-and-typed-hardware-compilation
[Wavelet26]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[WGSL]: bibliography.md#functional-and-typed-hardware-compilation
[Dahlia20]: bibliography.md#functional-and-typed-hardware-compilation
[Calyx21]: bibliography.md#functional-and-typed-hardware-compilation
[Filament23]: bibliography.md#functional-and-typed-hardware-compilation
[Bagwell01]: bibliography.md#resources-ownership-and-reuse
[Reduceron10]: bibliography.md#graph-reduction-and-combinator-hardware
[Heron23]: bibliography.md#graph-reduction-and-combinator-hardware
[Steele09]: bibliography.md#resources-ownership-and-reuse
[BendDocs]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[RRB11]: bibliography.md#resources-ownership-and-reuse
[Perceus21]: bibliography.md#resources-ownership-and-reuse
[Massimult24]: bibliography.md#graph-reduction-and-combinator-hardware
[Lamping90]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Salvadori25]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
