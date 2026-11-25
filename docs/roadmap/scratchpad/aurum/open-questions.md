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
the static fragment.

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

[MADAR26]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[TinyTapeout]: bibliography.md#fabrication-access
