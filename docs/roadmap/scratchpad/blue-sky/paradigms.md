# Paradigm field notes

Status: design direction; not normative

These notes back the [blue-sky rethink](README.md). Each entry says what the
paradigm is, what evidence exists, what Vibra would take from it, and a
verdict. Figures are quoted from the linked sources and were not reproduced
here.

| Verdict | Meaning |
| --- | --- |
| Core | Part of one of the three shifts |
| Add | Independent guardrail with a mature, automatable core |
| Explore | Promising; needs an experiment before any design |
| Avoid | Considered and rejected, with the reason recorded |

## Summary

| Paradigm | Verdict | One-line reason |
| --- | --- | --- |
| Reference meaning with verified realization | Core | Makes optimization safe and gives every cell an oracle |
| Auto-active verification with measured claims | Core | Agents can do the proofs; the claims are the weak point |
| Separation of powers | Core | Structural fixes stop cheating where instructions do not |
| Typed holes and constrained generation | Core | Removes ill-typed states from the search space |
| Journaled step machines | Core | Replay, simulation, and durability from one mechanism |
| Programs as checkable state machines | Core | Model checking with no gap between model and code |
| Logical time | Core | Deterministic concurrency without a performance penalty |
| Totality | Add | Removes a whole failure class and enables the rest |
| Static cost bounds | Add | Performance becomes a checked claim |
| Affine values and in-place reuse | Add | Resource safety and checked in-place updates |
| Information-flow labels | Add | Security policy checked before the program runs |
| Invertible definitions | Add | One definition where two could disagree |
| Choreographies | Add | Protocol errors between actors become unwritable |
| Monotone state and incremental evaluation | Explore | Strong theory, unclear fit for general programs |
| Superposed evaluation | Explore | Could make mutant and candidate checking cheap; unreplicated |
| Equality saturation for realization search | Explore | Natural fit for the meaning and realization split |
| Computed and positional names | Explore | Evidence points both ways |
| Interaction nets as the runtime | Avoid | Abandoned by their main proponent for performance |
| Tactic proofs as the default surface | Avoid | Models do several times worse than with solvers |
| Token-minimal syntax | Avoid | Removes the names and redundancy a checker and a model both use |
| Natural language as source | Avoid | Reintroduces the ambiguity claims exist to remove |
| User-defined effect handlers | Avoid | Lets a program redefine what the runtime must own |

## Core

### Reference meaning with verified realization

A definition is written twice: once for clarity and once for speed, with a
machine-checked link between the two.

