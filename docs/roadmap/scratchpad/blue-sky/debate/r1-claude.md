# Round 1: Claude's blind review

Status: working notes; not normative

Written without reading `r1-codex.md`. Tags: `[cited]` points at a file in
`research/` (those reports took their numbers from search summaries, so
every cited number is **unverified** until someone opens the paper),
`[measured]` is not used yet, `[argued]` is reasoning only.

## Summary of positions

1. The draft spends most of its novelty where the evidence is weakest
   (syntax, the store) and treats its strongest ideas (effects as data,
   port models, simulation, graded evidence) as later layers.
2. The thesis must be stated so it can fail: *agents ship fewer defects
   per task in Vibra than in a mainstream language with tests, at equal
   budget*. No benchmark for that exists in the plan. Build it first.
3. The concept count can drop from 13 cell kinds to 6 without losing a
   guarantee. Most of the cuts are collapses, not deletions.
4. Proof is an upgrade, not the entry price. The default grade should be
   `tested`, by differential testing against the reference meaning.

## A. Goals and author model

**A1. The no-corpus problem is real and the draft never prices it.**
Python scores more than three times low-resource languages on MultiPL-E;
for large models, in-context material recovers more than fine-tuning
`[cited: syntax-evidence]`. Low-resource languages gain the most from
tool feedback and typed-hole context `[cited: tooling-evidence]`.
So a new language is not fatal, but only if the surface is small enough
to fit in a prompt and the checker teaches. *Instead:* make "the whole
language fits in a 2,000-token card" a hard requirement, and ship a
checked example corpus that `vibra find` retrieves from. The corpus is
the substitute for training data.

**A2. Would a checked subset of an existing language serve better?**
The honest competitor is Dafny or Verus plus a durable-execution runtime
such as Restate. With the spec given, models verify 82% of Dafny tasks,
44% of Verus, 27% of Lean `[cited: verification-evidence]`. Dafny has a
corpus and Vibra has none. What a subset cannot do is *remove* things:
models drift to the full language, and escape hatches stay reachable.
Vibra's value is in what it forbids and in integrating claims, effects,
and replay with no way around them `[argued]`. I keep the new language,
but this is the point most worth attacking in round 2. *What would
change my mind:* the benchmark in N showing no defect reduction against
Dafny-or-TypeScript-plus-tests.

**A3. The mission is too wide.** "Libraries, CLIs, services, workflows",
plus proofs, plus cost bounds, plus information flow, plus notations,
plus choreographies. *Instead:* one sentence. "A language in which an
agent cannot finish a task by weakening the check, and in which every
effect can be replayed and simulated."

**A4. The author table is asserted, not measured.** "Dependent on
familiar notation" and "good at formal detail" are the two rows the
syntax and the proof story rest on. Neither has a number behind it.
*Instead:* each row becomes a hypothesis with an experiment (see C and
N).

**A5. Human review is a goal the syntax ignores.** The charter says
people review claims and manifests. Then claims must read well to
people. `(int.eq (cents.sum shares) total.value)` does not `[argued]`.

## B. Programs

**B1. Demote the store to a cache.** No evidence was found that
hash-addressed tree stores help agents, no independent report of agents
using Unison, and the only measured structured-edit result keeps text
files and git `[cited: tooling-evidence]`. The draft already concedes
this in practice: agents edit `src/` and run `stage`. That gives two
sources of truth and a sync command. *Instead:* files in git are the
source. Hashing stays, as a derived index: claim id, witness id,
evidence keyed by both, incremental checking. A revision is a commit.
We keep every stated benefit and delete `stage`, `sync`, `discard`,
`publish`, and the overlay.

**B2. Keep separate claim and witness hashes. It is the best idea in
the chapter.** GPT-5 gamed 54% of conflicting SWE-bench tasks; read-only
tests cut that to near zero `[cited: tooling-evidence]`. Spec gaming in
verified code is documented too: tautological and trivial postconditions
`[cited: verification-evidence]`. A tool that classifies any diff as
"witness only" or "claim changed" and gates the second is cheap and
directly supported.

**B3. Five roles are process, not language.** *Instead:* one rule. A
change to a signed claim needs a signature. Who signs is repository
policy.

**B4. Cut admission rule 7 (no duplicate `means`) and the closure
budget as hard rules.** Both are lints. The 2,000-node number is made
up.

**B5. A real payoff of content addressing that the draft misses.**
Running instances can pin the hash of the code they started on. That
answers the hardest known problem of durable execution, changing code
under a live journal `[cited: execution-evidence]`. See G3.

## C. Syntax

**C1. No published evidence decides any shape question.** Nothing found
on indentation against braces against `end`, on closer errors, on
S-expressions against infix, on labeled arguments, or on redundancy
`[cited: syntax-evidence]`. Every syntax rule in the draft is
`[argued]`. This must be measured here, in round 3.

