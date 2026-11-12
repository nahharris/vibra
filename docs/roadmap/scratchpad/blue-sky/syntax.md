# Syntax: is Lisp the right shape?

Status: design direction; not normative

Vibra uses a small S-expression syntax on the theory that a model can learn
it from a skill. This note tests that theory against published evidence and
against measurements of the repository, then proposes what the syntax could
become if nothing about it were fixed. It belongs to the
[blue-sky rethink](README.md) and shares its license to ignore the charter.
The sketches are illustrative.

## Short answer

The bet is half right.

- **Right:** models do learn an unfamiliar notation from a prompt when the
  language behaves like languages they know. Prefix application with no
  precedence is unambiguous, and nothing in the evidence argues for giving
  it up.
- **Wrong:** S-expressions are simple for a parser. For a model, simple
  means three other things: its habits point the right way, the distinctions
  that matter are visible where they apply, and nothing has to be counted.
  Vibra's syntax misses on all three.

Restriction is not the problem. Vibra restricts notation, which costs an
agent little to have more of, and leaves structure free: any nesting depth,
unnamed intermediate values, meaning carried by position. The proposal is to
invert that. Allow several checked notations over one canonical tree, and
make structure strict.

## What "Lisp syntax" bundles

"Lisp syntax" is seven separate decisions. They deserve separate verdicts.

| Decision | Effect on an agent | Verdict |
| --- | --- | --- |
| One tree encoding for code and data | Paths, hashes, queries, and edits work on one structure | Keep, as the stored form |
| Prefix application, no precedence | No operator table to recall; one reading per expression | Keep |
| Labelled attributes (`effects:`, `visibility:`) | Meaning named at the site | Keep, and extend |
| Meaning by position: flat name/type pairs, alternating match arms | One dropped element shifts every later one into the wrong role | Drop |
| Anonymous closers | Closing a form means counting, across tokens of uneven width | Drop past a small depth |
| Nesting as the only way to compose | Data flows inside-out, against the direction of generation | Drop |
| One shape for every category | Binder, type, pattern, access, and call look alike | Drop |

