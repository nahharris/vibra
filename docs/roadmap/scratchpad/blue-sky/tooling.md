# Tooling: edits, inspection, and evaluation

Status: design direction; not normative

Vibra plans three kinds of agent tooling: structured edits, structured
inspection, and an interactive evaluation session. This note asks which of
them earns its cost, in what shape, and in what order. It belongs to the
[blue-sky rethink](README.md). It does not change the
[v0.0.2 plan](../../v0.0.2/README.md), which already schedules all three;
it argues about emphasis within it.

## Short answer

| Investment | Verdict | Reason |
| --- | --- | --- |
| Dynamic evaluation | Invest the most | Its benefit holds for the strongest models, and Vibra's semantics make it cheap and safe |
| Structured inspection | Invest narrowly | Build only the facts a text search cannot compute, and deliver them without being asked |
| Structured edits | Invest in transactions, not in edit formats | Models edit text well. What they cannot do is keep a multi-part change coherent |

One rule sits under all three: the tool that helps is the one the agent
does not have to decide to call.

## What the evidence says about agent tools

**Interfaces mattered most for weaker models.** In 2024 a purpose-built
interface took one model from [11.0% to 18.0%][sweagent] on SWE-bench Lite
over a plain shell. Today a loop with [nothing but a shell][mini] scores
above 74% on SWE-bench Verified, and one team reports that
[removing 80% of its agent's tools][vercel] raised success from 80% to
100% on five test queries while using 37% fewer tokens. A tool that
compensates for a weakness the next model does not have has a short life.

**Agents skip optional tools.** A [study of language-server tools][lsp]
found that when finding a named symbol, agents free to choose reached for
the semantic tool 0% to 6% of the time and used text search instead.

**Tool catalogs cost context.** Presenting tools as code to call instead of
schemas to load cut one workload from
[150,000 tokens to 2,000][codemcp]. Agents that act by writing code have
also shown [up to 20% higher success][codeact] than agents that act
through JSON calls.

**Feedback pays at the moment of action, with a limit.** Rejecting a
syntactically broken edit on the spot was [worth 3 points][sweagent] in the
same 2024 study. But [RefactorBench][refactor] found that rejecting every
edit that introduces an error blocks real refactorings, which have to pass
through broken states.

**Too much output hurts.** Showing a whole file did worse than a 100-line
window, and paging through raw search hits did worse than no search tool
at all ([same study][sweagent]).

Four design rules follow.

1. Build what only the checker knows. Leave what text search already does.
2. Put facts in the results of actions the agent already takes, not behind
   a query it has to think of.
3. Keep the surface small, bounded, and textual.
4. Check at the boundary of a change, not at every step inside it.

## Structured edits

### Evidence

- Edit format once decided outcomes: one model went from
  [6.7% to 68.3%][harness] when the format changed. Later measurements
  find that for current models
  [edit format is not the bottleneck][nwyin].
- In a [29-task comparison][astedit], edits that target a syntax node
  and whole-file rewrites were the two formats with no format failures.
  Unified diffs had 31.
- The Clojure tooling that added structural editing for agents has
  [moved it to maintenance][cljmcp], because frontier models now edit
  S-expressions reliably with ordinary text tools.
- Multi-part changes are where agents fail.
  [RefactorBench][refactor] reports 22% success against 87% for a human,
  and no success on any task touching more than six files. The failure is
  losing track of what has been done. Showing the agent a computed summary
  of its own progress raised success by 43.9%.
- For multi-file renames, [text search solved every case][lsp] while
  reference lookup alone failed most of them. A rename also has to touch
  comments and strings, which semantic references leave out.

### Verdict

Do not invest in a fine-grained patch language or in addressing individual
expressions as a way to raise accuracy. The most robust edit is replacing
one whole declaration with new text. Vibra's declarations are small, and
cells in the rethink are smaller. That is where the v0.0.2 plan starts,
and it can stop there for longer than planned.

Invest in what makes a change coherent:

| Capability | What it does | Why it earns its cost |
| --- | --- | --- |
| Staged transactions | Collect many replacements, allow broken states in between, check once at publish | Refactorings need intermediate errors. Per-edit rejection blocks them |
| A computed worklist | After any staged change, list every dependent it broke and why | It replaces the state tracking that agents demonstrably lose |
| Stale rejection | Refuse a plan built on an old revision | Already specified. Keep it |
| Global operations | Rename, change a signature, move a declaration, each as one checked step | Their correctness is a whole-program property |

Two notes on that table. Under hash identity, as in
[Unison][unison], a rename edits no dependent at all, so the hardest global
operation disappears by design. And the plan's open question, whether a
plan may improve an invalid draft, has an answer in the evidence: it must.

A staged change might look like this. The command and its output are a
sketch, not a specified interface:

```text
$ vibra stage split-cents < witness.vib
staged   split-cents             revision r7 + 1 draft
check    ok
broken   1 of 2 dependents
  invoice.total   still satisfied
  report.line     example 2 fails: expected (34 33 33), got (33 33 34)
```

The agent did not ask what it broke. The answer came with the result.

## Structured inspection

### Evidence

- Semantic lookup is [more precise and not cheaper][lsp]. Finding
  references, it reported no false call sites against 24% for text search,
  with the same recall, at about 19% more tokens. It saved tokens only for
  the weakest model.
- A [50-task comparison][grepenough] found answer quality tied across
  text search, a language server, and structural navigation. They differed
  in context consumed: structural navigation used about half the tokens of
  text search, and the gap grew to 2.8 times on the hardest tasks.
- An [informal comparison][koppert] found a language server made agents
  three to four times faster in a 118,000-file repository and made no
  difference in a 61-file one.
- Models invent calls to rarely seen APIs. One model produced
  [valid calls only 38.58% of the time][cloudapi] for low-frequency APIs.
  In the same study, adding an API's specification raised valid calls
  from 41.80% to 86.82%. Adding its name or prose description reached
  about 53%. Retrieving the wrong documentation made common APIs worse.
- For filling a typed hole, [type definitions were the most useful
  context][chatlsp] a model could be given.
- Lean's agent tooling includes a local declaration search described as a
  way [to confirm declarations exist][leanmcp] and prevent invented names.

Every Vibra API is a low-frequency API. No model has seen the standard
library.

### Verdict

Do not rebuild text navigation. Vibra has one canonical spelling, no
wildcard imports, and no re-exports, so a definition is one search away.
Keep a text projection of the store on disk so the search habits agents
already have keep working.

Build the queries only the checker can answer:

| Query | Answer | Replaces |
| --- | --- | --- |
| What fits here? | Expected type at a position, with the in-scope values and members that have it | Guessing a member name |
| What is this? | Signature, effects, and one checked example of a callee | Reading its body |
| Why this effect? | The call chain that makes a function perform an effect | Tracing by hand |
| What breaks? | The dependents a change would invalidate | A project-wide search and a guess |
| Does this exist? | Declarations matching a type or a claim | Writing a duplicate |
| What is unproven? | Evidence grade and open debts of a cell and its dependencies | Trusting a green check |

Then deliver them without a query. Each diagnostic carries the facts needed
to fix it: the expected type, the candidates in scope, the signature of the
callee. Each staged change carries its worklist. Each hole carries its
computed context. A query the agent must think to make is, on the evidence,
a query it mostly will not make.

Documentation follows the same rule. A signature and a checked example are
the documentation. Prose does little, and anything not about the position
in question is noise that can make the answer worse.

## Dynamic evaluation

### Evidence

- The Clojure tooling that retired its editing tools kept one thing. Its
  author writes that [the part that still matters is the REPL][cljmcp].
- Access to a debugger took one model from
  [37.2% to 48.4%][debuggym] on SWE-bench Lite. The authors add that
  agents use debuggers poorly, and attribute it to how little interactive
  debugging appears in training data.
- [Debug2Fix][debug2fix] wraps the debugger in a subagent that answers
  questions about program state, and reports 60.2% to 73.1% on a Java
  bug benchmark.
- Every serious AI prover works through an interactive session.
  [Pantograph][pantograph] exposes proof search as goals an agent can
  branch on, and turns each gap in a draft into a goal. Lean's agent tools
  include one that [tries several tactics at a position][leanmcp] and
  returns the resulting state for each.
- [Hazel][hazellive] evaluates programs that still contain holes.
  Evaluation proceeds around each hole and records what reached it, and a
  hole can be filled without restarting.

### Why Vibra is well placed

A REPL in most languages is a liability: it runs arbitrary code with real
effects, and its state drifts from the source. Vibra's semantics remove
both problems.

- Pure code cannot touch the world, so evaluating it needs no sandbox.
  With totality and a cost bound it also needs no timeout.
- Evaluation is deterministic, so a result can be cached by hash and
  cited as evidence.
- Effects are data, so effectful code runs against a simulated world.
- Sessions pin a checked revision and stage drafts in an overlay, as the
  v0.0.2 plan already specifies, so the session cannot drift.

### What to build

In order of value for effort:

1. **Evaluate an expression against a pinned revision.** This is the first
   step of the v0.0.2 session plan.
2. **Promote a result to a checked example.** One step turns "I tried it
   and got this" into evidence attached to the declaration. Exploration
   stops evaporating when the session closes.
3. **Evaluate with holes.** Run unfinished code and see which holes the
   result is waiting on.
4. **Try several candidates at one position.** Evaluate each, return each
   outcome, let the agent pick.
5. **Search for inputs.** "Find an input where these two disagree." "Find
   an input that breaks this claim." This is the distinguishing-input
   dialogue of the main note, as a command.
6. **Ask questions of a journal.** When execution is a fold over recorded
   events, the state at any step is a value. "What was the state at step
   42?", "At which step does this invariant first fail?", and "What if
   event 43 had been a timeout?" are queries with answers, not stepping
   sessions.

The sixth item is the alternative to a stepping debugger. Agents are poor
at driving breakpoints and good at asking a question and reading the
answer. A journal makes debugging question-shaped. The
[debugger proposal](../agent-tooling/debugger.md) remains the right
interface for people in editors.

## How to expose them

- **Few verbs.** Show, find, evaluate, stage, publish. A catalog of thirty
  tools costs context before any work starts.
- **Text in, text out, in the language.** Results as Vibra data read like
  the code beside them. Reading JSON is harmless. Writing code inside a
  JSON string [measurably is not][aider], so replacement source should
  travel as plain forms on standard input or in a file, not as a string
  field in a tool call.
- **Command line first, MCP as a thin adapter.** The plan already requires
  equivalent CLI evidence. The evidence above says to treat the command
  line as the primary surface for agents and MCP as one client of it.
- **Bounded output with a stated remainder.** "12 more; narrow the query"
  beats twelve more pages.

## Relation to the v0.0.2 plan

The items below come from milestones [M2](../../v0.0.2/m2/README.md) and
[M3](../../v0.0.2/m3/README.md).

| Planned item | This note's reading |
| --- | --- |
| M2: queries exposed through CLI and read-only MCP | Keep. Add the rule that diagnostics and edit results carry the same facts unasked |
| M2: replace one declaration from Vibra forms | The core edit. Sufficient for a long time |
| M2: replace a selected expression | Lowest value. Whole-declaration replacement covers it |
| M2: rename and organize imports | Keep as global operations |
| M2: may a plan improve an invalid draft? | Yes. Check at publish |
| M3: session over a pinned revision with an overlay | The highest-value item in the release. Consider it before the wider edit work |
| M3: run selected tests in the session | Keep. Add promotion of an evaluation to an example |
| M3: execution permission even for pure expressions | Right for today's language. Totality and cost bounds would let pure evaluation go without it |
| MCP tools named after CLI concepts | Keep the count small, and avoid code inside JSON strings |

## Risks

- **Models improve.** A tool justified by today's failure may be dead
  weight later. The Clojure editing tools are the example. The selection
  rule above, build only what the checker knows, is meant to pick tools
  that survive this.
- **The evidence is about files.** Almost every study here is Python or
  TypeScript in a file tree. None measures a content-addressed store,
  where the trade-offs may differ.
- **Small samples.** The tool-removal report is five queries, the
  node-edit comparison is 29 tasks, and the language-server study calls
  itself preliminary.
- **Pushed facts cost context.** Unasked information that misses is worse
  than none. Each pushed fact needs a bound and a reason to be there.
- **Evaluation can replace thinking.** An agent that can try things may
  stop stating what should be true. Promotion to an example must feed the
  claim, not stand in for it.

## Experiments

| # | Question | Measure | Abandon if |
| --- | --- | --- | --- |
| T1 | Does evaluation help on Vibra tasks? | From a failing test, pass rate and turns with and without `eval` | No difference in pass rate or turns |
| T2 | Pushed or pulled facts? | Same facts in diagnostics against behind a query tool. One-turn fix rate and tool calls | Pulled does as well |
| T3 | Which edit unit? | Whole declaration, addressed node, and text replace. Format failures and tokens | Whole declaration is not the most reliable |
| T4 | Do transactions with a worklist help? | Signature change across many callers, staged against one checked edit at a time | No gain in completed changes |
| T5 | What should documentation be? | Signature only, signature with example, and prose. Rate of valid standard library calls | Prose does as well as signatures |
| T6 | Command line or MCP? | Same operations as text commands and as JSON tool calls. Pass rate and tokens | No difference |
| T7 | Questions or stepping? | Journal queries against a stepping debugger on the same failures | Stepping does as well |

T1, T2, and T5 can run on the current toolchain. T5 needs only the skill
and the checker.

## Sources

- [SWE-agent][sweagent], [mini-SWE-agent][mini], and
  [removing agent tools][vercel]
- [Language-server tools and token cost][lsp],
  [is grep enough][grepenough], and an [informal comparison][koppert]
- [Code execution with MCP][codemcp] and [CodeAct][codeact]
- [RefactorBench][refactor]
- [The harness problem][harness], [a replication][nwyin], and
  [node-targeted edits][astedit]
- [clojure-mcp][cljmcp]
- [Unison][unison]
- [API hallucination and documentation][cloudapi]
- [Typed holes as context][chatlsp]
- [lean-lsp-mcp][leanmcp] and [Pantograph][pantograph]
- [debug-gym][debuggym] and [Debug2Fix][debug2fix]
- [Live functional programming with typed holes][hazellive]
- [Code in JSON][aider]

[sweagent]: https://arxiv.org/abs/2405.15793
[mini]: https://github.com/SWE-agent/mini-swe-agent
[vercel]: https://vercel.com/blog/we-removed-80-percent-of-our-agents-tools
[lsp]: https://arxiv.org/html/2608.13568
[grepenough]: https://github.com/Entelligentsia/is-grep-enough
[koppert]: https://zackkoppert.com/posts/does-lsp-help-ai-coding-assistants/
[codemcp]: https://www.anthropic.com/research/code-execution-with-mcp
[codeact]: https://xwang.dev/blog/2024/codeact/
[refactor]: https://arxiv.org/html/2503.07832
[harness]: https://stencil.so/blog/the-harness-problem
[nwyin]: https://nwyin.com/hashline-vs-replace-edit-bench
[astedit]: https://www.geometric.so/research/ast-edits
[cljmcp]: https://www.github.com/bhauman/clojure-mcp
[unison]: https://www.unison-lang.org/docs/the-big-idea/
[cloudapi]: https://arxiv.org/html/2407.09726
[chatlsp]: https://dl.acm.org/doi/10.1145/3689728
[leanmcp]: https://github.com/oOo0oOo/lean-lsp-mcp/blob/main/docs/tools.md
[pantograph]: https://centaur.stanford.edu/blog/2025/05/03/pantograph.html
[debuggym]: https://simonwillison.net/2025/Mar/31/debug-gym/
[debug2fix]: https://arxiv.org/html/2602.18571v2
[hazellive]: https://arxiv.org/abs/1805.00155
[aider]: https://aider.chat/2024/08/14/code-in-json.html