**C2. The draft has three block mechanisms.** Indentation inside,
`end name` outside, parentheses in leaves. That is not one shape.

**C3. Hannah's shape is the better skeleton, with one gap.**
`head label: value label: value … endhead` is one rule for every
construct. Three arguments for it `[argued]`:

- Sections are labeled arguments. `takes:`, `returns:`, `then:`,
  `else:` are the same thing as `left:` in a call. Declarations,
  statements, and calls share one shape, and `:` gets one meaning.
- With closers and no significant indentation the grammar is
  context-free. That matters: type- and grammar-constrained decoding is
  the best-evidenced syntax-level intervention, cutting compile errors
  by more than half `[cited: syntax-evidence]`, and providers accept
  context-free grammars, not `INDENT`/`DEDENT`.
- A typed closer (`endif`) turns a wrong nesting into a local error. A
  `)` cannot.

The gap is expressions. `f a g b c` is ambiguous without brackets, so
"brace-less" cannot reach the leaves. Calls need `f(a, b)` or `(f a b)`.

**C4. Prefix arithmetic is probably a mistake.** `(int.lt a b)` and
`(int.sub a b)` invite argument-order errors and are the least familiar
form in any corpus `[argued]`. The precedence hazard has a known fix:
allow infix but require parentheses whenever two different operators
meet, as Pony does. *What would settle it:* error rates on
arithmetic-heavy tasks, prefix against infix.

**C5. Unmeasured costs the draft imposes:** a type on every `let`, the
depth limit of 3, no anonymous functions (a five-line nested `fn` for
every `map`). Each may be right. Each needs a number.

**C6. Token cost.** Closers such as `endfn` cost tokens that a dedent
does not. Whitespace itself is about 25% of tokens in ordinary code
`[cited: syntax-evidence]`, so a format that does not need indentation
could be stored compact and printed indented. Count it, per tokenizer.

**C7. Cut the table surface and notations.** Both add grammars no model
has seen, and padded tables waste tokens. A table can be a read-only
rendering.

**Round 3 experiment (proposal).** Four surfaces: (1) today's
S-expressions, (2) the draft line surface, (3) keyword sections with
closers and `f(a, b)` calls, (4) a mainstream control with braces and
infix. Same eight tasks, one-page card each, fresh subagents from both
model families, a real parser per surface, tokens counted. Secondary
arms: prefix against infix, typed `let` against inferred.

## D. Types

**D1. Refinements by `wraps … where`: keep.** It is the useful fraction
of dependent types, and Flux reports annotation overhead near zero on
light benchmarks `[cited: verification-evidence]`.

**D2. Full dependent types: reject.** Lean is at 27% where Dafny is at
82%, and the only dependent-type data point is Idris at 22 of 56
`[cited: verification-evidence]`. Proof terms cost more than solver
obligations.

**D3. Affine and linear types: not in the core.** `own` exists to
*guarantee* in-place update. With immutable values and reference
counting, reuse already happens when the count is one, as in Koka,
Lean, and Roc `[argued]`. `own` is an optimization promise shown as a
type feature. `resource` has no use in the sample, and linearity cannot
survive a crash anyway. Rust needs ownership because it has mutation
and no collector. Vibra has neither problem. *Instead:* cut both until
a case forces them.

**D4. Collapse `label` into opaque types.** A wrapped type whose reader
is private to its module already restricts where the data goes. The
lifting rule is an implicit conversion, which the charter forbids, and
it ignores timing channels. Cut `label` and `releases`.

**D5. Totality: keep, but infer structural measures.** With no loops,
every iteration is recursion, so a mandatory `decreases` is a tax on
every helper. Infer it for structural recursion; ask only when
inference fails. No evidence was found on measure-writing burden
`[cited: verification-evidence]`, so measure it.

**D6. "Pure code cannot fail": keep both forms, default to checked.**
The draft gives a proved form and an `option` form. Agents should reach
for the `option` form first and upgrade when a proof is cheap.

**D7. One sequence type.** `list` and `array` differ only in
representation. Let the runtime choose. `uncertain` is `option` with
other names; keep it only if the distinct name measurably helps.

## E. Claims and evidence

**E1. The realistic default is `tested`.** Writing spec and proof
together succeeds 4 to 5% on Verina and at most 1 of 161 on CLEVER with
2025 models `[cited: verification-evidence]`. Solvers are unstable on up
to 5% of queries `[cited: verification-evidence]`. *Instead:* layer 1
ships with no solver. Evidence comes from examples, generated inputs,
and differential testing of `body` against `means`. Solver grades are a
later upgrade that changes no source text. This is what grades are for.