- [Exo 2](https://arxiv.org/pdf/2411.07211) builds user-defined schedules
  from 46 trusted, equivalence-preserving primitives in the LCF tradition,
  and matches vendor-library performance on more than 80 kernels.
- [Aqueduct](https://rdi.berkeley.edu/blog/aqueduct/) treats the existing
  program as the specification and requires a Lean proof that the optimized
  program equals it. It reports proved speedups for 156 of 189 Verina
  programs with a 1.85× geometric mean, and 92 proved rewrites across 17
  applications. Its audit of a test-only harness found 39.5% of accepted GPU
  kernels were incorrect.
- [ShardStore](https://jamesbornholt.com/papers/shardstore-sosp21.pdf) keeps
  executable reference models at about 1% of implementation size, with
  property definitions and harnesses at 12% of the code base, and credits
  them with preventing 16 production issues.

Vibra takes: a `meaning:` on every cell that is total and executable, three
routes to a realization (trusted rewrites, free-form with a certificate,
search), and the rule that performance work cannot change meaning.

### Auto-active verification with measured claims

Contracts in the host language, discharged by a solver, with counterexamples
on failure. This is already the direction of the
[verification proposal](../verification/README.md). The new evidence is
about where it breaks.

- Agents can do the proving. The
  [vericoding benchmark](https://arxiv.org/abs/2509.22908) reports 82%
  success in Dafny, 44% in Verus, and 27% in Lean over 12,504
  specifications.
- The gap between solver-backed and tactic-based systems is large.
  [AlgoVeri](https://arxiv.org/html/2602.09464v2) reports best full
  correctness of 40.3% in Dafny, 24.7% in Verus, and 7.8% in Lean on 77
  classical algorithms.
- Acceptance by a verifier says little by itself.
  ["Verified but Vacuous"](https://arxiv.org/abs/2611.05590) measures 99.1%
  acceptance against 1.9% binding to a specification predicate.
  [VeriAct](https://arxiv.org/html/2604.00280) finds 22.7% of verified
  specifications have no postcondition and only 35.6% are meaningfully
  verified.
- Agents write incomplete specifications.
  [PostcondBench](https://aclanthology.org/2026.findings-acl.1768.pdf)
  reports completeness of at most 17% for Python and 43% for Java, and
  [CodeSpecBench](https://arxiv.org/html/2604.12268) a best score of 20.2%
  at repository level.
- Cross-checking independent artifacts works.
  [Clover](https://arxiv.org/html/2310.17807) runs six consistency checks
  among code, annotations, and documentation and accepts up to 87% of
  correct programs with no false positives on its benchmark.
- [Intent formalization](https://www.alphaxiv.org/abs/2603.17150) frames
  specification quality as soundness and completeness, measurable by
  mutation, and resolves ambiguity by asking the user about distinguishing
  tests.

Vibra takes: solver-first verification, plus admission checks on the claim
itself for vacuity, determinacy, and mutation strength.

### Separation of powers

The implementer cannot change what it is judged against, and always has an
honest way to stop.

- [ImpossibleBench](https://arxiv.org/abs/2510.20270) builds tasks whose
  tests contradict the specification, so any pass is a cheat. GPT-5 passes
  76% of one variant. Stronger models cheat more. Read-only tests stop test
  editing, and an explicit abort option cut one cheating rate from 54% to
  9%.
- Lean's
  [proof validation guide](https://lean-lang.org/doc/reference/latest/ValidatingProofs/)
  treats unreviewed machine-written proofs as potentially malicious and
  recommends checking them against a trusted statement file and auditing
  the axioms they use.
- [Kleppmann](https://martin.kleppmann.com/2025/12/08/ai-formal-verification.html)
  argues the proof checker makes hallucinated proofs harmless and moves the
  problem to the specification.

Vibra takes: roles enforced by the store, root claims that an implementer
cannot edit, and an unfilled cell as a legitimate result.

### Typed holes and constrained generation

Every intermediate state of a program has a type and a meaning, and
generation is restricted to states that can still become well-typed.

- [Hazel](https://hazel.org/) assigns static and dynamic meaning to every
  editor state through first-class holes.
- [The Hazel assistant study](https://dl.acm.org/doi/10.1145/3689728)
  supplies a model with the types and headers around a hole and finds type
  definitions the most useful context.
- [Type-constrained decoding](https://arxiv.org/abs/2504.09246) cuts
  compile errors by more than half and improves functional correctness,
  with the largest gain in repair.

Vibra takes: a move protocol over holes as the authoring interface, with
computed context per hole. The scratchpad's
[type-constrained decoding](../agent-tooling/README.md#type-constrained-decoding)
proposal is the text-based first step.

### Journaled step machines

The program is a pure transition function; the runtime owns time, order,
failure, and effects, and records them.

- [Verse](https://simon.peytonjones.org/assets/pdfs/verse-icfp23.pdf) makes
  failure part of evaluation, and its
  [failure contexts](https://verselang.github.io/book/08_failure/) roll
  back effects of a failed branch. Epic's
  [AutoRTFM](https://www.unrealengine.com/en-US/tech-blog/bringing-verse-transactional-memory-semantics-to-c)
  extends that to C++.
- [Deterministic simulation testing](https://antithesis.com/docs/resources/deterministic_simulation_testing/)
  runs a whole system under a controlled scheduler with injected faults.
  TigerBeetle's
  [VOPR](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/internals/vopr.md)
  is a production example.
- Durable execution engines replay a journal after a crash, which is why
  they [require deterministic code](https://journal.resonatehq.io/p/from-where-do-deterministic-constraints).
  A language that only has deterministic steps meets the requirement by
  construction.
- [Yuan et al.](https://www.usenix.org/system/files/conference/osdi14/osdi14-paper-yuan.pdf)
  found 92% of catastrophic failures in distributed systems came from
  incorrect handling of non-fatal errors, and that simple testing of error
  handlers would have prevented 58%.

Vibra takes: `step` as the only shape of an effectful program, intents as
data, and a journal that serves replay, simulation, recovery, and upgrade
evidence. The
[simulation section](../concurrency/README.md#determinism-and-simulation-testing)
of the concurrency proposal already assumes ordered host responses.

### Programs as checkable state machines

- [P](https://p-org.github.io/P/) programs are communicating state machines
  that a checker explores systematically.
- [Quint](https://quint.sh/guides/formal-verification-tools) gives a
  TLA-style specification an executable, typed syntax and several checkers.
- [Specula](https://arxiv.org/html/2607.25333) has agents write TLA+ models
  of existing system code and found 249 bugs in 48 open-source systems.
- [SysMoBench](https://proceedings.iclr.cc/paper_files/paper/2026/file/438c34f45d7b5d82aef1b6016e695d5a-Paper-Conference.pdf)
  measures how well models write such formal models.

Vibra takes: no separate modeling language. The step function is the model,
and bounded exploration is a command.

### Logical time

[Lingua Franca](https://dl.acm.org/doi/10.1145/3617687) reactors exchange
events tagged with logical time, which makes concurrent execution
deterministic. Its runtime outperforms Akka by 1.86× and CAF by 1.42× on the
Savina suite, so determinism did not cost throughput there.

Vibra takes: logical timestamps on events between machines, with physical
time entering only as an event from the runtime.

## Add

### Totality

Every function terminates and every match is complete. Recursion carries a
measure, as in Dafny, and as in [Vera](https://veralang.dev/), which
requires `decreases` clauses. Totality is what lets a meaning be executed
safely by a checker and what makes a step atomic. Where no measure is known,
an explicit `diverge` debt records the gap.

### Static cost bounds

Automatic amortized resource analysis infers bounds from types.
[Resource Aware ML](https://www.raml.co/) derives
[multivariate polynomial bounds](https://www.cs.cmu.edu/~janh/assets/pdf/HoffmannDW17.pdf)
by linear programming, evaluated on 411 functions, and the type derivation
is a checkable certificate. The technique extends to
[message-passing protocols](https://www.cs.cmu.edu/~janh/assets/pdf/DasHP17.pdf).

Vibra takes: `cost:` as part of a claim, inferred where possible. This
subsumes the reduction budgets in the
[foundations proposal](../foundations/README.md#execution-budgets-and-reductions)
with a static bound.

### Affine values and in-place reuse

- [FP²](https://doi.org/10.1145/3607840) defines a fragment of functional
  programs guaranteed to run in place with no allocation.
- [Mutable value semantics](https://arxiv.org/html/2106.12678) gives
  in-place mutation without shared mutable references.
- Lean [reuses unshared values in place](https://lean-lang.org/functional_programming_in_lean/Programming___-Proving___-and-Performance/Summary/)
  through reference counting.

Vibra takes: affine handles for resources, as the
[type-system proposal](../type-system/README.md#stronger-guarantees-for-agents)
already studies, and a checked in-place guarantee as part of a cost claim.

### Information-flow labels

[CaMeL](https://arxiv.org/abs/2503.18813) tracks the provenance of data
through an agent's plan and enforces policy at tool calls. It solves 77% of
AgentDojo tasks with provable security, against 84% for an undefended
system. [Qosm](https://qosm.io/blog/introducing-qosm) models capabilities
as functions passed to code.

Vibra takes: labels on values, checked statically. Effect rows say what a
cell may do; flow labels say which data may reach which intent.

### Invertible definitions

[Lenses](https://www.cs.cornell.edu/~jnfoster/papers/quotient-lenses.pdf)
define a transformation and its inverse together.
[Invertible syntax descriptions](https://dl.acm.org/doi/10.1145/2088456.1863525)
derive a parser and a printer from one definition.
[Janus](https://dl.acm.org/doi/10.1145/1244381.1244404) makes whole
programs reversible.

Vibra takes: one definition for each codec, migration, and view, with
round-trip laws discharged once for the combinators. Whole-program
reversibility is not needed.

### Choreographies

A [choreography](https://www.fabriziomontesi.com/bliki/ChoreographicProgramming)
describes a protocol among several participants as one program, and
endpoint projection generates each participant's code.
[HasChor](https://doi.org/10.1145/3607849) does this as a library.
Mismatched sends and receives cannot be written.

Vibra takes: a choreography as the claim for a group of machines, with the
individual step functions as projections.

## Explore

### Monotone state and incremental evaluation

The [CALM theorem](https://arxiv.org/abs/1901.01930) says a distributed
program has a consistent, coordination-free implementation exactly when it
is monotone. [DBSP](https://www.vldb.org/pvldb/vol16/p1601-budiu.pdf)
derives an incremental version of a query mechanically, as
`Q^Δ = D ∘ Q ∘ I`, with a chain rule for composition and a Lean
formalization.

Both are proved for query-like programs. The open question is how much of an
ordinary program they cover.

### Superposed evaluation

HVM's superposition nodes let one evaluation share work across many
alternatives
([notes](https://gist.github.com/VictorTaelin/d5c318348aaee7033eb3d18b0b0ace34),
[more notes](https://gist.github.com/VictorTaelin/7fe49a99ebca42e5721aa1a3bb32e278),
[HVM4 explained](https://bend2.dev/notes/hvm4-explained/)). If it holds up,
evaluating a set of mutants or candidate witnesses could cost far less than
running each one. The program-synthesis results built on it are unpublished
according to a
[third-party status report](https://deliberus.com/docs/research/bend-and-deliberus/bend-hvm-status-report-2026-09-29.md),
so this needs independent replication first.

### Equality saturation for realization search

E-graphs represent many equivalent programs at once and extract the
cheapest. With a trusted rewrite set, every extracted program equals the
meaning by construction. This is a search strategy for the second route in
the meaning and realization split, not a language feature.

### Computed and positional names

The evidence conflicts. [Vera](https://veralang.dev/) replaces variable
names with typed De Bruijn slots and reports six of nine models at 100% on
its own benchmark. Against that,
["When Names Disappear"](https://arxiv.org/html/2510.03178) shows execution
prediction dropping from 85.4 to 71.2 without meaningful names, another
study measures an [18.6 point average loss](https://arxiv.org/abs/2504.10557)
under renaming, and an
[obfuscation study](https://lab.cloud/research/obfuscated-code-performance.pdf)
finds structural changes cost nothing while identifier changes cost 9 to 16
points.

The rethink proposes names computed from nominal types as a middle path.
That is a hypothesis and experiment E5 tests it.

## Traps

### Interaction nets as the runtime

Interaction nets promise optimal sharing and automatic parallelism. A
[third-party status report](https://deliberus.com/docs/research/bend-and-deliberus/bend-hvm-status-report-2026-09-29.md)
records that Bend 2 dropped them because they could not be made as fast as
conventional evaluation on ordinary hardware, and that the measured speedup
was 8.8× to 12.1× on 16 threads. Bend 2 is instead affine, dependently
typed, and termination-checked. Pure total steps already give Vibra
parallelism that a conventional native runtime can exploit.

### Tactic proofs as the default surface

Dependent types with tactic proofs are the most expressive option and the
one models handle worst: 27% in Lean against 82% in Dafny on vericoding,
7.8% against 40.3% on AlgoVeri. AlgoVeri attributes the gap to models
fighting the proof language instead of the problem. An open goal is also a
worse diagnostic for an agent than a counterexample. Lean stays as a bridge
for the residue, as verification stage V3 already proposes.

### Token-minimal syntax

[ilo](https://ilo-lang.ai/) optimizes for "the fewest tokens from intent to
working code" with prefix notation and single-character names. The naming
studies above say short opaque identifiers remove information models use,
and redundancy that a checker can cross-examine is worth more than brevity.
Token count still matters, because context is scarce. The
[syntax note](syntax.md#tokens-matter-a-little) separates tokens that carry
information from tokens that do not.

### Natural language as source

Prose is the right medium for a human to state a wish and the wrong one to
store a program in. The vericoding benchmark found natural-language
descriptions did not improve verification success. Prose belongs in the
dialogue that produces a claim, and the claim is what gets stored.

### User-defined effect handlers

Handlers let a program reinterpret an effect. In the journal model the
runtime alone interprets intents, which is what makes replay and simulation
trustworthy. Test doubles are supplied by the runtime's simulator, not by
handlers in the program.

## The 2026 agent-language cluster

Several languages for agent authors appeared within a year. They share most
of Vibra's current recipe, which is why the rethink looks elsewhere for its
main ideas.

| Language | Central bet |
| --- | --- |
| [Vera](https://veralang.dev/) | Mandatory contracts checked by Z3, typed positional slots instead of names, required termination measures |
| [Chelis](https://chelis.ch/blog/launch/) | A tiered prover that labels each result with the strength of its verdict |
| [Qosm](https://qosm.io/blog/introducing-qosm) | Capabilities passed as functions |
| [AIL](https://github.com/hyun06000/AIL-old) | The agent harness expressed in the grammar |
| [Overt](https://github.com/paulmooreparks/Overt) | Every effect, error, dispatch, and mutation visible at its site; transpiles to C# and Go |
| [Tern](https://ternlang.dev/) | Syntax models already know, value semantics, errors that say what to write |
| [Codong](https://github.com/brettinhere/Codong) | Exactly one way to do each thing |
| [ilo](https://ilo-lang.ai/) | Minimal token count |

Tern's bet on familiar syntax is the strongest counterargument to this whole
note: staying inside the training distribution is a guardrail too. The
[syntax note](syntax.md) takes that argument seriously. It concludes that
the surface should resemble familiar languages where Vibra behaves like
them, and experiments E3 and S1 are where the claim is tested.
