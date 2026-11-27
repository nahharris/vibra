# Aurum

Aurum is a proposed non-von-Neumann machine: a fabric of small processing
elements that compute by rewriting a graph locally, with no global program
counter, no mandatory global clock, no universal machine word, and no
globally coherent mutable heap. Its research charter and architecture
prospectus (draft 0.1, 25 November 2026) is kept outside this repository.
The prospectus names Vibra as the source language at the top of its stack.

These notes record the literature research behind that idea and work out
what an Aurum target would mean for Vibra, and what an operating system, a
shell, and an interface for such a machine could be. They are research.
They change no specification chapter and no release roadmap, and the
prospectus itself asks that experimental features stay out of the language
roadmap until evidence justifies them.

| Note | Question it answers |
| --- | --- |
| [An Aurum target for Vibra](vibra-target.md) | How one language serves two kinds of machine, which Vibra features a fabric can run, and how the compiler reports the rest |
| [Execution models and their evidence](execution-models.md) | What has been built and measured for each model the prospectus considers |
| [Number systems](number-systems.md) | Whether the prospectus's arithmetic holds up, how numeric domains reach Vibra, and whether exact rationals should sit beside the floats |
| [Resources, determinism, and verification](correctness.md) | Where linearity and proofs can live, given the charter, and what verified stacks have cost |
| [An operating system for the fabric](operating-system.md) | What is left for an operating system to do when isolation, authority, and determinism come from the machine and the language |
| [A shell for the fabric](shell.md) | How launching, naming, piping, and job control change when the command language is Vibra |
| [Interaction](interaction.md) | Which conventions of using a computer lose their cause, what a person can perceive instead, and how little of it needs the fabric |
| [Open questions](open-questions.md) | Every undecided point, each with a recommendation |
| [Bibliography](bibliography.md) | Every source, with how far it was checked |

## The prospectus in brief