**E2. Determinacy forces specs to be programs.** A claim that must
admit one result is, in practice, a reference implementation. So most
functions will be `means` with no `body`, and the claim is the code.
*Instead:* make that the plain case. A function has a definition. A
second, faster definition is the rare case and gets the extra keyword.

**E3. Strength checks: keep mutation, keep examples, demote
determinacy.** Mutation checks on specs exist but only on small samples
`[cited: verification-evidence]`. Determinacy by solver is undecidable
in general; report it as a grade, not a gate.

**E4. Cost bounds must not be load-bearing.** The scheduler depends on
every handler having a bound. No evidence was found that agents can
write them. *Instead:* meter fuel at run time. Cost claims become
optional.

**E5. Collapse grades to four.** `proved` and `solved` differ only in
method, which the record already holds. Six debt kinds become one debt
with a tag.

**E6. `lemma` is a `fn`.** A lemma is a function with no result whose
`ensures` is the statement. `apply` is a call. One kind and one keyword
fewer.

## F. Effects and capabilities

**F1. Effects as data to ports: keep. It is the core of the design.**
Deterministic simulation works exactly when all I/O crosses one
interface `[cited: execution-evidence]`, and here the language enforces
that.

**F2. `uses` is not capability security.** It names global port
declarations. There is no attenuation, no second instance, no "this
directory only". It is the old effect ceiling renamed. *Instead:*
capabilities are values. A procedure can address only what it was
handed as a parameter. `uses` disappears into `takes`, and narrowing a
capability is an ordinary function.

**F3. Ports, addresses, and interfaces are one concept.** A machine
address "confers the ability to send and nothing else". That is a
capability. A port is the message interface of something you can send
to. A driver is such a thing implemented by the host. A model is the
same interface implemented in Vibra. *Instead:* `port` becomes an
`interface`, drivers and models become `impl`s, and a target chooses the
`impl`. That removes `port`, `model`, `uses`, and `bind`.

**F4. A model's nondeterminism is a port.** A model returns a list of
answers because machines cannot choose. Give it a `choose` capability
that simulation controls, and a model is an ordinary machine.

**F5. The cost of "every port ships a model" is real.** Closed effect
sets pushed all new capabilities through platform authors in Elm and
Roc `[cited: execution-evidence, from memory]`. *Instead:* allow a
recorded-trace model: replay of captured outcomes, at grade `tested`.

**F6. `once`, `repeatable`, and `uncertain`: keep.** This is the one
place the type system forces a crash case to be written.

## G. Execution model

**G1. Journaling must be a target policy.** A durable write per step
suits a payment and ruins a game loop or a CLI. A 60 Hz frame tick
floods a journal `[cited: execution-evidence]`. *Instead:* three
levels chosen by the target: none, in-memory ring, durable. Semantics
are identical.

**G2. A state hash per commit is O(state).** Say how: persistent
structures with cached hashes, or hash at snapshots only.

**G3. Upgrades are under-specified, and the draft's rule is wrong.**
"A new witness takes effect at the next step" cannot hold for a
procedure whose suspension points moved. Every durable-execution system
hits this `[cited: execution-evidence]`. *Instead:* an instance pins
its code hash and finishes on it. Only long-lived machines migrate, by
an explicit function.

**G4. `machine` is a recursive `proc` that awaits its inbox.** The
draft already turned loops into recursive functions whose `requires`
are the invariants. Do the same here: state becomes the parameter,
`invariant` becomes `requires`, the handler grid becomes two nested
exhaustive `match`es. One construct. The grid stays as a rendering.
*Risk:* a blocking `await` inside such a procedure stalls its inbox,
which is the request-reply deadlock of actors
`[cited: execution-evidence]`. Keep `emit` for sends that must not
block.

**G5. Is the actor model right?** Half right. Its known failures are
unbounded mailboxes, request-reply deadlock, reentrancy at `await`, and
orphans `[cited: execution-evidence]`. The draft already contains the
better default: procedures awaiting composite intents are structured
concurrency. *Instead:* structured concurrency is the default. Addressed
machines are for long-lived state, always owned by a parent, with a
bounded inbox whose `send` outcome includes `full`.

**G6. Logical time: shrink.** The journal order is the order. Keep
`now` as the event's instant. Drop the tag-ordering rules until a
distributed runtime needs them.

## H. Concurrency and parallelism

**H1. Pure, total, immutable code is already fearless.** A transform
can run in parallel with no change in meaning. So parallelism needs no
syntax. It is a property of library transforms and a choice of the
runtime `[argued]`.

**H2. Laws earn their keep here.** A parallel fold is correct exactly
when the combining function is associative. An `impl` that proves that
law licenses the parallel version. `f64` addition is not associative, so
it is not parallelized, and determinism holds.

