# Execution models and their evidence

The prospectus lists six candidate execution models for the fabric: lambda
graph reduction, interaction combinators, rich typed interaction nets,
dataflow, spatial automata, and hybrids. This note places each in its theory,
records what has been built and measured, and says what that implies for a
fabric that has to run Vibra.

## The theory in one map

All six candidates are ways of running a proof net or a graph that
approximates one.

- **Linear logic** gives programs a graph form, the proof net, in which
  every value is used exactly once and copying and discarding are explicit
  [Girard87].
- **Interaction nets** generalize proof nets into a programming model:
  nodes with one principal port, rewrite rules that fire only where two
  principal ports meet, and therefore no two rewrites that can interfere.
  The result is strong confluence: every reduction order reaches the same
  normal form in the same number of steps [Lafont90]. Three node kinds
  suffice for universality [Lafont97].
- **There are two ways to run a net.** One rewrites the graph, which is
  parallel and needs an allocator. The other, from the geometry of
  interaction, leaves the graph fixed and sends a token around it, which
  needs no allocator and maps onto static hardware, but is sequential per
  token and repeats work [Girard89] [Mackie95] [DanosRegnier99].
- **Optimal reduction** shares every copy of a redex that the lambda
  calculus can share [Levy78] [Lamping90] [GAL92] [AspertiGuerrini98]. Its
  cost is in the
  bookkeeping that keeps sharing correct, which the count of shared steps
  does not measure [LawallMairson96] [AspertiMairson01]. How bad that is in
  practice is still argued [Asperti17] [GuerriniSolieri17].
- **Dataflow** is the first-order relative: a graph of operators connected
  by channels [Dennis74], determinate whenever each operator is a function
  of its input histories [Kahn74].

Strong confluence and Kahn determinacy are what let a fabric drop the global
program counter and the global clock without becoming nondeterministic. They
guarantee one result. They do not guarantee that the work is small, that it
is parallel, or that it is local in space.

## Interaction nets

**What has been measured.** HVM2 evaluates interaction combinators at about
400 million interactions per second on one thread of an Apple M3 Max, 5,200
million on 16 threads, and 74,000 million on an NVIDIA RTX 4090 [Taelin24].
The 16-thread figure is a thirteenfold speedup. An interaction is much
smaller than a function call, and independent reporting found Bend 1, the
language on HVM2, not competitive with Haskell or OCaml [LWN25]. Bend 1
also carried 24-bit numbers [BendDocs]. An earlier GPU evaluator observed
that the available parallelism depends on the program and on run-time
values [Jiresch14]. A compilation model for interaction nets on
conventional processors is described in [HMS15].

