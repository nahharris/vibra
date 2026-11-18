# Blue-sky adversarial review: protocol

Status: working notes; not normative

Two reviewers debate the whole blue-sky proposal: **Claude** (terminal
`t13`) and **Codex** (terminal `t1c`). Hannah Harrisonn owns the project
and asked for this review. Her position: Vibra is written by LLMs, so
LLM evidence decides, not her taste. Her ideas are inputs to test, not
rulings.

## What is under review

Everything in `docs/roadmap/scratchpad/blue-sky/`: the rethink
(`README.md`), `paradigms.md`, `syntax.md`, `tooling.md`, and the draft
specification in `spec/` (chapters 00 to 09). Nothing is fixed. Goals,
assumptions, syntax, semantics, compiler, runtime, execution model,
paradigms, and tooling may all be reshaped or deleted.

The standing aim: a language that is **clean and small on the surface
without giving up guarantees or expressiveness**. Collapse close
concepts, reuse shapes, use familiar terms.

## Owner inputs

Hannah answered three questions on 2026-11-15. These set scope; they do
not settle design questions.

1. **Priority of program kinds:** command-line programs first, then
   libraries, then services. Native UI is for the future. Weigh features
   by that order.
2. **Human reading:** people will read some code. The target is claims
   readable enough that a person can trust the witness nearly blindly.
   Human readability of claims therefore counts as a measured criterion,
   beside model error rate and token cost.
3. **New language:** Vibra will be built. Challenging the premise was
   welcome, but from here the debate spends its effort on *how to build
   it*, not on whether to. Comparisons with existing languages remain
   useful as baselines and as sources of design, not as alternatives.

## Rules

1. **Attack first.** Look for what is wrong, unsupported, redundant, or
   not worth its cost. Agreement must be earned by a failed attack.
2. **Tag every claim** with its support:
   `[cited]` (a source with a link), `[measured]` (an experiment run in
   this review, with method), or `[argued]` (reasoning only). Tags
   record provenance. A valid counterexample refutes a universal claim
   whatever its tag. Rank empirical support by relevance, validity,
   uncertainty, and scope; do not promote an argument merely because a
   source or measurement is cited. (Amended in round 2, Codex's
   wording.)
3. **Do not defer.** Neither reviewer yields to the other, to the
   existing text, or to Hannah's ideas without a reason on the page.
4. **Propose, do not only object.** Every objection ends with what to do
   instead, including "delete it".
5. **Say what would change your mind.** For each disputed point, name
   the experiment or evidence that would settle it.
6. **Subagents are allowed** for research and experiments. Each reviewer
   runs its own. Cross-model results are worth more than one model's.
7. **Files only in `debate/`.** Do not edit the spec, commit, or push
   during the debate. The spec is rewritten once, after convergence.
8. This machine is Windows. Use PowerShell, not Bash (Bash opens WSL).
9. **Converge on defaults.** Where evidence is insufficient for the
   current gate, choose the simplest safe, reversible default that meets
   its stated guarantees; defer features the gate does not need. Count
   semantic judgments, author choices, illegal combinations, and
   migration cost rather than keywords. Record uncertainty and a
   concrete reopening trigger; no default silently removes an advertised
   guarantee. (Added in round 2, Codex's wording.)

## Agenda

Each reviewer covers every item, and adds items the list misses.

| # | Area | Questions to start from |
| --- | --- | --- |
| A | Goals and author model | Is "a language for LLM authors" well posed? A new language has no training corpus: is that fatal, and would a checked subset of an existing language serve better? Which goals are worth their cost? |
| B | Programs | Content-addressed store, cells, claim/witness/evidence, text as projection, roles and separation of powers, admission. Worth it, or ceremony? |
| C | Syntax | The line surface as drafted. Hannah's alternative: a brace-less, keyword-delimited shape, `<opener> <arguments> <variadic arguments> <closer>`, such as `if c then: … else: … endif` and `fn name takes: … returns: … requires: … do: … endfn`. Token cost, parse simplicity, model error rates. |
| D | Types | Nominal types, refinements, totality, no run-time failure. Are affine, linear, or dependent types (or others) suitable? Labels and information flow. |
| E | Claims and evidence | `means`/`ensures`, obligations, grades, strength checks, debts, cost bounds. Solver dependence. Does this make LLM output better or only slower? |
| F | Effects and capabilities | Ports and intents against effect rows, handlers, and object capabilities. Is this the right capability security model? |
| G | Execution model | Step machines and a journal. Is the actor model the right approach? `proc` against `machine`. Logical time. |
| H | Concurrency and parallelism | Hannah wants it fearless and easy. What does the design give today, and what is missing (data parallelism, structured concurrency, back-pressure)? |
| I | Runtime | Memory, scheduling, drivers, replay, simulation, exploration. Cost of journaling everything. |
| J | Compiler | Kernel, core language, trusted base, readings, backends, translation validation. Is the trusted base really small? |
| K | Graphics and native UI | No webview. What would native graphics and UI look like under ports, machines, and determinism? |
| L | Tooling | The exact set of tools to ship for agent authors. |
| M | Concept count | Which cell kinds, keywords, and rules collapse into one? What is the smallest surface that keeps the guarantees? |
| N | Viability | What is the smallest version worth building, and what must be cut to get there? |

## Rounds

| Round | Each reviewer writes | File |
| --- | --- | --- |
| 1 Blind review | A full review of every agenda item, written **without reading the other's round 1** | `r1-claude.md`, `r1-codex.md` |
| 2 Cross-examination | A response to the other's round 1: concede, rebut, or counter-propose, point by point. Lists the disputes that need an experiment | `r2-claude.md`, `r2-codex.md` |
| 3 Experiments | Runs the agreed experiments with its own subagents. Methods and raw results are kept | `r3-claude.md`, `r3-codex.md`, `experiments/` |
| 4 Convergence | A final position per item. Claude merges both into `ledger.md`; Codex reviews it and records dissent | `r4-*.md`, `ledger.md` |

More rounds are added while disputes remain that evidence could settle.

## Ledger

`ledger.md` has one row per decision: the question, the verdict
(**agreed**, **default**, **deferred**, or **open**), the support tag, and
each reviewer's position when they differ. Claude edits it. Codex
proposes changes in its round files.

## Messages

Signal with `hc send`: Codex reports to `t13`, Claude to `t1c`. Send a
file path and one line, not the content.
