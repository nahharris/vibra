# Aurum open questions

Each question has a recommendation and the evidence that would settle it.
None of them is decided.

## Scope and naming

### 1. "Nexus" names two things

In this repository Nexus is the native compiler and runtime: the
[Nexus plan](../../nexus/README.md), the `vibra-nexus` crates, and the
v0.0.3 roadmap. The prospectus uses "Nexus Core" for a portable graph IR
and "Nexus Physical" for node layouts and encodings.

**Recommendation.** Keep Nexus for the native runtime, which is scheduled
work with crates behind it, and rename the prospectus's two layers. These
notes say **net IR** and **layout** for them. If the prospectus's intent is
that the graph IR eventually replaces the execution IR, the two should be
one design with one name, and that is a decision for the v0.0.3 roadmap, not
for a scratchpad.

### 2. Where Aurum lives

The prospectus sketches one repository holding `vibra/`, `nexus/`, `aurum/`,
and `experiments/`.

**Recommendation.** A separate repository for the fabric, the emulator, the
experiments, and any Lean development. This repository gains code only when
an Aurum profile has a specification: one lowering crate that consumes
checked IR. The charter asks for an implementation "small enough to finish
and verify", and the prospectus itself says the Vibra roadmap must not
change silently.

**Settled by** whether the lowering needs anything from the compiler that
checked IR does not already export.

### 3. What Aurum is for, in Vibra's terms

The charter's mission is command-line programs, libraries, and small
services written by coding agents. A new machine architecture is outside
that mission.

**Recommendation.** State Aurum's value to Vibra narrowly: it is a third
executor that tests whether the language's semantics are as
machine-independent as the specification claims. Every place the Aurum
lowering finds the specification depending on a sequential machine is a
finding about Vibra, whatever happens to the fabric.

## The target

### 4. One Aurum profile or two

