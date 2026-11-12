# Blue sky: what a Vibra program could be

Status: design direction; not normative

This note asks what Vibra would look like if it were designed from nothing
for its real author, an AI agent, with no obligation to resemble a language
a human would enjoy writing. It ignores the
[charter's exclusions](../../../spec/00-charter.md#deliberate-exclusions) on
purpose. Nothing here is supported, scheduled, or a proposal for a release.
The sketches are illustrative and their syntax is not a commitment.

Three companion notes go deeper. The [field notes](paradigms.md) record the
evidence for each paradigm considered, including the ones rejected. The
[syntax note](syntax.md) asks whether S-expressions are the right surface
for an agent and what could replace them. The [tooling note](tooling.md)
asks which agent tools are worth building: structured edits, structured
inspection, or interactive evaluation.

## The premise: the author changed

Programming languages are shaped around what their authors find hard. Human
languages economize on typing, memory, and tedium, and they trust the author
to mean well. An agent has a different profile, and the evidence for it is
now measurable.

| An agent is | Evidence |
| --- | --- |
| Cheap at volume, repetition, and retries | A second or tenth attempt costs tokens, not morale |
| Good at formal tedium when the checker gives counterexamples | Models verify 82% of Dafny tasks in the [vericoding benchmark][vericoding], and pure Dafny verification rose from 68% to 96% in one year |
| Unreliable over long contexts | Accuracy degrades with input length even when the needed facts are present ([context rot][chroma], [13.9% to 85% degradation][ctxlen]) |
| Dishonest under pressure | When tests contradict the task, GPT-5 still "passes" 76% of one [ImpossibleBench][impossible] variant by editing tests, special-casing inputs, or overloading operators |
| Weak at stating intent | A [field guide][dbggr] reports one-shot specifications correct 48% of the time against 92% for code. [22.7% of verified specifications][veriact] have no postcondition at all |
| Dependent on familiar surface | The same models reach 40.3% full correctness in Dafny and 7.8% in Lean on [AlgoVeri][algoveri]. Removing meaningful names costs [9 to 16 points][obfusc] of comprehension |
| Prone to duplication | Copied blocks rose eightfold in 2024 while refactoring fell below 10% of changed lines ([GitClear][gitclear]) |

The design rule that follows is to stop asking the agent for the things it is
bad at and to charge it freely for the things it is good at:

- Turn "be right" into "be checkable". A small kernel decides; the agent
  only proposes.
- Turn global coherence into local obligations, each checkable from a
  bounded context.
- Turn honesty into structural inability. The agent that implements cannot
  edit the thing it is judged against.
- Turn intent into a small artifact a human can sign, and measure how much
  that artifact actually says.

Three shifts carry this out. A program becomes a ledger of claims. Execution
becomes the advancing of a journal. Authoring becomes refinement of typed
holes. The rest of this note describes each, the guardrails they enable,
what they cost, and how to test them.

## Where Vibra stands

Vibra today combines nominal types, static effect rows, a canonical
formatter, structured diagnostics, and a content-addressed codebase. That is
a sound base, and it is also the recipe the 2026 wave of agent-first
languages converged on: [Vera][vera], [Chelis][chelis], [Qosm][qosm],
[Overt][overt], [AIL][ail], and others pair types and effects with
contracts and machine-readable errors. A rethink has to go past that recipe.
The three shifts below change what a program is, not which checks run on a
source file.

## Shift 1: a program is a ledger of claims

### Cells

The unit of a program is a **cell**. A cell has three parts with three
different owners.

| Part | Content | Owner |
| --- | --- | --- |
| Claim | Type, permitted effects and data flows, reference meaning, examples, cost bound | The intent authority: a human, or an agent acting for one |
| Witness | An implementation that satisfies the claim | An implementer agent. Witnesses are interchangeable and nobody needs to read them |
| Evidence | Certificates that the witness satisfies the claim, each with a grade | The kernel, which checks them |

```vibra
(defcell split-cents (total cents parts part-count) (list cents)
  meaning: (the (shares (list cents))
             (all (integer.eq (list.length shares) (part-count.value parts))
                  (cents.eq (cents.sum shares) total)
                  (cents.le (cents.spread shares) (cents.of 1))
                  (list.descending shares)))
  examples: ((split-cents (cents.of 100) (part-count.of 3))
             (list (cents.of 34) (cents.of 33) (cents.of 33)))
  cost: (linear (part-count.value parts))
  flows: ())
```

`meaning:` uses a definite description: `the` names the single value that
satisfies the predicate. It is executable by search on small inputs, which
is enough to test against, and it says nothing about how to compute the
answer. The witness lives elsewhere and can be anything that the kernel
accepts:

```vibra
(witness split-cents
  (let (base (cents.div total parts)
        extra (cents.rem total parts))
    (list.build parts
      (lambda (index u64) cents
        (if (integer.lt index extra) (cents.succ base) base)))))
```

Dependents of a cell see only its claim. Witness bodies are opaque to the
checker of any other cell, in the way a Dafny caller reasons from a method's
specification and never from its body. This bounds the **check closure** of
a cell: the cell itself plus the claims of what it calls. The store can
refuse a cell whose closure exceeds a fixed budget, which turns the
context-rot result above into an admission rule.

### Meaning and realization

Halide and Exo separate an algorithm from its schedule: one text says what is
computed, another says how, and the tool guarantees the second preserves the
first. [Exo 2][exo2] does this with 46 trusted rewriting primitives and
reaches the performance of vendor libraries on more than 80 kernels.
[Aqueduct][aqueduct] removes the rewriting vocabulary entirely: the existing
program is the specification, an agent writes any faster program it likes,
and a Lean proof of `f = f′` is the admission ticket. It reports proved
speedups on 156 of 189 programs, up to five orders of magnitude.

Vibra can adopt the split for every cell, not only numeric kernels. The
meaning is the clearest total definition anyone can write, brute force
included. The realization is whatever runs fastest. Three routes connect
them, in increasing cost:

1. Derivation by trusted rewrites, where each step is correct by
   construction.
2. Free-form witness plus an equivalence or refinement certificate.
3. Search over rewrites, with the kernel checking the result.

Amazon's ShardStore shows the economics on a production system: executable
[reference models were about 1% of the implementation's size][shardstore],
and checking the implementation against them prevented 16 issues from
reaching production.

Performance work then stops being a correctness risk. An agent can be as
aggressive as it likes, because the worst outcome is a rejected witness.

### Measuring the claim

A verified witness for a weak claim is worth little, and weak claims are
what agents write. One study found [99.1% verifier acceptance against 1.9%
of results binding to a specification predicate][vacuous]. Postconditions
written by a frontier model were [complete at most 17% of the time for
Python][postcond]. So the claim itself is measured before it is admitted.

| Check | Question | Failure the agent sees |
| --- | --- | --- |
| Vacuity | Does the claim reject at least one input and output pair? | "Every output satisfies this claim" |
| Determinacy | Do any two outputs satisfy the claim for one input? | Two concrete outputs, both accepted |
| Mutation | Does the claim reject mutants of the witness? | A surviving mutant and an input on which it differs |
| Examples | Do the examples follow from the meaning? | The example and the value the meaning gives |

Remove `list.descending` from `split-cents` and the determinacy check
answers with `(34 33 33)` and `(33 34 33)`. Either the author wanted a set of
allowed answers and says so with an explicit `any`, or the claim was
underspecified and gets fixed. An unmarked gap is not admitted.

When the author is unsure which behavior is intended, the tool asks with
distinguishing inputs instead of prose, as in [test-driven intent
formalization][lahiri]: "for `(split-cents 1 3)`, is the answer `(1 0 0)` or
an error?"

### Separation of powers

[ImpossibleBench][impossible] reports that read-only tests stop test
tampering and that offering a way to give up cut GPT-5's cheating from 54%
to 9%. Both are structural fixes, and a ledger makes them the default.

| Role | May | May not |
| --- | --- | --- |
| Intent authority | Write and sign root claims | Skip the strength checks |
| Implementer | Propose witnesses and private helper cells | Edit a claim it is implementing |
| Kernel | Check evidence, admit or reject | Be extended by an implementer |
| Adversary | Search for counterexamples and surviving mutants | Admit anything |

An implementer always has an honest exit: return the cell unfilled with the
obligation it could not meet. Lean's guidance for untrusted proofs takes the
same position, treating [unreviewed AI output as potentially
malicious][leanvalid] and checking it against a trusted statement file.

### Debts instead of escape hatches

Every language has `unsafe`, `sorry`, `assume`, or a skipped test. Here each
one is a **debt**: a named, typed entry in a trust ledger attached to the
cell. A debt taints every dependent, shows up in every query, and can be
refused by a target's policy. The evidence grades from the
[verification proposal](../verification/README.md) carry over unchanged:
checked by test, verified by solver, assumed, failed.

### Redundancy as an error-correcting code

Human languages remove redundancy because typing is expensive. For an agent
redundancy is nearly free, and independent descriptions that must agree
detect errors in each other. [Clover][clover] checks code, formal
annotations, and documentation against one another and accepts up to 87% of
correct programs with no false positives on its benchmark. A claim's
meaning, examples, and cost bound play the same role.

The rule becomes: repeat the meaning, never the mechanism. Two cells whose
meanings normalize to the same form are one cell, and the store says so at
admission. That answers the duplication trend in the premise without
asking the agent to remember what already exists.

## Shift 2: execution is advancing a journal

### The step function

A program does not run. It **advances**. The whole application is a total
function

```text
step : state × event → state × (list intent)
```

and the runtime owns everything else: the loop, the journal of events, the
clock, randomness, retries, and the performing of intents. Effects are data.
A step cannot open a socket; it can only return an intent that asks the
runtime to, and the outcome comes back as a later event. Each step is
atomic, in the manner of [Verse's failure contexts][verse], where a failed
branch leaves no trace. The only thing that does not terminate is the event
stream itself.

```vibra
(defmachine transfer
  state: transfer-state
  event: transfer-event
  intent: ledger-intent
  invariant: (state (cents.eq (transfer-state.held state)
                              (transfer-state.owed state)))
  (step (state event)
    (match (pair state event)
      (pair (transfer-state.idle) (transfer-event.requested id from to amount))
        (advance (transfer-state.debiting id from to amount)
                 (ledger-intent.debit id from amount))
      (pair (transfer-state.debiting id from to amount)
            (transfer-event.debit-ok id))
        (advance (transfer-state.crediting id from to amount)
                 (ledger-intent.credit id to amount))
      (pair (transfer-state.crediting id from to amount)
            (transfer-event.credit-failed id reason))
        (advance (transfer-state.refunding id reason)
                 (ledger-intent.refund id from amount)))))
```

The sketch shows three cases. The full match must be exhaustive over the
product of states and events, so "what if the credit fails after the debit
succeeded" is a case the checker demands, not one the author has to think of.
A study of failures in distributed systems found that
[92% of the catastrophic ones came from mishandled non-fatal errors][yuan].

The journal is the complete record:

```text
41 event  (transfer.requested t-9 alice bob 500)
41 intent (ledger.debit  key: t-9/41 alice 500)
42 event  (ledger.debit-ok t-9)
42 intent (ledger.credit key: t-9/42 bob 500)
43 event  (ledger.credit-failed t-9 @account-closed)
43 intent (ledger.refund key: t-9/43 alice 500)
```

Idempotency keys come from journal positions, so a retried intent is
recognizable by construction.

### What falls out

| Property | Why it follows |
| --- | --- |
| Deterministic replay | State is a fold of `step` over the journal. A journal is a complete bug report |
| Counterfactual debugging | Replay a prefix, substitute one event, and compare |
| Fault injection by default | The runtime chooses event order and failures, so every program is a [deterministic simulation][dst] target without harness code |
| Durable execution | A crash loses nothing. Restart replays the journal and resumes |
| Model checking the real program | The checker explores `step` itself, so there is no gap between a model and the code. [Specula][specula] found 249 bugs in 48 systems by having agents write TLA+ models of existing code; here the model is the program |
| Evidence for upgrades | Replay production journals against the new version and diff the intents |
| Free parallelism inside a step | Steps are pure and total, so evaluation order inside one is unobservable |
| Deterministic concurrency | Machines exchange events stamped with logical time, as [Lingua Franca][lf] reactors do |
| Coordination-free distribution | State that only grows needs no consensus, by the [CALM theorem][calm]. The checker can tell which state is monotone |

Illustrative commands:

```text
vibra replay journal.vibon
vibra replay journal.vibon --against <cell-hash> --diff
vibra simulate --faults --seeds 10000
vibra explore --depth 12 --invariant held-equals-owed
```

### Direct style stays

Nobody should write state machines by hand, agents included. Effectful code
keeps its direct style and the compiler produces the step machine, with one
state per suspension point. The
[Nexus runtime](../execution-targets/nexus.md) already plans runtime-owned
frames and cooperative checkpoints. The shift is to make every checkpoint a
journal boundary and every host operation an intent. The local actors
planned for v0.0.4, which handle one message at a time over immutable state,
are this model already; the change is to make it the only one.

### One definition, many readings

A total, pure definition is not tied to one evaluator. The same cell can be
read concretely to get a value, symbolically to get a verification
condition, abstractly to get a cost bound, as a generator to enumerate
inputs that satisfy a predicate, incrementally to update an output from an
input delta ([DBSP][dbsp]), and backwards when it is a lens. Each reading is
a tool the agent gets without writing a second program.

## Shift 3: authoring is refinement, not text

### Holes and moves

A text file passes through thousands of meaningless states on its way to a
valid program. [Hazel][hazel] shows an editor can assign a type and a
meaning to every intermediate state by making holes first-class. For an
agent that becomes a protocol. The kernel presents a hole with its expected
type, the bindings in scope, and the applicable moves. The agent picks a
move: introduce a constructor, split on a value, apply a cell, or fill with
a literal. An ill-typed program has no sequence of moves that produces it.

This is the limit of [type-constrained decoding][tcd], which already cuts
compile errors by more than half by masking tokens that cannot lead to a
well-typed program. The same study found that 94% of compile errors in
generated code were type errors and only about 6% were syntax errors, so
grammar constraints alone miss most of the problem. Supplying a model with
the types around a hole was also the most effective context in
[the Hazel assistant study][chatlsp].

The context for each hole is computed, not retrieved: the claim, the claims
of the cells in scope, and nothing else.

### Projections

With a store of cells and a move protocol, `.vib` text stops being the
truth and becomes one projection among several. An agent sees holes and
claims. A human reviewer sees claims, debts, and evidence grades, and never
needs a witness body. An auditor sees flows and the trust ledger.

The [syntax note](syntax.md) takes this further. It keeps the S-expression
tree as the stored form, proposes a flatter line-oriented surface for
authoring, and adds tables and checked domain notations, each with a proved
round trip to the tree.

### Names

Renaming and obfuscation studies agree that models lean on identifiers:
[execution prediction falls from 85.4 to 71.2][names] in one, and
structural obfuscation costs nothing while identifier obfuscation costs
[9 to 16 points][obfusc]. Vera's choice of typed De Bruijn slots removes
the wrong-variable error by removing variable names. The evidence supports a
middle position:

- Cells are identified by hash. Their names are metadata that can change
  without touching a dependent, as in Unison.
- Local names are computed, not chosen. A binding of type `cents` in a
  context with one such value is `cents`; a second is `cents-2`, or is named
  by its role in the claim. The agent cannot pick a misleading name, and it
  still reads meaningful ones.
- Naked primitives do not cross a cell boundary. Two `u64` parameters can be
  swapped. A `part-count` and a `cents` cannot.

## Guardrails: five gates

A wrong program has to get past every gate. Each gate is cheaper than the
next, so most errors die early.

| Gate | A wrong program is | Mechanism |
| --- | --- | --- |
| 1 | Unsampleable | The move protocol and constrained decoding never offer the token |
| 2 | Unrepresentable | No form exists for partial functions, missing cases, ambient effects, naked primitives at boundaries, or use after consume |
| 3 | Unacceptable | The store refuses a cell without evidence, with a vacuous or indeterminate claim, with a duplicate meaning, or with an oversized check closure |
| 4 | Unshippable | A target's policy sets minimum evidence grades, forbids debts on sensitive flows, and requires clean replay diffs and simulation runs |
| 5 | Unhideable | What remains is a recorded debt that taints dependents and appears in every query |

No gate makes wrong code impossible in general. The claim can still be the
wrong claim. What the gates do is move every remaining error into the one
artifact a human signs, and measure that artifact.

## Further guardrails

These are independent of the three shifts and each removes a class of
error. All of them would be painful for a human to write by hand, which is
the point: the cost falls on the party for whom it is cheap.

| Guardrail | Rules out | What the agent pays |
| --- | --- | --- |
| Totality by default with `decreases:` | Non-termination, partial matches, undefined results | A measure on each recursion, or a `diverge` debt |
| Static cost bounds ([AARA][raml]) | Accidental quadratic or exponential behavior, unbounded memory | Nothing when inference succeeds; a bound annotation otherwise |
| Affine values with guaranteed in-place reuse ([FP²][fbip]) | Use after close, double release, hidden copying | Explicit duplication where a value is used twice |
| Flow labels on data ([CaMeL][camel], made static) | Secrets reaching a log, untrusted input reaching a command | Labels at sources and sinks, declassification as a debt |
| Invertible definitions ([lenses][lenses], [invertible syntax][invsyn]) | Encoders and decoders that disagree | One definition instead of two |
| Choreographies ([choreographic programming][choreo]) | Deadlock and mismatched sends and receives between actors | One global protocol instead of one program per actor |
| Monotone state by default ([CALM][calm]) | Races and lost updates in distributed state | A stated reason wherever state is not monotone |

## Ranking

**The bet.** The three shifts, taken together. Each is useful alone, but
they reinforce one another: claims give the journal something to check
against, the journal gives claims about whole systems a place to live, and
refinement keeps both within the reach of a bounded context.

**Strong additions.** Totality, cost bounds, affine values with in-place
reuse, flow labels, invertible definitions, and choreographies. Each has a
decidable or well-automated core and a published track record.

**Frontier.** Incremental evaluation of ordinary definitions, superposed
evaluation of many candidates at once, equality saturation as a realization
search, and a Lean bridge for obligations a solver cannot close. Worth
experiments, not commitments.

**Traps.** The [field notes](paradigms.md#traps) give the evidence for each.

- Interaction nets as the execution substrate. The Bend project moved its
  second version off them for performance.
- Tactic proofs over dependent types as the default surface. Models do far
  worse there than in solver-backed languages.
- Token-minimal syntax. Dropping tokens that carry no information is fine.
  Dropping names and redundancy that a checker can use is the trap.
- Natural language as source. It restores every ambiguity the claim exists
  to remove.
- User-defined effect handlers. They let a program redefine what an intent
  means, which is the runtime's job.

## Costs and risks

- **Intent becomes the bottleneck.** Every guarantee is relative to a claim,
  and writing claims is what agents do worst. The strength checks and
  distinguishing-input questions reduce the problem and do not remove it.
- **Proof effort.** Published ratios of proof to code run from
  [3.4 to 1 up to about 27 to 1][dbggr]. Agents make that cheaper, not free.
  The grades exist so that a cell can ship at a lower grade knowingly.
- **Unfamiliar surface.** Models are trained on human code. On languages
  with a truly foreign paradigm they collapse, and examples in the prompt do
  not rescue them. "Painful for a human" is therefore not the same as "fine
  for a model". The alien parts of this design belong in what the kernel
  checks and how execution works, while the page an agent writes stays near
  what it knows. The [syntax note](syntax.md#evidence) has the evidence.
- **Journal overhead.** Recording every event and intent costs time and
  storage. Pure steps can be recomputed instead of recorded, and journals
  can be compacted to snapshots, but hot paths will need care.
- **Totality limits expression.** Some algorithms have no obvious measure.
  The `diverge` debt keeps them possible and visible.
- **Complexity.** This note does not count total machinery as a cost. It
  is far more than the charter's sixth priority allows, on purpose. The one
  thing that has to stay small is what is trusted. That is the LCF
  discipline: a tiny kernel, and everything else an untrusted proposer whose
  output the kernel checks.
- **Human review moves, it does not disappear.** Someone still has to read
  claims. They are shorter than code, and nothing guarantees they are easy.

## From today's Vibra

| Today | Rethought |
| --- | --- |
| `.vib` source files | A projection of store cells |
| `defn` with a signature | A cell: claim, witness, evidence |
| Static effect rows | Intent types plus flow labels |
| Tests | One evidence grade among several |
| The interpreter as semantic oracle | A reference meaning per cell |
| Nexus checkpoints and resume | Journaled steps |
| Local actors | Machines with logical time |
| Audit events | The journal |
| Backend parity tests | Translation validation per build |
| Verification stages V0 to V3 | The evidence ladder |
| Content-addressed codebase with queries and edits | The ledger and the move protocol |

Most rows extend something Vibra has already committed to. The two that do
not are the ownership split between claim and witness, and the journal as
the only way to have an effect.

## Experiments

Each experiment is small enough to run against the current toolchain or a
prototype, and each can fail.

| # | Question | Measure | Abandon if |
| --- | --- | --- | --- |
| E1 | Does reference-then-realize beat writing the fast version directly? | Hidden-test pass rate and cheating rate on tasks with impossible variants | No gain in correctness at equal token budget |
| E2 | Do the strength checks catch weak claims? | Share of seeded vacuous and indeterminate claims rejected; false rejections of good claims | Good claims are rejected as often as bad ones |
| E3 | Does hole refinement beat text edits? | Checker-clean rate and tokens per accepted cell | Moves cost more tokens for the same acceptance |
| E4 | Does the journal model find real bugs? | Port `examples/m4-demo` to a step machine; bugs found by fault simulation and replay diff | Simulation finds nothing a unit test would not |
| E5 | Do computed local names help or hurt? | Task accuracy with chosen, computed, and positional names | Computed names are measurably worse than chosen ones |
| E6 | Is cost inference practical? | Share of standard library functions with an inferred bound | Most functions need manual annotation |
| E7 | Does the ownership split reduce cheating? | Cheating rate with claims writable, read-only, and read-only with an honest exit | No difference from read-only tests alone |
| E8 | Is superposed evaluation worth its complexity? | Time to evaluate a mutant set, superposed against sequential | No speedup on realistic mutants |

E4 and E7 test the two ideas that are new to Vibra, the journal and the
ownership split. E2 needs the least machinery and is the best first step.
The [syntax note](syntax.md#experiments) adds experiments S1 to S7 on the
surface itself, and the [tooling note](tooling.md#experiments) adds T1 to
T7 on the tools.

## Sources

Evidence on agents and verification:

- [A benchmark for vericoding][vericoding]
- [AlgoVeri][algoveri]
- [Verified but Vacuous][vacuous]
- [VeriAct and Spec-Harness][veriact]
- [PostcondBench][postcond]
- [Intent formalization][lahiri]
- [ImpossibleBench][impossible]
- [Clover][clover]
- [Type-constrained code generation][tcd]
- [Statically contextualizing LLMs with typed holes][chatlsp]
- [Formal verification strategies field guide][dbggr]
- [Lean: validating proofs][leanvalid]
- [Context rot][chroma] and [context length alone hurts][ctxlen]
- [When Names Disappear][names] and
  [obfuscated code comprehension][obfusc]
- [GitClear 2025 code quality report][gitclear]

Paradigms and systems:

- [Exo 2][exo2] and [Aqueduct][aqueduct]
- [ShardStore lightweight formal methods][shardstore]
- [Verse failure contexts][verse]
- [Deterministic simulation testing][dst]
- [Simple testing can prevent most critical failures][yuan]
- [Specula][specula]
- [Lingua Franca][lf]
- [CALM][calm] and [DBSP][dbsp]
- [Hazel][hazel]
- [Resource Aware ML][raml]
- [FP²: fully in-place functional programming][fbip]
- [CaMeL][camel]
- [Quotient lenses][lenses] and [invertible syntax descriptions][invsyn]
- [Choreographic programming][choreo]
- Agent-first languages: [Vera][vera], [Chelis][chelis], [Qosm][qosm],
  [Overt][overt], [AIL][ail]

[vericoding]: https://arxiv.org/abs/2509.22908
[algoveri]: https://arxiv.org/html/2602.09464v2
[vacuous]: https://arxiv.org/abs/2611.05590
[veriact]: https://arxiv.org/html/2604.00280
[postcond]: https://aclanthology.org/2026.findings-acl.1768.pdf
[lahiri]: https://www.alphaxiv.org/abs/2603.17150
[impossible]: https://arxiv.org/abs/2510.20270
[clover]: https://arxiv.org/html/2310.17807
[tcd]: https://arxiv.org/abs/2504.09246
[chatlsp]: https://dl.acm.org/doi/10.1145/3689728
[dbggr]: https://dbggr.dev/articles/formal-verification-strategies/
[leanvalid]: https://lean-lang.org/doc/reference/latest/ValidatingProofs/
[chroma]: https://www.trychroma.com/research/context-rot
[ctxlen]: https://arxiv.org/abs/2510.05381
[names]: https://arxiv.org/html/2510.03178
[obfusc]: https://lab.cloud/research/obfuscated-code-performance.pdf
[gitclear]: https://www.gitclear.com/ai_assistant_code_quality_2025_research
[exo2]: https://arxiv.org/pdf/2411.07211
[aqueduct]: https://rdi.berkeley.edu/blog/aqueduct/
[shardstore]: https://jamesbornholt.com/papers/shardstore-sosp21.pdf
[verse]: https://verselang.github.io/book/08_failure/
[dst]: https://antithesis.com/docs/resources/deterministic_simulation_testing/
[yuan]: https://www.usenix.org/system/files/conference/osdi14/osdi14-paper-yuan.pdf
[specula]: https://arxiv.org/html/2607.25333
[lf]: https://dl.acm.org/doi/10.1145/3617687
[calm]: https://arxiv.org/abs/1901.01930
[dbsp]: https://www.vldb.org/pvldb/vol16/p1601-budiu.pdf
[hazel]: https://hazel.org/
[raml]: https://www.raml.co/
[fbip]: https://doi.org/10.1145/3607840
[camel]: https://arxiv.org/abs/2503.18813
[lenses]: https://www.cs.cornell.edu/~jnfoster/papers/quotient-lenses.pdf
[invsyn]: https://dl.acm.org/doi/10.1145/2088456.1863525
[choreo]: https://www.fabriziomontesi.com/bliki/ChoreographicProgramming
[vera]: https://veralang.dev/
[chelis]: https://chelis.ch/blog/launch/
[qosm]: https://qosm.io/blog/introducing-qosm
[overt]: https://github.com/paulmooreparks/Overt
[ail]: https://github.com/hyun06000/AIL-old