**Where the main proponent ended up.** Bend 2, released in September 2026,
has no interaction nets at run time; its author could not make them as fast
as conventional evaluation on ordinary hardware [Bend2]. This is the
finding behind the "avoid" verdict in the
[paradigm notes](../blue-sky/paradigms.md#interaction-nets-as-the-runtime).
Other interaction-net languages remain active [Vine].

**What is clean and what is not.** Δ-Nets shows the boundary precisely. For
linear lambda terms a single rule suffices and any order is correct. Once
erasure and sharing are admitted, the system needs canonicalization rules
that are not local and a leftmost-outermost order to stay optimal and
normalizing, and its claims are not mechanized [Salvadori25].

**What has not been built.** The search found interaction-net evaluators on
processors and on GPUs and no interaction-net machine in an FPGA or in
silicon. That gap is the prospectus's opportunity and its largest risk:
hypothesis H1 has no hardware prior at all.

**What Vibra needs from the model.** Much less than the lambda calculus
does. Vibra is strict, and its functions are named definitions or closures
over them. A call can instantiate a body from a template, so partially
reduced function bodies are never duplicated, and the machinery that optimal
reduction exists to manage does not arise. Only data is duplicated, and data
is first-order. The prospectus's "rich typed interaction nets" with one node
kind per constructor and a rule table per function are the right candidate;
pure interaction combinators are a compilation target of last resort,
because they spend many interactions encoding what one wide node can do.

## Graph-reduction machines

Hardware graph reduction has a forty-year record [SKIM80] [GRIP87], and its
modern results come from one design idea.

- **The Reduceron** instantiates supercombinator templates using wide,
  parallel memories, so that one reduction completes in very few clock
  cycles. It ran at 96 MHz on a Virtex-5 FPGA [Reduceron10], and against a
  conventional implementation on a soft RISC core on the same FPGA it was
  substantially faster [Reduceron12].
- **Heron** continues the line and reports up to six times as many
  reductions per clock cycle as GHC, in 2% of its FPGA [Heron23].

Both exploit parallelism inside a single reduction. Neither is a fabric of
many reducers. The two recent attempts at many reducers are cautionary.

- **Massimult** runs unsynchronized reducer threads over a shared cell pool.
  Reduction counts and memory use are nondeterministic, reference counts can
  go negative so reduction halts for recycling, and it exists only as a
  software virtual machine without performance numbers [Massimult24].
- **Fitchett and Fox** build a tree of cells that each hold one lambda
  expression. It runs only in logic simulation, on a 16-node cluster; it
  fails beyond 16 nodes, loads and reads results only through the root, and
  slows on unbalanced trees [FitchettFox26].

The lesson for the prospectus's processing elements is that a single wide
reducer is a strong baseline that a mesh of small ones has to beat, and that
the mesh's problems are allocation, load balance, and input and output, not
the rewrite rule.

## Dataflow

Dataflow is the candidate with silicon behind it, in both directions.

**Why the first generation lost.** Tagged-token machines needed an
associative store to match operands [Manchester85] [TTDA90]. Monsoon
replaced it with frames allocated per activation [Monsoon90], which is
closer to a conventional stack than its name suggests. Fine-grained dataflow
still executed two to four times the instructions of control flow for the
same program [Petersen06].

**Why the current generation wins where it does.** RipTide implements
ordered dataflow without tags, with control flow steered in the network, and
reports 6.6 times less energy than a von Neumann core and 25% less than the
previous energy-minimal design, synthesized in an Intel 22 nm process
[RipTide22] [SNAFU21]. It wins on energy for loop kernels, by removing
instruction fetch and by keeping the topology static. WaveScalar and the
EDGE designs occupy the middle: dataflow inside a region, conventional
sequencing outside [WaveScalar03] [TRIPS04].

**What has been proved.** Wavelet compiles a small sequential language to
RipTide-class dataflow with a proof in Lean 4 of forward simulation,
deadlock freedom, and determinacy, and it matches unverified compilers
closely. It pays for this with source restrictions that are exactly the
[static fragment](vibra-target.md#two-fragments): tail recursion only, no
mutual recursion, one static external call site per function [Wavelet26].

## Fixed nets and static schedules

Three lines of work show what the static fragment can become.

- **Geometry of Synthesis** compiles an affine higher-order language to
  circuits through its game semantics. The affine restriction is what makes
  the representation finite, and recursion is unfolded in space or in time
  [GoS07] [GoS11]. It is the token-passing reading of a net, made into
  hardware.
- **SAFL** reaches static allocation by restricting the language instead:
  first-order with tail recursion only [SAFL00].
- **MADAR** removes addresses and the program counter by circulating
  instructions and data on rings under a compile-time schedule. Its RTL is
  validated in simulation, and its energy model reports wins for small loops
  and losses for longer dependency chains and for pointer-chasing
  [MADAR26]. It is a useful example of publishing the cases a design loses.

## Spatial scale

The prospectus's topology hypothesis, H5, is about what happens when there
are many processing elements. Tascade's finding for conventional manycores
is the relevant prior: scaling to very many units is limited by
communication distance and by load imbalance, and it takes regional
coalescing of updates to scale to a million units [Tascade23].
SpiNNaker shows that a million-core asynchronous message-passing machine can
be built, for a workload whose graph is known before it runs [SpiNNaker14].
Network-on-chip and latency-insensitive design are the established
engineering for the fabric's wiring [DallyTowles01] [Carloni01]
[Chapiro84].

A rewrite fabric adds a problem these machines do not have: the graph moves.
A node created by a rewrite has to be placed somewhere, and the two nodes of
a future interaction may be created far apart. Placement and migration
policy is likely to dominate the fabric's performance, and none of the
sources read here measures it.

## Equality saturation

The prospectus lists e-graphs among its rewrite tools. The evidence places
them at compile time. E-graph rewriting of datapaths reduced circuit area by
up to 63% on industrial and open benchmarks and produced a certificate that
a standard equivalence checker accepts [ROVER24] [Egg21] [Egglog23]. For
Aurum that suggests e-graphs belong in the lowering from the net IR to node
layouts, where they can search over number representations and operator
implementations and emit a checkable trace. They are not a run-time model.

## What the evidence supports

| Model | Storage | Evidence in hardware | Fit for Vibra |
| --- | --- | --- | --- |
| Ordered dataflow, static topology | Fixed | Synthesized, measured, and verified [RipTide22] [Wavelet26] | The static fragment |
| Token passing on a fixed net | Fixed | Compiled to circuits [GoS11] | The static fragment, including function values that are used affinely |
| Single wide graph reducer | Dynamic, one heap | FPGA, measured [Reduceron10] [Heron23] | The net fragment; the baseline to beat |
| Typed interaction nets on many elements | Dynamic, distributed | None found | The net fragment; the prospectus's actual bet |
| Pure interaction combinators | Dynamic, distributed | None in hardware; software results discouraging [Taelin24] [Bend2] | Poor: too many interactions per operation |
| Spatial automata | Fixed cells | None read | Unknown |

**Recommendation.** Treat the fabric as a hybrid from the start, as the
prospectus allows. Lower the static fragment to ordered dataflow, where
there is silicon evidence and a verified compiler to learn from. Lower the
rest to typed interaction nets with template instantiation. Make the first
quantitative question of the project a comparison against a single wide
reducer, not against a processor, because that is the comparison a
many-element fabric is least likely to win and the one that decides whether
the mesh is worth building.

[Girard87]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Girard89]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Lafont90]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Lafont97]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Mackie95]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[DanosRegnier99]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Levy78]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Lamping90]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[GAL92]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[LawallMairson96]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[AspertiMairson01]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Asperti17]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[GuerriniSolieri17]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Salvadori25]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Taelin24]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[LWN25]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[BendDocs]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Bend2]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Vine]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Jiresch14]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[HMS15]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[AspertiGuerrini98]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Dennis74]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[SKIM80]: bibliography.md#graph-reduction-and-combinator-hardware
[GRIP87]: bibliography.md#graph-reduction-and-combinator-hardware
[Reduceron10]: bibliography.md#graph-reduction-and-combinator-hardware
[Reduceron12]: bibliography.md#graph-reduction-and-combinator-hardware
[Heron23]: bibliography.md#graph-reduction-and-combinator-hardware
[Massimult24]: bibliography.md#graph-reduction-and-combinator-hardware
[FitchettFox26]: bibliography.md#graph-reduction-and-combinator-hardware
[Kahn74]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[Manchester85]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[TTDA90]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[Monsoon90]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[Petersen06]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[WaveScalar03]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[TRIPS04]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[SNAFU21]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[RipTide22]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[Wavelet26]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[MADAR26]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[Tascade23]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[SpiNNaker14]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[DallyTowles01]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[Carloni01]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[Chapiro84]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[SAFL00]: bibliography.md#functional-and-typed-hardware-compilation
[GoS07]: bibliography.md#functional-and-typed-hardware-compilation
[GoS11]: bibliography.md#functional-and-typed-hardware-compilation
[ROVER24]: bibliography.md#equality-saturation
[Egg21]: bibliography.md#equality-saturation
[Egglog23]: bibliography.md#equality-saturation