The stack has four layers: Vibra; a portable graph IR; a physical layer that
chooses node layouts and encodings; and the fabric. The prospectus calls the
two middle layers Nexus Core and Nexus Physical. That name is
[already taken here](open-questions.md#1-nexus-names-two-things), so these
notes say **net IR** and **layout**.

Its principles are correctness, performance, representation pluralism,
semantic immutability, parallelism, and falsifiability. Its distinctive
commitments are that the number system is a choice per value, that
computation is local graph rewriting, and that every architectural claim is
a hypothesis with a stated failure condition. It plans an emulator before
any hardware.

## Findings

**The language does not need to change.** An Aurum target is a conformance
profile and a platform. The specification already says that a profile is
not a dialect, that an executor rejects what it cannot run before running
anything, and that the rejection carries the profile and a reason. Vibra on
Aurum is the same language with the same meaning, and less of it is
available. [Details](vibra-target.md#what-the-specification-already-provides).

**Not every feature should be available, and the line is principled.** The
hardware-compilation literature divides programs by whether their storage
can be fixed before they run. That gives two nested fragments of Vibra: a
**static fragment** that lowers to fixed circuits, and a **net fragment**,
all of pure Vibra, that needs a fabric able to allocate.
[The feature table](vibra-target.md#feature-availability) goes through the
language construct by construct.

**The first Aurum target should be a function, not a program.** Because
`fn` is pure, a function can be placed on the fabric while the program
around it runs natively, and a function the fabric cannot take stays on the
processor. An unsupported feature then costs speed and never correctness.
[Details](vibra-target.md#three-ways-to-use-the-fabric).

**Vibra is unusually well suited to this.** It has no traps on user data,
its contracts leave no run-time checks, purity is visible in the kind of
callable, and memory exhaustion is already a host event. Its execution IR
already tracks ownership with explicit copies and drops, which is the
discipline an interaction net needs, so the prospectus's resource types can
be inferred below the language without the ownership system the charter
excludes.

**The specification describes one machine in a few places.** The clearest
is reclamation, which is stated as reference counting with one worker per
context. Finding those places is useful to Vibra whatever happens to
Aurum. [Details](vibra-target.md#where-the-runtime-chapter-assumes-one-worker).

**The existing verdict on interaction nets stands.** The
[paradigm notes](../blue-sky/paradigms.md#interaction-nets-as-the-runtime)
advise against interaction nets as Vibra's runtime, because their main
proponent abandoned them for performance on ordinary hardware. Nothing
found here contradicts that. Aurum asks a different question, whether
hardware built for the model changes the cost, and for that question there
is no evidence in either direction: no interaction-net machine in an FPGA or
in silicon was found. [Details](execution-models.md#interaction-nets).

**Strict evaluation has to be enforced on a net.** A net lowered naively
can return, or perform a host operation, while an operand the interpreter
would still be waiting for is running. The target note adds a
[completion rule](vibra-target.md#what-an-aurum-run-may-change) for it.

**The operating system is small, and it shares the fabric in space.**
Isolation is connectivity, authority is a grant on a wire, and determinism
is confluence, so what remains is to allot the fabric, guard the boundary,
keep a store of values, and admit programs. A task is a region, a world,
and a budget. Nothing takes turns, so a scheduler divides area, and two
counters the fabric can read, live nodes and active pairs, say how little a
task fits in and how much it can use.
[Details](operating-system.md#what-an-operating-system-is-for).

**The shell is Vibra, and a session is a document.** The specification
excludes a REPL, live redefinition, permission prompts, and dynamic
loading, and a shell needs none of them: a session is a module whose
values are derived and cached, with a log of the runs that touched the
world. A plan is shown before it runs and can be rehearsed against a fork
of the user's data. [Details](shell.md#a-session-is-a-document-and-a-log).

**Most of the new interaction needs Vibra and not Aurum.** Plans before
effects, authority handed over as an argument, one history, replay, and
exact numbers follow from the language and can be tried on today's
executors. The fabric adds what is new to interaction design: computations
with a shape and no sequence, room in place of priority, and running work
that can be paused, moved, and put away.
[Details](interaction.md#what-needs-the-fabric).

## Relation to the charter

| Prospectus | Charter | Consequence |
| --- | --- | --- |
| Immutable values, pure functions, explicit effects | The same commitments | Nothing to reconcile |
| A second machine for the same programs | "Two executions, one meaning", with the interpreter as the reference | Aurum is a third executor under the same agreement rule |
| Linear and affine multiplicities | "an ownership or borrowing system" is excluded | Inferred in the net IR, never written in source |
| Solver-backed and dependent contracts | Contracts are checked statically by fixed rules; "quantified contracts, user-written proofs" are excluded | Vibra's guards as they are |
| A refinement chain to the circuit | "verified-compiler claims" are excluded | A separate artifact; agreement is tested, as it is for native code |
| Many numeric domains | A fixed scalar set and a library-first core | Library types |
| Automatic parallelism | Determinism for fixed source, inputs, and ordered host responses | Allowed for pure code, where order is unobservable; never for the order of host events |
| A new architecture | A language for command-line programs, libraries, and small services | Outside the mission; justified only as an experiment that also tests the specification |
| A native shell and desktop | Permission prompts, dynamic loading, runtime plugins, and language-level budgets are excluded | Tools around the toolchain that need none of them; nothing a program can name |

## The hypotheses, and what to expect

The prospectus states eight hypotheses. The summaries are paraphrases.

| Hypothesis | Prior expectation from the literature | Cheapest decisive test |
| --- | --- | --- |
| H1: local rewriting can compete with a conventional core | Unknown in hardware. Discouraging in software [Taelin24] [Bend2]; encouraging for a single wide graph reducer [Heron23] | Emulated fabric against an emulated single reducer on the same programs |
| H2: redundant golden-ratio arithmetic shortens carries | Fails against redundant binary, by theory [FPS11] [FPS16] | Already answerable on paper; [details](number-systems.md#parallel-addition-and-hypothesis-h2) |
| H3: exact algebraic domains pay for themselves | Plausible in niches, with production precedent [GilesSelinger13]; the baseline is adaptive-precision predicates [Shewchuk97] | A pure Vibra library on today's executors |
| H4: graph memory suits immutable updates | Mixed. Persistent trees gain; the baseline is in-place reuse of unshared storage, which Vibra already permits [Perceus21] | List and dict workloads in the emulator |
| H5: spatial topology yields usable parallelism | Limited by communication distance and load imbalance [Tascade23], and by program shape [Steele09]; placement of a moving graph is unmeasured | Emulator with a wire-delay model |
| H6: resource types reduce run-time overhead | Likely for counting, with a copying cost to measure | Count duplicators and erasers when lowering the existing corpus; no hardware needed |
| H7: the fabric suits neural-symbolic workloads | Logarithmic arithmetic has a measured energy benefit for low-precision training [LNSMadam22]; the symbolic half was not researched | Not attempted here |
| H8: end-to-end refinement is tractable | For the static fragment, as multi-year research [Wavelet26]; not for the net fragment | A Lean model of the net IR alone |

## A path that leaves the roadmap alone

Each stage produces evidence without depending on the next, and none is
scheduled work.

| Stage | Work | Evidence it yields | Needs |
| --- | --- | --- | --- |
| 1 | Numeric domains as a pure Vibra package | H3 in software; benchmark programs | The v0.0.2 language |
| 2 | A net IR, its reference evaluator, and a kernel-mode lowering from checked IR | Agreement with the interpreter on the pure conformance cases; the H6 counts; a list of the places the specification assumes a sequential machine | Checked IR |
| 3 | An emulator with processing elements, topology, and a cost model | H1 against a single reducer and against native code; H4; H5 | Stage 2 |
| 4 | The static fragment lowered to ordered dataflow, on an FPGA | The first physical measurement, on the model with the most prior evidence | Stage 2 |
| 5 | Co-processor mode | A Vibra program that runs natively with kernels on the emulator | The v0.0.3 native runtime |
| 6 | Whole-program mode with the broker | Effects, grants, and the audit trace on the fabric side | Stage 5 |

Stages 1 and 2 are small, need no hardware, and are worth doing even if
Aurum goes no further, because both report on Vibra.

The operating environment is a second track with the same property. A
session, a store, a memo table, replay, and rehearsal can be built on the
interpreter, hosted on an ordinary operating system, before stage 3
exists. [Where that work lives](open-questions.md#35-where-this-work-lives)
is an open question.

[Taelin24]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Bend2]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Heron23]: bibliography.md#graph-reduction-and-combinator-hardware
[FPS11]: bibliography.md#number-systems-and-exact-arithmetic
[FPS16]: bibliography.md#number-systems-and-exact-arithmetic
[GilesSelinger13]: bibliography.md#number-systems-and-exact-arithmetic
[Shewchuk97]: bibliography.md#number-systems-and-exact-arithmetic
[LNSMadam22]: bibliography.md#number-systems-and-exact-arithmetic
[Perceus21]: bibliography.md#resources-ownership-and-reuse
[Steele09]: bibliography.md#resources-ownership-and-reuse
[Tascade23]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[Wavelet26]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