An eighth property comes along unasked: the text looks like Lisp, and Vibra
does not behave like Lisp. That is the subject of
[False friends](#false-friends).

## Evidence

### Learning from a skill works for near languages

Three results bound what a prompt can teach.

- [Anka][anka] is a pipeline language no model had seen. With only a prompt,
  Claude 3.5 Haiku parsed 99.9% of its attempts and scored 95.8% against
  91.2% for Python on the same tasks, and 100% against 60% on multi-step
  tasks. Its design rules are one canonical form per operation, a name for
  every intermediate result, explicit steps, and keywords over symbols. The
  study is one author, 100 tasks, and small models, and Python won the
  category labelled hard.
- [EsoLang-Bench][esolang] gives frontier models 80 problems they solve
  completely in Python. Across five esoteric languages the best
  configuration in the best language reaches 11.2%, and few-shot examples
  add 0.8 points, which is not significant.
- A [study of four other esoteric languages][esoicl] finds documentation and
  examples produce "some correct code" at rates far below mainstream
  languages. Feeding verified solutions back as examples lifts one language
  from 16.67% to 30.82%.

Anka is a familiar paradigm in new spelling. Brainfuck and Unlambda are new
paradigms. The prompt can carry spelling. It cannot carry a paradigm. A
[semantic parsing study][bogin] reaches the same conclusion from another
direction: replacing a Lisp-like DSL with Python took one compositional
split from 7.9% to 66.5%, Scala did about as well as JavaScript despite
being far rarer, and the authors conclude that resemblance to
general-purpose code matters more than prevalence in the training data.
[MultiPL-E][multipl] agrees that accuracy tracks popularity only loosely.

This matters for the whole rethink. Its premise says alien paradigms are
acceptable because the author is not human. The evidence says a model is
trained on human code and is no better at truly alien surfaces than people
are. The alien part of the design has to live in what the kernel checks and
how execution works. The page the agent writes should stay near what it
knows.

### False friends

When a familiar surface behaves in an unfamiliar way, models follow the
surface. [Counterfactual variants of familiar tasks][wu], including Python
with one-based indexing, degrade performance substantially and
consistently, even where a separate check shows the model understood the
changed rule.

Vibra's skill opens with this instruction:

> Vibra is not Lisp, Clojure, or Scheme. Do not carry over macros, quoting,
> `car`/`cdr`, `nil`, truthiness, `+`, `=`, or `cond`. None of them exist.

That sentence is the false-friend cost, paid on every task. The surface
calls up the one language family whose semantics Vibra shares least. Its
semantics are those of ML and Rust: nominal types, `option` and `result`,
exhaustive `match`, interfaces, no exceptions. Its spelling is that of a
dynamically typed, macro-based family.

Lisp chose S-expressions for a reason Vibra excludes. They are what makes
macros and code-as-data cheap. McCarthy meant them as the internal form and
planned a second notation for writing programs:

> The project of defining M-expressions precisely and compiling them or at
> least translating them into S-expressions was neither finalized nor
> explicitly abandoned. It just receded into the indefinite future.
> ([History of Lisp][mccarthy])

Vibra pays for an internal notation and, with macros excluded, collects none
of what it buys.

### Nesting and counting

Deep nesting is a known weak point, with a mechanical cause.

- Practitioners built [delimiter repair for Clojure agents][cljlight] to
  escape what its author calls the "Paren Edit Death Loop", where a model
  fails repeatedly to rebalance its own edit. The repair uses parinfer,
  which infers the closers from the indentation.
- The same author now reports that [current frontier models edit Clojure
  reliably][cljmcp] and has put the structural editing tools into
  maintenance. The problem shrinks as models improve. Smaller and cheaper
  models still have it; one [practitioner report][hn] found a nesting limit
  of five kept the damage down.
- [MoonBit][moonbit], designed alongside its own constrained sampler, chose
  "a flat and minimally nested structure" with mandatory signatures at the
  top level.
- Theory points the same way. Self-attention [handles hierarchical
  languages of bounded depth][yao], and one study found a transformer
  [solving bracket prediction by counting][bernardy] and not with a stack.

Two measurements from this repository, reproducible with a short script:

| Measurement | Result |
| --- | --- |
| Closers per token (`o200k_base` and `cl100k_base`) | One to four `)` are one token each. Five are `))` then `)))`. Six are `))))` then `))`. A following newline merges into the token |
| Nesting in 34 `.vib` files under `stdlib`, `examples`, and the skill (206 top-level forms, 2,001 lines) | Median depth 3, 90th percentile 6, maximum 10. 16% of forms reach depth 6. The longest closer run is 8 |

The first row is the mechanism. To close six forms a model emits two tokens
whose widths must sum to six. Balancing is arithmetic over tokens of uneven
width, not one token per form. Other model families tokenize differently,
so this is an example and not a universal fact.

The second row says today's code is mostly shallow. The exposure is in the
tail, and the tail is where the logic is.

### Format has a price

- Models asked to return code inside JSON [score lower than with plain
  fenced text][aider]. One model made no syntax errors either way and still
  scored lower.
- [A study of structured output][formattax] attributes most of the loss to
  asking for the format at all, before any decoder constraint applies.
- Grammar-constrained decoding guarantees well-formed output and
  [distorts the model's distribution][gad] while doing so.

Two consequences. A notation the model must fight costs reasoning, not only
syntax errors. And the moves of the
[refinement protocol](README.md#shift-3-authoring-is-refinement-not-text)
should be written in the language itself, as plain text, not as JSON tool
calls.

### Tokens matter a little

[SimPy][simpy] re-spells Python with the same syntax tree and a converter in
both directions. It saves 13.5% and 10.4% of tokens for two models with
accuracy maintained. Stripping formatting from input
[saves 24.6% of tokens with accuracy largely unchanged][format], but output
shrinks only 2.9% because models keep writing in the style they know.

The main note calls token-minimal syntax a trap. That needs sharpening.
Context is a scarce resource, since accuracy falls as it fills. The trap is
removing names and redundancy a checker can use. Removing tokens that carry
no information is fine.

### Prefix holds up

Transformers trained to translate word problems into arithmetic did better
emitting [prefix or postfix than infix][prefix]: 85.4% and 85.0% against
69.0%. These are small trained models, not general ones. The result still
runs against the assumption that infix is the natural choice. An
unambiguous notation with no precedence is worth keeping.

## Expressiveness

Syntax that can be translated away locally adds no expressive power in
[Felleisen's sense][felleisen]. Parentheses do not limit what Vibra can
say. Its real limits are semantic and deliberate.

What notation decides is where information lives: on the page, or in the
reader's memory. Every rule below is true of Vibra today, is stated in the
[skill](../../../../plugins/vibra/skills/vibra-language/SKILL.md), and is
invisible at the place it applies.

| Rule held in memory | What the page shows |
| --- | --- |
| In a pattern a bare name binds, while a literal or a module value such as `true` compares | Two lowercase symbols |
| `(p @x)` reads a field, `(pair 0)` projects, `(items 2u64)` looks up and returns an `option`, `(f x)` calls | Four applications |
| Parameters and `let` are flat name/value pairs | An even-length list |
| `match` arms alternate pattern and result | An even-length list |
| Attributes come in a fixed order | Labels in any order parse |

Each row is a rule a skill must teach and a model must recall under load,
and recall across a long context is what models do worst. A uniform syntax
does not remove these distinctions. It moves them off the page.

There is a second, larger cost. This function from `stdlib/src/std/str.vib`
decodes the lead byte of a UTF-8 sequence:

```vibra
(defn sequence (items (array u8) index u64 first u32) (option (tuple u32 u64))
  (if (within first 0u32 127u32)
    (option.some (tupleof first 1u64))
    (if (within first 194u32 223u32)
      (match (continuation items index 1u64 128u32 191u32)
        (option.some second)
          (option.some
            (tupleof (u32.add (u32.mul (u32.sub first 192u32) 64u32) second) 2u64))
        (option.none) (option.none))
      (if (within first 224u32 239u32)
        (three items index first)
        (if (within first 240u32 244u32) (four items index first) (option.none))))))
```

It is correct, and every constant in it was compiled by hand from a table
that the UTF-8 specification writes in four lines:

```text
0xxxxxxx
110xxxxx 10xxxxxx
1110xxxx 10xxxxxx 10xxxxxx
11110xxx 10xxxxxx 10xxxxxx 10xxxxxx
```

Write `4096` where `64` belongs and the program still type-checks. The
language has one notation, the domain has another, and the translation
between them happens in the author's head, where nothing checks it. This is
the sense in which a restrictive syntax costs expressiveness. The loss is
not sugar. It is notations whose structure a checker could have understood.

## Proposal: one tree, several surfaces

### The tree is the canonical form

Keep the S-expression as what the store holds, hashes, queries, and diffs.
It is the right shape for that job and it is already `.vibon`. Text becomes
a projection of the tree, as the main note proposes, and each projection
must round-trip: print then parse returns the same tree, as a law proved
once per projection.

This is how the precedents work. Wolfram expressions have one
[`FullForm`][wolfram] and several input notations that parse to it.
[Rhombus][rhombus] gives Racket conventional syntax by parsing shrubbery
notation into S-expressions. SimPy and Python share one syntax tree. It is
also Lisp's original plan.

"One canonical spelling" survives, per projection. What stops being true is
that there is only one projection.

### Rules for the surface an agent writes

| # | Rule | Removes |
| --- | --- | --- |
| 1 | Prefix application stays. No precedence anywhere | Operator-table recall |
| 2 | Leaves nest to a small fixed depth. A deeper expression needs a named step. The kernel enforces this, not the formatter | Closer counting; unnamed intermediates |
| 3 | Values are named before they are used. Flow reads forward | Inside-out composition |
| 4 | Blocks close by name | Anonymous closers on long spans |
| 5 | A pair is visibly a pair: one line or one delimiter each | Shifted flat lists |
| 6 | A name being introduced is spelled differently from a name being used | Binds-or-compares confusion |
| 7 | Field read, lookup, and call are spelled differently | Overloaded application |
| 8 | Look familiar where behavior is familiar, and unlike anything where it is not | False friends |

Rules 2 through 4 restrict structure. Rules 5 through 7 add notation. Rule 8
chooses which neighbors to resemble: ML and Rust for `match`, `option`,
`result`, and type annotations, and nothing at all for claims, intents, and
machines.

### The same function, restructured

```text
defn sequence
  takes items (array u8), index u64, first u32
  gives (option (tuple u32 u64))

  when (within first 0u32 127u32)
    give (option.some (tupleof first 1u64))
  when (within first 194u32 223u32)
    second u32 = try (continuation items index 1u64 128u32 191u32)
    high u32 = (u32.mul (u32.sub first 192u32) 64u32)
    give (option.some (tupleof (u32.add high second) 2u64))
  when (within first 224u32 239u32)
    give (three items index first)
  when (within first 240u32 244u32)
    give (four items index first)
  otherwise
    give (option.none)
end sequence
```

Leaves are still parenthesized prefix calls. The spine is lines. This is
not a re-spelling of the tree above: the guard chain is flat and the inner
`match` became a `try` and a named step. It is the same function in the
flatter tree the rules require.

| | Tree form | Line form |
| --- | --- | --- |
| Tokens (`o200k_base`) | 192 | 197 |
| Maximum nesting depth | 9 | 3 |
| Longest closer run | 6 | 2 |
| Named intermediate values | 1 | 2 |

One example proves nothing about accuracy. It shows only that the flatter
form does not have to cost tokens.

### Tables for case analysis

A machine's `step` matches on every pair of state and event. That is a
grid, and a grid has a notation.

```text
machine transfer

| in state  | on event      | go to     | emit                         |
| --------- | ------------- | --------- | ---------------------------- |
| idle      | requested     | debiting  | (ledger.debit from amount)   |
| idle      | debit-ok      | idle      | (alert.unexpected event)     |
| idle      | credit-failed | idle      | (alert.unexpected event)     |
| debiting  | requested     | debiting  | (reply.busy id)              |
| debiting  | debit-ok      | crediting | (ledger.credit to amount)    |
| debiting  | credit-failed | idle      | (alert.unexpected event)     |
| crediting | requested     | crediting | (reply.busy id)              |
| crediting | debit-ok      | crediting | (alert.unexpected event)     |
| crediting | credit-failed | ?         | ?                            |
```

The kernel writes the left two columns, one row per pair. The agent writes
the right two. A `?` is a hole, and a table with a hole is not admitted.
The case nobody thought of is a visibly empty cell.

Tabular notation for exactly this purpose is old.
[SCR requirements tables][scr] are checked mechanically for missing cases
and for overlapping ones. Models already write tables fluently.

### Checked notations

The UTF-8 table can be source:

```text
layout utf8-scalar gives char
  0xxxxxxx
  110xxxxx 10xxxxxx
  1110xxxx 10xxxxxx 10xxxxxx
  11110xxx 10xxxxxx 10xxxxxx 10xxxxxx
  where: shortest-form, scalar-value
end utf8-scalar
```

From it the kernel derives the decoder, the encoder, and the law that they
are inverses. The constants `192`, `64`, and `4096` are never written by
anyone.

A notation here is not a macro. It is a cell in the store with four parts:
a grammar, an elaboration into core forms, a printer, and the round-trip
law. It cannot capture names, cannot run arbitrary code at expansion, and
is identified by hash like any other cell. Because its grammar is data, a
constrained decoder can follow it, and the skill that teaches it can be
generated. [Lean 4][lean] shows an extensible notation layer living over a
small trusted kernel.

Candidates are the domains that already have a settled notation: byte
layouts, text formats and grammars, state tables, units and dimensions,
time and duration, and relational queries. Arithmetic formulas are a
candidate too, but the prefix result above says to test before assuming.

### A surface per model

Once projections round-trip, the surface a model sees is a parameter, not a
language decision. It can be chosen by measurement, per model family, and
changed when a model changes. The existing corpus converts mechanically,
and so does the skill.

The corpus can also train. The kernel can emit unlimited programs that are
checked by construction, in any projection.
[MultiPL-T][multiplt] fine-tunes models for low-resource languages,
Racket included, on translated code that tests have validated. With
enough verified corpus, learnability stops constraining the design and
becomes a training budget.

## What this changes in the rethink

- "Alien" describes the checking and execution model, not the page.
- Moves in the refinement protocol are text in the language.
- The token-minimal trap narrows to removing names and checkable
  redundancy.
- "One canonical spelling" becomes "one canonical tree, one spelling per
  projection".
- Tables and layouts are holes made visible, which is
  [Shift 3](README.md#shift-3-authoring-is-refinement-not-text) reached
  from the notation side.

## Risks

- **Every new surface is out of distribution too.** The line form could
  call up Python habits as the tree form calls up Lisp ones. `=` invites
  reassignment, which Vibra rejects.
- **The evidence is thin where it matters most.** Anka is one small study.
  The Clojure reports are anecdotes, and the newest ones say the problem is
  fading. The prefix result comes from small trained models. Nothing here
  measures Vibra.
- **More than one surface is more to keep correct.** Diagnostics must map
  spans into each projection, and a human and an agent discussing the same
  cell see different text.
- **A depth limit can produce junk names.** An agent forced to name a step
  may call it `tmp-3`. Computed names are the intended answer, and are
  untested.
- **Notations multiply.** Each one is another thing a model must have seen.
  The bar for adding one should be that the domain already writes it that
  way.

## Experiments

| # | Question | Measure | Abandon if |
| --- | --- | --- | --- |
| S1 | Does any surface beat the tree form? | Convert the conformance corpus and examples to tree, line, and ML-like surfaces. Same tasks and skill budget; first-attempt check-clean rate and test pass rate per model | The tree form is within noise of the best |
| S2 | Does a depth limit help within today's syntax? | Checker-enforced limit of 4 against no limit | No change in error rate, or names turn to junk |
| S3 | Do marked binders remove pattern mistakes? | Rate of binds-or-compares diagnostics with and without marking | The diagnostic is already rare |
| S4 | Do tables beat nested `match` for machines? | Missing-case rate and tokens per accepted machine | Models fill tables no more completely |
| S5 | How large is the false-friend cost? | Count of Lisp habits in first attempts in tree form, and Python habits in line form | Both are negligible with the current skill |
| S6 | Which surface needs the shortest skill? | Smallest skill that reaches 95% check-clean | Skill length does not vary by surface |
| S7 | Are moves better as text or as tool calls? | Acceptance rate and tokens per accepted cell | No difference |

S2, S3, and S5 need no new surface. S2 is a lint, and S5 is a count over
transcripts that already exist.

## If one thing were decided today

Keep the S-expression tree and stop treating it as the only thing an agent
may write. Three changes are available without leaving it: a depth limit,
marked binders in patterns, and delimited pairs. A second, line-oriented
surface is the larger bet, and S1 is how to find out whether it pays.

## Sources

- [Anka][anka]
- [EsoLang-Bench][esolang] and
  [in-context learning for esoteric languages][esoicl]
- [Leveraging code for in-context semantic parsing][bogin]
- [MultiPL-E][multipl] and [MultiPL-T][multiplt]
- [Reasoning or Reciting?][wu]
- [History of Lisp][mccarthy]
- [clojure-mcp-light][cljlight], [clojure-mcp][cljmcp], and a
  [practitioner thread][hn]
- [MoonBit][moonbit]
- [Self-attention and bounded hierarchical languages][yao] and
  [nested recursion with symbol masking][bernardy]
- [Code in JSON][aider], [The Format Tax][formattax], and
  [grammar-aligned decoding][gad]
- [SimPy][simpy] and [the cost of code formatting][format]
- [Prefix, postfix, and infix targets][prefix]
- [On the expressive power of programming languages][felleisen]
- [Wolfram expressions][wolfram], [Rhombus][rhombus], and
  [Lean 4 macros][lean]
- [Consistency checking of SCR tables][scr]

[anka]: https://arxiv.org/html/2512.23214v1
[esolang]: https://github.com/Lossfunk/EsolangBench
[esoicl]: https://openreview.net/forum?id=u6iFnIttKP
[bogin]: https://aclanthology.org/2024.naacl-long.279/
[multipl]: https://arxiv.org/abs/2208.08227
[multiplt]: https://github.com/nuprl/MultiPL-T
[wu]: https://aclanthology.org/2024.naacl-long.102/
[mccarthy]: https://www-formal.stanford.edu/jmc/history/lisp/node3.html
[cljlight]: https://github.com/bhauman/clojure-mcp-light
[cljmcp]: https://www.github.com/bhauman/clojure-mcp
[hn]: https://news.ycombinator.com/item?id=49269429
[moonbit]: https://llm4code.github.io/2024/assets/pdf/papers/7.pdf
[yao]: https://arxiv.org/html/2105.11115
[bernardy]: https://aclanthology.org/2021.findings-acl.67.pdf
[aider]: https://aider.chat/2024/08/14/code-in-json.html
[formattax]: https://arxiv.org/html/2604.03616
[gad]: https://proceedings.neurips.cc/paper_files/paper/2024/file/2bdc2267c3d7d01523e2e17ac0a754f3-Paper-Conference.pdf
[simpy]: https://dl.acm.org/doi/10.1145/3650212.3680347
[format]: https://arxiv.org/html/2508.13666
[prefix]: https://arxiv.org/abs/1912.00871
[felleisen]: https://www2.ccs.neu.edu/racket/pubs/scp91-felleisen.pdf
[wolfram]: https://reference.wolfram.com/language/tutorial/TextualInputAndOutput.html
[rhombus]: https://dl.acm.org/doi/10.1145/3622818
[lean]: https://arxiv.org/abs/2001.10490
[scr]: https://dl.acm.org/doi/10.1145/234426.234431