**H3. Expect less than "automatic".** Real speedups need regular,
array-shaped work `[cited: execution-evidence]`. Promise parallel
transforms over arrays, not parallel everything.

**H4. The backend constrains this.** WebAssembly threads are weak. One
instance per machine gives actor parallelism cheaply; data parallelism
needs shared memory or a native backend. Decide before promising.

## I. Runtime

- **Memory.** A region per step, with the state promoted out, fits the
  model exactly. Keep reference counting for reuse.
- **Driver conformance** by running the model beside production is
  expensive. Sample it.
- **Simulation** is the best-supported feature in the whole design
  `[cited: execution-evidence]`. Move it earlier.
- **Exploration** will be bounded only. Say so, and keep it out of the
  first releases.

## J. Compiler

**J1. The trusted base is larger than claimed.** The kernel must check
types, exhaustiveness, totality, and effects, *and generate verification
conditions*. A bug there is unsound. Certificate checking for solver
proofs is a project of its own. *Instead:* reserve `proved` for what the
kernel computes itself. Drop the second kernel.

**J2. "Translation validation" by running samples is differential
testing.** Call it that.

**J3. Keep three readings, move five to an ideas file.** Concrete,
symbolic, generative. The rest is speculation in a specification.

**J4. Cut notations and protocols from the specification.**

## K. Graphics and native UI

**K1. The model fits.** A machine holds state, a pure `view` returns a
scene as data, the scene goes to a `window` capability, and input
arrives as events. This is the Elm architecture.

**K2. The payoff is for agents.** An agent cannot see a screen. It can
read a scene tree and replay input. UI tests without pixels, by
simulation, is a real advantage `[argued]`.

**K3. Three problems to solve first.** Frame-rate events flood a
journal (G1). Animation must be declared in the scene and run by the
driver, not stepped at 60 Hz. Textures and buffers are stateful handles,
uploaded by intent.

**K4. Do not build a toolkit.** Text shaping, input methods, and
accessibility are years of work. Specify the scene protocol, and write
the first driver over an existing native stack. Not in the first
releases. No evidence was found that models write UI better in any one
paradigm `[cited: execution-evidence]`.

## L. Tooling

Ship, in this order:

1. `check`: parse, types, obligations, with complete diagnostics and
   confirmed counterexamples. A validated edit loop was the largest
   ablation in SWE-agent `[cited: tooling-evidence]`.
2. `fmt` that accepts sloppy input and repairs it, so a spacing mistake
   never costs a turn.
3. `eval` and `test`, on the reference evaluator.
4. `show` and `find`, including find-an-example.
5. `run`, `simulate`, `replay`, `ask`.
6. A generated skill card, error catalogue, and example corpus.

Do not ship: an MCP server (command-line tools cost 4 to 32 times fewer
tokens `[cited: tooling-evidence]`), the staging commands (B1), or
"moves". Gate 1, "unsampleable", needs control of the decoder, which
users of hosted models do not have. Publish the grammar as a
context-free grammar so that those who do have it can use it.

## M. Concept count

| Today | Becomes | Why |
| --- | --- | --- |
| `type`, `label` | `type` | Opaque types (D4) |
| `fn`, `lemma` | `fn` | A lemma is a function (E6) |
| `interface`, `port` | `interface` | A port is a message interface (F3) |
| `impl`, `model`, driver binding | `impl` | F3, F4 |
| `proc`, `machine` | `proc` | G4 |
| `target` | `target` | Kept |
| `notation`, `protocol` | Cut | J4 |

Thirteen kinds become six. Keywords that go with them: `uses`, `bind`,
`model`, `label`, `releases`, `lemma`, `apply`, `pass`, `machine`, `on`,
`accepts`, `starts`, `own`, `resource`, `invariant` (as `requires`),
`impossible` (as `assert false`).

## N. Viability

**N1. The smallest version worth building:** pure functions with
examples and tested properties; procedures and capabilities; models;
simulation; journal and replay on the reference evaluator. No solver, no
cost bounds, no ownership, no labels, no notations, no store.

**N2. The benchmark comes first.** Twenty realistic tasks with hidden
tests. Agents solve them in Vibra from the card, and in TypeScript and
Dafny as controls. Measure defects shipped per task at equal budget. If
Vibra does not win, the design is wrong somewhere and no roadmap should
proceed.

## Items the agenda missed

- **O. Libraries.** A general-purpose language starts with none.
  *Proposal:* drivers as WebAssembly components reuse an ecosystem at
  once; pure libraries are ported by agents under differential testing
  against the original.
- **P. Human reviewability** of claims and manifests (A5).
- **Q. Evaluation plan** (N2), as a standing part of the specification.
