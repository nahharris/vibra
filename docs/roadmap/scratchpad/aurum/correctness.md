# Resources, determinism, and verification

The prospectus puts correctness first among its principles. It proposes
linear, affine, and unrestricted multiplicities, refinement and dependent
indices, session types, solver-backed contracts, and a refinement chain from
Vibra down to a circuit, with Lean as the candidate proof assistant. This
note sorts those proposals by where they can live, given what the
[charter](../../../spec/00-charter.md#deliberate-exclusions) excludes from
the language, and by what the verified-systems literature says they cost.

## Theory

- **Substructural type systems** classify values by whether they may be
  duplicated and discarded. Linear values admit neither, affine values may
  be discarded, relevant values may be duplicated, and unrestricted values
  admit both. These are the fragments of linear logic [Girard87], and they
  match the fragments of the lambda calculus in which sharing-graph
  reduction is progressively harder [Salvadori25].
- **Multiplicities as part of a type** are well studied: on function arrows
  [LinearHaskell18], as usage counts in a dependent theory [QTT18]
  [Idris2], and as graded modalities [Granule19].
- **Reference counting is a linear resource calculus.** Perceus derives
  precise `dup` and `drop` instructions from one, and reuses storage in
  place when a count is one [Perceus21] [Beans19]. A fragment of functional
  programs needs no allocation at all [FP2].
- **Compiler correctness is simulation** between source and target
  semantics [CompCert09], and determinism of a concurrent target is
  confluence of its rewrite relation [Lafont90] or determinacy of its
  network [Kahn74].

## Where each proposal can live

| Prospectus proposal | Charter position | Where it can live |
| --- | --- | --- |
| Linear, affine, and unrestricted multiplicities | "an ownership or borrowing system" is excluded | In the net IR, inferred. The [execution IR](../../nexus/design.md#ownership) already infers the same facts |
| Explicit `Dup` and `Erase` | Not a language feature | In the net IR, inserted by lowering |
| Refinement and dependent indices | Bare guards are static obligations discharged by fixed rules; dependent types are not part of the language | Vibra's guards as they are |
| Solver-backed contracts | The checker discharges guards by fixed rules; "quantified contracts, user-written proofs" are excluded | Outside the language |
| Session types | No protocol types in the language | In the net IR, for the fabric's own channels, if anywhere |
| A refinement chain to the circuit | "verified-compiler claims" are excluded | A separate research artifact that makes no claim on behalf of the toolchain |

The pattern is that every resource and protocol discipline the prospectus
wants can be an invariant of the IR, established by the lowering and checked
by a verifier, without becoming something a Vibra programmer writes. That is
how the execution IR's verification rules work today, and it is consistent
with keeping the surface small.

It also makes hypothesis H6, that resource types reduce run-time overhead,
testable with no language change. Lower the conformance corpus and the
standard library to a net IR, and measure the fraction of values that are
used exactly once, the number of duplicator and eraser nodes per function,
and how much duplication moves large structures. Those three numbers say
whether inferred linearity is enough or whether the source language would
have to help.

## Determinism

Vibra's determinism commitment is for fixed source, inputs, and ordered host
responses, and executors must agree on the order of completed host events.
A fabric with no global clock has to show that its timing cannot be
observed. The argument has three parts.

- **Pure computation.** If every rewrite rule is an interaction between two
  principal ports, the net is strongly confluent and the result does not
  depend on firing order. A rule that inspects more than two nodes, or a
  node with two principal ports, breaks this, and each such extension needs
  its own confluence argument. Nested pattern matching and the non-local
  canonicalization rules of optimal reducers are the cases to watch
  [Salvadori25].
- **Sharing.** Unsynchronized reference counting made Massimult's results
  nondeterministic in cost and unsafe in reclamation [Massimult24]. A fabric
  that shares by duplicator nodes has no counters to race on.
- **Effects.** A [sequencing wire](vibra-target.md#effects-and-authority-on-the-fabric)
  makes the order of host requests a matter of causality in the net, and the
  broker serializes them.

A fabric must not contain an arbiter or a nondeterministic merge that the
program's result can depend on. Where the hardware needs arbitration, for
example to share a link, it has to be confluent at the level of the net:
whichever request wins, the same net results.

## What verified stacks have cost

| Project | What was proved | What it took | Source |
| --- | --- | --- | --- |
| CakeML on Silver | One theorem from ML source to a processor's behaviour, composed from the compiler's theorem and the processor's, and run on an FPGA | An existing verified compiler [CakeML14] and a proof-of-concept processor designed for the purpose, in HOL4 | [Silver19] |
| The bedrock2 lightbulb | An end-to-end theorem about one concrete embedded program on a pipelined RISC-V processor | A purpose-built language, compiler, and processor in Coq | [Lightbulb21] [Kami17] |
| Vericert | A C-to-Verilog compiler proved correct in Coq | Performance comparable to an unverified tool only with that tool's optimizations off, and about 4.5 times slower with them on, in the authors' early comparison | [Vericert21] |
| Wavelet | Control-flow conversion and linking to asynchronous dataflow, proved in Lean 4: simulation, deadlock freedom, determinacy | 797 lines of verified code, 1,534 of specification, 14,819 of proof; about four person-months for the hardest proofs; tail recursion only; operator selection and optimization left unverified | [Wavelet26] |

Three lessons recur.

1. **Every success restricted the source.** None verified a language of
   Vibra's size against hardware. The restrictions are the
   [static fragment](vibra-target.md#two-fragments).
2. **Proof is roughly an order of magnitude larger than the code it
   covers,** and the parts left unverified are the ones that decide
   performance.
3. **Translation validation is the cheaper tool.** Checking each
   compilation's output against its input, as WaveCert does for dataflow
   [WaveCert23] and Wavelet does for its front end, gives per-program
   assurance without a compiler proof. Rule-based hardware languages with
   verified compilers are the other ingredient worth borrowing for the
   processing element itself [Koika20].

## Recommendation

Hypothesis H8, that end-to-end refinement is tractable, is plausible for the
static fragment as a multi-year research result and implausible for the net
fragment in any near term. The useful order of work is the one Vibra already
practices, extended one level down.

1. **An executable reference.** A small Rust evaluator for the net IR,
   deterministic and slow, that defines what a net means.
2. **Differential agreement.** The Vibra interpreter remains the oracle.
   Every kernel the Aurum lowering accepts is run on both and compared, on
   the existing conformance corpus. This is evidence, in the specification's
   sense, and not a proof.
3. **A mechanized model of the net IR alone.** The rewrite rules, their
   confluence, and the absence of stuck nets for well-typed inputs, in Lean.
   This is small, it is where a proof assistant earns its cost, and it is
   independent of the Vibra compiler.
4. **Translation validation** of the lowering for the static fragment, if
   and when that fragment targets real hardware.

No step here makes a verified-compiler claim, and none adds a proof
obligation to Vibra programs.

[Girard87]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Lafont90]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Salvadori25]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Massimult24]: bibliography.md#graph-reduction-and-combinator-hardware
[Kahn74]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[Wavelet26]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[WaveCert23]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[LinearHaskell18]: bibliography.md#resources-ownership-and-reuse
[QTT18]: bibliography.md#resources-ownership-and-reuse
[Idris2]: bibliography.md#resources-ownership-and-reuse
[Granule19]: bibliography.md#resources-ownership-and-reuse
[Perceus21]: bibliography.md#resources-ownership-and-reuse
[Beans19]: bibliography.md#resources-ownership-and-reuse
[FP2]: bibliography.md#resources-ownership-and-reuse
[CompCert09]: bibliography.md#verified-compilers-and-hardware
[Silver19]: bibliography.md#verified-compilers-and-hardware
[CakeML14]: bibliography.md#verified-compilers-and-hardware
[Lightbulb21]: bibliography.md#verified-compilers-and-hardware
[Kami17]: bibliography.md#verified-compilers-and-hardware
[Koika20]: bibliography.md#verified-compilers-and-hardware
[Vericert21]: bibliography.md#verified-compilers-and-hardware
