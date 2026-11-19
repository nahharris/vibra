# Evidence on syntax and LLM code generation

Research date: 2026-11-15. Sources were found through web search. Full papers were mostly NOT opened; numbers come from search-result summaries of abstracts and secondary pages, so verify any number before quoting it. No URL was fetched directly.

## (a) Directly measured findings

### Type-constrained decoding reduces compile errors
- Claim: using the type system to constrain decoding cuts compile errors by more than half and raises functional correctness 3.5-5.5% relative on synthesis and translation. Repair of non-compiling code improves 37% relative. About 94% of compile errors in the baseline were type-check failures. An ICLR workshop version reports 70.9% fewer compile errors and +16.2% correctness.
- Source: https://arxiv.org/abs/2504.09246 (Muendler et al., 2025; PLDI '25).
- Limitation: models from 2B to 34B parameters, on HumanEval and MBPP style tasks (small problems), and the language is TypeScript-like. The gain comes from rejecting invalid tokens, which is a different mechanism from "types written in the source help the model".

### Format restrictions can hurt reasoning
- Claim: strict format constraints (JSON mode and similar) lower accuracy on reasoning tasks and can help classification tasks. Stricter constraints mean larger drops.
- Source: "Let Me Speak Freely?", https://aclanthology.org/2024.emnlp-industry.91.pdf (2024).
- Limitation: tested on natural-language reasoning and structured answers, not on code. A blog cites a figure of "up to 27 points"; I could not confirm it, so I do not use it.
- Follow-ups: CRANE (https://arxiv.org/abs/2502.09061, 2025) argues a grammar that is too narrow leaves no room for reasoning, and fixes this by letting the grammar allow free reasoning before the constrained answer. "Thinking Before Constraining" (https://arxiv.org/html/2601.07525, 2026) does the same with trigger tokens. Both support: constrain the final program, not the thinking.

### Whitespace and indentation cost tokens but not accuracy
- Claim: stripping indentation, whitespace and newlines cut input tokens by about 24.5% with no statistically significant accuracy loss, over 10 models and Java, Python, C++ and C#.
- Source: https://arxiv.org/abs/2508.13666 (2025).
- Limitation: fill-in-the-middle completion, not generation from scratch. Python cannot drop indentation, so its savings were limited. This says nothing about whether indentation-sensitive syntax causes more errors.

### Low-resource languages score much lower
- Claim: on MultiPL-E, Python scores are more than 3x those of low-resource languages in one survey. Julia and Lua approach high-resource levels for large models, while R and Racket keep a major gap. MultiPL-T (translate Python training data into the target language, then fine-tune) lifted a 15B model on Lua to 31.0 vs 30.6 for Python.
- Sources: survey https://arxiv.org/abs/2410.03981 (2024, ACM 2025); MultiPL-T https://greenberg.science/papers/2024oopsla_multiplt.pdf (2024).
- Limitation: gap sizes depend on model and era. Both results are from older, smaller models than today's frontier.

### What recovers a low-resource language is model-size dependent
- Claim: for small models fine-tuning is best. For large models in-context learning is the safer bet, and fine-tuning can hurt (a secondary summary says DeepSeek Coder 33B on R fell from 30.2% to 25.3%). Translation-based pre-training added little. "No silver bullet."
- Source: https://arxiv.org/abs/2501.19085 (Giagnorio et al., 2025).
- Limitation: R and Racket only, which do have substantial public code. A brand-new language has zero public code.

### Grammar prompting
- Claim: putting a BNF grammar (a per-example subset) in each demonstration lets an LLM write a DSL competitively on semantic parsing, PDDL planning and molecule generation.
- Source: https://arxiv.org/abs/2305.19234 (NeurIPS 2023).
- Limitation: I did not retrieve the numbers. Tasks are small DSLs and the models were 2023-era.

### Token cost across languages
- Claim: a RosettaCode comparison with a GPT-4 tokenizer found a 2.6x gap between C (most tokens) and Clojure (fewest). J averaged about 70 tokens versus 109 for Clojure.
- Source: https://tokencalculator.ai/most-token-efficient-languages-for-llms-ranked-priced/ and similar blogs (2025-26). I did not find or open the original RosettaCode analysis.
- Limitation: blog-grade. Dan Luu's critique says the test problems are trivial. Token counts depend on the tokenizer.

### TOON (data format, not code)
- Claim: TOON uses about 39.9% fewer tokens than JSON, with 76.4% vs 75.0% retrieval accuracy over 209 questions and 4 models.
- Source: https://www.mintlify.com/toon-format/toon/advanced/benchmarks (project's own benchmark, 2025-26).
- Limitation: maintainers' own numbers, and it measures LLMs reading the format, not writing it. Its authors concede JSON can be smaller for nested data.

## (b) Weak or indirect evidence

- Lisp and parentheses: no controlled study found. An Advent of Code 2025 recap had Python 10/10 first-try solvers and Clojure 5/10 (https://benjamin-thomas.github.io/posts/aoc-2025-llm-benchmark-recap). One informal test; confounded by training-data volume, and it does not split out paren errors. Hacker News accounts say paren imbalance on edits was common in earlier models and is now fixed quickly through tool feedback. Anecdotes only.
- Functional vs imperative: GPT-5 pass rate was 42.3% Haskell, 52.2% OCaml, 61.1% Java, with 24.3% Haskell compile errors (https://arxiv.org/pdf/2601.02060, 2026). Clojure and Lisp not included. Confounds: type systems, library ecosystems and training data all differ together with syntax.
- A custom Lisp dialect (PTC-Lisp) scored 149/150 in its author's benchmark (https://hexdocs.pm/ptc_runner/benchmark-eval.html). No baseline, one model, and failures came from models expecting Clojure functions. This suggests a small Lisp is learnable in context, but it is not a controlled comparison.
- Parameter-level API errors: a study annotated 3,209 method-level and 3,492 parameter-level API misuses in LLM code, including hallucinated and missing parameters (https://arxiv.org/abs/2503.22821, 2025). It shows parameters are a major error source. It does not test named versus positional arguments.
- Survey claims that LLMs fail often on syntax in functional languages or in translation are general error taxonomies (https://arxiv.org/pdf/2503.01245), not syntax-shape experiments.

## (c) Questions with NO evidence found

- Indentation-sensitive vs brace-delimited vs keyword-closed (`end`, `fi`) blocks: no controlled comparison of LLM correctness.
- Bracket-matching and long-range closer errors as a measured rate by syntax family: nothing quantitative found.
- Named or labeled arguments reducing LLM errors: no direct study.
- Whether explicit type annotations written in source (as opposed to enforced by decoding) reduce LLM errors: only blogs and the decoding paper; not isolated.
- Redundancy (repeated information, closing labels) as an error-reducing feature: nothing.
- Prefix (S-expression) vs infix notation correctness: nothing controlled.
- A language designed for LLM authors with a measured evaluation: Pel (https://arxiv.org/abs/2505.13453, 2025) is a Lisp-style agent-orchestration language whose grammar is designed for constrained generation, but I saw no benchmark numbers for it. "Beyond Human-Readable" (https://arxiv.org/html/2604.07502, 2026) is a position paper proposing an LLM-native language with no evaluation.
- Brand-new language with zero training data and a full-size spec in context: no direct study found; the closest are the low-resource papers above.
- Grammar-constrained decoding gain for general-purpose code, with a measured harm to code quality: only the NL-reasoning harm above.

## Reading the evidence for Vibra

- The solid findings favor: types checked during generation, compiler feedback, constraining only the final program text, and in-context documentation for large models.
- The syntax-shape questions (braces vs indentation vs `end`, S-expressions, named arguments) are open. Any choice there is a hypothesis; a cheap experiment on a few frontier models would answer more than the literature does.
- Token savings from dropping whitespace are measurable (about 25%) and did not hurt accuracy in completion, but nobody has shown token-lean syntax improves generation.