**Recommendation.** Two, for the
[static and net fragments](vibra-target.md#two-fragments), with the static
profile included in the net profile. Build the net profile first, on the
emulator: it accepts the most Vibra for the least analysis, and it is the
one that tests H1. Build the static profile when something physical is
targeted, because that is where an FPGA result is cheapest.

### 5. Whole programs or kernels first

**Recommendation.** [Kernel mode, then co-processor mode](vibra-target.md#three-ways-to-use-the-fabric).
Whole-program mode needs the effect broker and is a milestone of its own.

### 6. When unavailability is reported

At build time the answer is certain, because the platform is known. At
check time it would need the project to declare its platforms.

**Recommendation.** Build time first, with `@tool.unavailable`, the profile,
and a reason atom, plus a query fact per function. A `platforms` field on a
target, so that `check` reports it, is an additive project-format change to
propose only after the reason set has stabilized.

### 7. Where the lowering branches off

**Recommendation.** From checked IR, sharing the ownership analysis with the
native lowering. The execution IR's slots, frames, and checkpoints encode a
sequential machine and would have to be undone.

**Settled by** reading how much of the consume-and-borrow inference can be
separated from slot allocation in `vibra-lower`.

### 8. Sharing by counting or by duplicating

**Recommendation.** Duplicator and eraser nodes in the net fragment, with no
reference counts on the fabric.

**Settled by** the H6 measurement in the
[correctness note](correctness.md#where-each-proposal-can-live): if
duplication of large structures dominates, the fabric needs shared read-only
regions, which brings counting back for those regions only.

### 9. How a list is stored

**Recommendation.** As a balanced tree on the fabric, with the cost model
stated in the profile. The runtime chapter permits it: `List` "promises no
linked list, no copy-on-write, and no constant-time operation".

### 10. Reclamation stated as a mechanism

The runtime chapter requires reference counting and one worker per context.
A fabric satisfies the chapter's observable bound on live storage by other
means, and only if [erasure is never starved](vibra-target.md#where-the-runtime-chapter-assumes-one-worker).

**Recommendation.** If an Aurum profile is ever specified, restate the
reclamation rule as the bound, with counting as the native runtime's way of
meeting it. Until then, record it as the clearest place where the
specification describes a machine and not a meaning.

### 11. Speculation

**Recommendation.** None across `match` and `if` in the net fragment. Both
arms may be computed in the static fragment, where arms cannot diverge.
Revisit if totality is added to the language.

### 12. Parallel reductions

**Recommendation.** No automatic reassociation, ever; the runtime chapter
forbids it. Add a tree-shaped collection and a reduction whose combiner
carries an associativity law, as a library, once interface laws are proof
obligations. This agrees with the existing position that
[automatic parallel evaluation remains a research question](../concurrency/README.md#data-parallelism).

### 13. Floating point

**Recommendation.** Unavailable in the first profiles.

### 14. Concurrency

**Recommendation.** One sequencing wire per program until the actor layer
ships. Then revisit, because actors with isolated state and ordered
mailboxes are the natural unit to map onto regions of a fabric.

## Number systems

### 15. Library types or new scalars

**Recommendation.** Library types, as a pure Vibra package, with no new
scalar and no new language role. This runs today's executors and gives H3 a
software result before any hardware exists.

### 16. Exact domains without big integers

Exact rings grow their coefficients, and Vibra has only fixed-width
integers.

**Recommendation.** Bounded domains whose overflowing operations return
`Result`. A big-integer type built on `List` is a separate library, outside
the static fragment. A rational type cannot take this route; see
[question 37](#37-unbounded-integers).

### 17. Whether to build base-φ digit arithmetic at all

**Recommendation.** Yes, in the emulator only, as a registered experiment
with redundant binary as the control and a negative result as the
[stated expectation](number-systems.md#parallel-addition-and-hypothesis-h2).
No fabric decision should wait on it.

## Evidence

### 18. The baseline for H1

**Recommendation.** Three baselines, reported together: Vibra's own native
executor on the same program, a single wide graph reducer of the Reduceron
kind in the same emulator, and an energy model of a conventional core.
Report the cases the fabric loses, as [MADAR26] does. A result that beats
only the interpreter means nothing.

### 19. The benchmark programs

**Recommendation.** Vibra programs from the conformance corpus and the
standard library first, because they exist and were not written for the
fabric, then the prospectus's suite. Programs written to flatter the
fabric, such as balanced tree reductions, are reported separately.

### 20. How much to verify

**Recommendation.** An executable reference for the net IR, differential
agreement with the Vibra interpreter, and a Lean model of the net IR's
rules and their confluence. No end-to-end claim. Details in the
[correctness note](correctness.md#recommendation).

### 21. When to go physical

The cheapest silicon route offers tiles of about 160 by 100 micrometres,
roughly a thousand gates each on the older shuttles [TinyTapeout]. One
processing element with a node arena, a queue, and a rule matcher does not
fit in a tile.

**Recommendation.** FPGA is the first physical step, and only for the static
fragment or a single processing element. Before any shuttle, write down
what a multi-tile design would demonstrate that the FPGA did not.

### 22. What would make Aurum roadmap work

**Recommendation.** Use the scratchpad's
[existing rule](../README.md#moving-a-proposal-into-a-release-roadmap): user
evidence or a charter goal, shipped prerequisites, drafted specification
changes including the profile and its diagnostics, and a release with a demo
and exit gates. The prerequisite that matters is the native cutover in
v0.0.3, because the co-processor mode builds on its value store and its
ownership analysis.

## The operating environment

These come from the [operating-system](operating-system.md),
[shell](shell.md), and [interaction](interaction.md) notes.

### 23. Where a session's authority comes from

The charter allows two roots of authority: a binding the project target
declares, and the root grant the test runner issues. A shell that hands
grants to tools at a prompt looks like a third.

**Recommendation.** No third root. A session is a project, its resources
are the binding recipes of its target, and the shell is a driver around the
toolchain in the way `vibra run` is. To run a plan, the shell writes an
ordinary entry `proc` into the session's source that receives the target's
grants, narrows them, and awaits the plan. The generated entry is visible,
checked, and approved like any other.

**Settled by** whether a prototype needs anything the target file cannot
express. The first candidate is a typed result: an entry returns `ExitCode`
only, so a run can report a value only by writing it somewhere.

### 24. Revocation

The effects chapter excludes revoking a single grant, and a long session
will want to take authority back.

**Recommendation.** Keep the exclusion. The unit of withdrawal is the
world: each run gets its own, and ending the run makes its grants unusable.
A session that wants finer control runs smaller plans.

### 25. A document or a prompt

**Recommendation.** A [document and a log](shell.md#a-session-is-a-document-and-a-log).
A prompt that accumulates bindings needs live redefinition, which the
tooling chapter excludes, and it produces sessions that cannot be
reproduced from what is on screen.

### 26. A command syntax

**Recommendation.** None. The command language is Vibra as written, and
the cost in keystrokes is met by an agent or by the editor. A second
syntax is a second thing to check and to learn, and the charter prefers
one canonical way.

**Settled by** how much of a session people write by hand, and how often
they give up on small tasks.

### 27. Rehearsal needs a world the user can choose

"The shipped launcher has no user-selectable world or replay switch", and
rehearsal is exactly a run against a chosen world.

**Recommendation.** A profile of its own, in the operating-environment
project, built on the mechanism the conformance harness already has. The
shipped launcher stays as it is.

### 28. Replay from the audit trace

**Recommendation.** Try it on the interpreter before anything else. The
trace records the arguments and the complete result of every host call, so
replay should need nothing more. If it does, that is a finding about the
runtime chapter's claim of determinism for "ordered host responses". An
unredacted trace holds everything a program read, and has to be treated as
sensitive.

### 29. Compensation

Undoing an external effect by a declared inverse would need each operation
to name its inverse, which is a change to the operation registry.

**Recommendation.** Not yet. Rehearsal and holding cover a first
environment. Revisit when there are operations on resources that other
parties hold, which the standard inventory does not have today.

### 30. Net images

**Recommendation.** A [net image](operating-system.md#the-store) is a
cache tied to the program's witness identities and the net-IR version, and
never the only copy of anything. The consequence to accept: after a
toolchain change, a suspended run with effects cannot be resumed. It is
ended, and its log says how far it got.

### 31. Where hashing happens, and at what grain

**Recommendation.** At the boundary with the store, for whole arguments of
coarse calls. Interning nodes on the fabric needs a global table.

**Settled by** memo hit rates and hashing cost on the tooling workloads.

### 32. Budgets

The charter excludes "a language-level budget for time, memory, or host
operations".

**Recommendation.** Budgets are limit profiles of the environment, never
something a program can read or name. Reaching one pauses the task, as a
scheduler quota does, and the user decides whether to add budget or drop
it.

### 33. The cost of the completion rule

The [completion rule](vibra-target.md#what-an-aurum-run-may-change) adds a
signal to every evaluation the interpreter would have waited for.

**Recommendation.** Adopt it; the alternative is an executor that disagrees
with the interpreter on programs that diverge. Count the added nodes in
stage 2, and omit the signal wherever the toolchain already knows an
evaluation terminates.

**Settled by** the measured overhead on the conformance corpus.

### 34. Where protection lives

**Recommendation.** In two places: the net-IR verifier at admission, and a
hardware check at region boundaries. The first is trusted software, as the
compiler already is. The second protects against a faulty element and not
against a hostile program. Claim no information-flow security.

### 35. Where this work lives

**Recommendation.** In its own project, hosted on an ordinary operating
system, using the toolchain's machine contracts and no private interface.
[Most of the environment needs no fabric](interaction.md#what-needs-the-fabric).
Nothing enters the Vibra roadmap. What the project owes Vibra is a list of
what the contracts lacked: so far, a typed result from a run, a world the
user can select, and whether the audit trace suffices for replay.

### 36. Who the first user is

**Recommendation.** An agent, through the existing server. The view for a
person is a rendering of the same facts and can follow. This matches the
charter's account of who writes Vibra, and it lets the environment be
tested before any interface is designed.

## Exact numbers in the language

These concern Vibra on every machine, and come from the proposal to put
[exact rationals beside the floats](number-systems.md#exact-rationals-beside-the-floats).
They are decisions for the specification, recorded here because the Aurum
number work raised them.

### 37. Unbounded integers

A rational over fixed-width integers overflows within a few dozen
operations, so an exact default for fractional quantities needs integers
that grow. Vibra's scalar set is fixed-width today.

**Recommendation.** Add an unbounded natural and an unbounded integer, with
representations the compiler owns, and keep the fixed widths for layout
and interchange. A version built on `List` is enough to try the idea and
too slow to be the default. Tool limits have to charge these operations by
size.

**Settled by** a pilot: whether agent-written programs choose the
unbounded types unprompted, and what the size-dependent cost does to work
accounting.

### 38. Rationals beside the floats, or in place of them

**Recommendation.** Beside them. `Rational` is a library type and the
standard library's default for anything that is not whole. `F32` and `F64`
stay unchanged for interchange and for numeric work at a fixed cost.
Removing the floats would delete several special rules from the
specification, and it would leave iterative numerics with rounding placed
by hand and no interchange type.

**Settled by** how often a pilot's programs reach for `F64` where a
rational would do. If the answer is rarely, removal can be reconsidered
with evidence.

### 39. Literals for a library number type

An unsuffixed literal is typed by its expected type among the builtin
number types, so a library `Rational` cannot be written `2.5`.

**Recommendation.** Do not make `Rational` a builtin for the sake of its
literals. Decide whether a general rule should let any type accept a
decimal literal exactly, which would serve the algebraic domains as well.
Until then a rational is a constructor call.

### 40. Conversion between floats and rationals

"The floating-point types convert to and from other number types through
no interface."

**Recommendation.** Keep that sentence true. From a float to a rational is
a static member that returns `Result`, failing for the infinities and NaN.
From a rational to a float is a static member whose name states the
rounding. Neither is `From` or `TryFrom`, so no expected type ever turns
an exact number into an approximate one.

[MADAR26]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[TinyTapeout]: bibliography.md#fabrication-access
