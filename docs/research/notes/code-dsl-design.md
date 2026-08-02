# Code-generation DSL design: five-paper distillation

Reading notes for Vibra's language design. Sources in
`C:\Users\jorge\Documents\papers\vibra-research\code\`. All content paraphrased.

## Anka: a DSL for reliable LLM code generation (Al Mazrouei, UW–Madison, arXiv 2025)

- **Problem.** LLMs fail systematically on multi-step programs; the hypothesis is that
  general-purpose languages offer too many equivalent forms and too much implicit state,
  so the model must choose and chooses wrong.

- **Core mechanism.** Data-pipeline DSL, four rules: exactly one canonical form per
  operation (filtering is only `FILTER src WHERE cond INTO dst`); every operation must
  name its output via `INTO`, so no anonymous or reused intermediates; operations sit in
  named `STEP` blocks making sequence syntactic; English keywords instead of operators
  and method chains. A `PIPELINE` declares typed inputs (`TABLE[field: TYPE, ...]` over
  INT/STRING/DECIMAL/BOOL/DATE/DATETIME), steps, and `OUTPUT`. 18 operations plus
  IF-ELSE, FOR_EACH, WHILE, TRY-ON_ERROR. ~6.4 kLoC Python: Lark grammar (98
  productions), 68 immutable AST dataclasses, tree-walking interpreter. No constrained
  decoding — the model learns Anka from a ~100-line syntax guide in the prompt.

- **Evidence.** 100 tasks / 8 categories, 10 samples each at temp 0.3; a task counts
  correct if >=50% of samples match. Claude 3.5 Haiku: 95.8% overall vs Python 91.2%;
  multi-step 100% vs 60%; 99.9% parse success with zero training exposure. GPT-4o-mini
  replicates the multi-step gap (+26.7 pts). Python failures split 42% variable
  shadowing / 31% ordering / 27% chaining. Weak instrument: single author, self-designed
  benchmark, headline bucket is 10 tasks (100% vs 60% = 10 vs 6). Direction credible,
  effect size not.

- **Design implications.**
  - The gain lives in *state threading across steps*, not aesthetics. Falsifiable: a
    Vibra variant keeping the keywords but allowing reused intermediate bindings should
    lose most of the advantage.
  - One canonical spelling per operation (their arithmetic: 3 choice points x 5 steps =
    243 candidate programs collapsing to 1), and mandatory output naming — the latter is
    the highest-leverage single rule, targeting the largest measured error class.
  - Expect no benefit under ~3 operations, and regression on branch-heavy logic.

- **Caveats.** Pipelines only; Anka *lost* by 10 pts on nested-conditional tasks.
  Constrained language, not constrained decoding — nothing enforces the grammar at
  sampling time.

## LLMON: an LLM-native markup language (Hind, Shbita, Wu, Ahmed, DeLuca, Fulton, Cox, Gutfreund — IBM Research, arXiv 2026)

- **Problem.** A prompt is one flat string: instructions and data are indistinguishable,
  spans cannot be named or referenced, nothing says which instruction should run. Root
  cause of prompt injection and lost-in-context failures.

- **Core mechanism.** Two isomorphic syntaxes. Human LLMON writes `\tag\ text /tag/`;
  tags are user-defined, may carry an instance name after `:` (`instr:task_a`), and
  encode nesting *flattened into the tag name* via `.` (`email.header.from`) — the
  argument being that a transformer has no pushdown stack, so tree nesting costs context
  and is learned poorly. Annotations are strictly prefix so they can steer generation. An
  `exec` span binds a referenced `instr` plus optional `data` input: that is the
  execution-control primitive. Machine LLMON re-encodes the grammar with exactly six
  reserved special tokens the tokenizer treats atomically, so boundaries are stable and
  unfragmentable and escaping disappears. Typing is by cast tag (bare `3.4` is a string,
  `\float\3.4/float/` a number). Two exploitation paths: post-train on LLMONized corpora,
  or mask attention over declared boundaries at inference with no retraining.

- **Evidence.** Granite-4.0-Micro-Base (3B) and Qwen2.5-3B, SFT on ~3.4M examples /
  2.9B tokens (Alpaca+Dolly wrapped, distractors injected). On a 100-instance Distractor
  benchmark scored by an LLM judge, base and chat-template baselines score ~0–0.4;
  LLMON full fine-tuning reaches 83–88, LoRA ~70–75 (avg +74.2 pts). Inference-only
  masking on three instruct models: ~41–43 to 69–72 (avg +29.3), no training. Capability
  cost is real and soft-pedaled — Granite MMLU falls 61.5 to 36–43. Near-zero baselines
  make the delta partly an artifact.

- **Design implications.**
  - Separate representation from enforcement: declared boundaries pay only when a runtime
    (mask, decoder, interpreter) acts on them. Named spans with no enforcement path buy
    nothing.
  - Flatten hierarchy into dotted names at the model interface even if the IR is a tree.
    Deep nesting is a decoding liability, not merely verbosity.
  - Annotations must precede the content they govern; postfix metadata cannot steer.
  - Reserve a tiny set of atomic delimiter tokens rather than reusing punctuation —
    tokenizer-level atomicity is what makes boundaries learnable.

- **Caveats.** 3B-class models only; larger ones may already internalize structure.
  Measures distractor robustness, not code generation.

## Pel: a language for orchestrating AI agents (Mohammadi, CMU, preprint June 2025)

- **Problem.** Function calling cannot express control flow, scales badly (hundreds of
  JSON schemas degrade reasoning), and delegates verification to model judgment. Emitting
  Python is expressive but needs sandboxing and cannot be restricted at the grammar
  level, Python's grammar being too large to compile to a decoding automaton.

- **Core mechanism.** A small Lisp. `(...)` is always application, operator-first;
  `[...]` always a data list — the call/list ambiguity is removed and special forms are
  eliminated, so `if`/`case`/`for`/`do` are ordinary non-strict closures callable with
  named arguments. Fixed arity gives automatic currying on under-application. Composition
  is by pipe: `|>` injects into first position, `^` is an explicit placeholder for any
  position, recursing into nested structures. Rationale is generation order — nested
  `bar(foo(a))` forces committing to `bar` first, whereas `(foo a) |> (bar ^)` lets the
  model decide afterward. Literal lists are themselves closures taking
  `:at`/`:from`/`:to`, unifying indexing, slicing, and key lookup with call syntax.
  Safety is grammar-level: the EBNF is small enough to edit, so disabling network, file
  I/O, or specific builtins is a grammar edit enforced during constrained decoding,
  replacing runtime sandboxing. Natural-language conditions are first class — a string in
  a `case` arm is dispatched to an LLM against the scrutinee. The REPL preserves the
  pre-error environment and offers Common-Lisp-style restarts, including rewriting only
  the failing expression and a self-healing mode that hands the exception plus the
  callee's docstring to a helper LLM. Optional async mode pre-scans top-level ASTs and
  runs forms with no defines/uses dependency concurrently.

- **Evidence.** None. No benchmark, no user study, no ablation — a design paper with
  worked examples of router/terminal agent hierarchies. Every performance claim is
  argument, not measurement. Source of mechanisms only.

- **Design implications.**
  - Pipe-with-placeholder suits a left-to-right generator: no need to plan the outer call
    first. Cheap to test in Vibra and falsifiable.
  - Capability restriction belongs in the grammar/manifest, not a runtime sandbox, if
    constrained decoding is the target — imposing a hard size budget on Vibra's surface.
  - Preserve environment across failure, and make errors carry the callee's documented
    signature. Discarding expensive agent-call results because a later line is malformed
    dominates cost in agentic runs; restart-and-repair beats a better error message.

- **Caveats.** Unvalidated, single author. Author-acknowledged: no user-defined
  non-strict functions; restarts unreliable in full async mode. Grammar-level safety
  assumes you control decoding, ruling out black-box APIs.

## Compiled AI: deterministic code generation for workflow automation (Trooskens, Karlsberg, Sharma, De Brouwer, Van Puyvelde, Young, Thickstun, Alterovitz, De Brouwer — XY.AI / Stanford / Cornell / Harvard, arXiv 2026)

- **Problem.** Agent frameworks invoke a model per transaction, making cost, latency, and
  behavior non-deterministic and unauditable. Most enterprise workflows need intelligence
  to *author* logic, not re-derive it per request.

- **Core mechanism.** Remove the model from the execution loop. Input is a YAML workflow
  spec; an orchestrator selects from tested templates (sync handler, streaming,
  batch-with-checkpointing, validating input), reusable modules (DB access, HTTP with
  retry, notifications), and compliance prompt blocks, assembles one prompt, and invokes
  the LLM *once* to fill narrow business-logic functions bounded to ~20–50 lines. The
  artifact passes four ordered gates — static security analysis (Bandit/Semgrep),
  syntax/type/lint (AST, mypy, ruff), sandboxed execution against fixtures, accuracy
  against golden data — regenerating with the error text on failure. Output is a static
  Temporal activity. Escape hatch for genuinely semantic steps is *bounded agentic
  invocation*: a compiled artifact may make a narrow schema-validated LLM call (the
  "Code Factory" variant) while the surrounding flow stays deterministic.

- **Evidence.** Two tasks, Claude Opus 4.5 at temperature 0. BFCL (n=400): 96% task
  completion with all 16 failures caught at compile time; one-time 9.6K generation tokens
  then zero at runtime; break-even at ~17 transactions, 57x fewer tokens at 1,000, TCO
  $555 vs $22,000 at 1M tx/month; 4.5ms vs 2,004ms p50; 100% reproducibility vs 95% for
  runtime inference. DocILE (5,680 invoices): pure regex compilation collapses (20.3%
  KILE) while Code Factory matches direct LLM (80.0%) and leads on line-item recognition
  (80.4%). Cost and determinism figures are near-tautological consequences of the
  architecture and therefore robust; quality and security figures rest on 20–30-item
  fixture sets and one model. Generated code averages cyclomatic complexity 23.8 vs 8.

- **Design implications.**
  - Compile-once/run-many is the economic default for well-specified workflows, and
    templates bounding the model to small holes beat better prompting: the error space
    shrinks because APIs and schemas come from pre-tested infrastructure.
  - Validation must be staged and mandatory, and the *accuracy* gate earns its keep — 4%
    of artifacts executed cleanly with wrong outputs, i.e. silent production failures.
    Vibra should treat "compiles and runs" as insufficient for admission.
  - Provide an explicit bounded-LLM-call construct with schema and retry. Pure
    deterministic compilation loses ~60 accuracy points on noisy input; the hybrid does
    not.

- **Caveats.** Assumes users can write a correct YAML spec — the specification problem is
  untouched and, by their framing, fundamental. Two task types, one model; artifact
  quality is model-dependent, so upgrades force re-validation.

## Type-Constrained Code Generation (Mündler, He, Wang, Sen, Song, Vechev — ETH Zurich / UC Berkeley, PLDI 2025)

- **Problem.** Grammar-constrained decoding enforces only syntax, but in their
  measurements syntax is ~6% of compilation errors in LLM-generated TypeScript while
  ~94% are type failures. Type systems are not context-free, so CFG-based completion
  engines cannot be reused.

- **Core mechanism.** A completion engine deciding, for a partial program, whether some
  suffix makes it well-typed, driving a sample-and-check loop (sample a token, test
  whether the extended prefix stays in the prefix language, else zero its probability and
  resample). The machinery is a *prefix automaton*: a non-deterministic automaton over
  Unicode characters whose states are created dynamically and annotated with typing
  context — type environment, the left-hand-side expression being extended, the parsed
  expression's type, and any type it is constrained to inhabit. The prefix property
  (every reachable state can still reach an accepting state) makes reachability equal
  prefix-language membership, and is established compositionally over union,
  concatenation, Kleene-star, and terminal automata. The hard part is extension
  expressions: deciding whether a partial expression can be completed to a required type
  is type inhabitation, solved by a type-reachability search over an abstracted type
  graph (nodes = types; edges = member access, call, operator), pruned by depth/root
  heuristics that skip higher-order types offering no new reachable types. Function
  automata track declared return types and force another statement when not all paths
  return. Formalized on a simply-typed Turing-complete core, then extended to a
  TypeScript subset with operator precedence woven into both parsing and type search. To
  keep search decidable they *force* annotations: all parameter and return types, all
  declarations annotated or initialized, and `reduce` callback first parameters. 11,249
  lines of Python, differentially tested against `tsc`.

- **Evidence.** Six open-weight models (2B–34B: Gemma 2 2B/9B/27B, DeepSeek Coder 33B,
  CodeLlama 34B, Qwen2.5 32B) on TypeScript HumanEval (159) and MBPP (384) from
  MultiPL-E, across synthesis, translation, and repair. Compiler errors drop 74.8% /
  56.0% in synthesis vs an *idealized* syntax-only ceiling of 9.0% / 4.8%; worst
  per-model reduction 54.8% / 27.3%. pass@1 improves +3.5 (synthesis), +5.0
  (translation), +37.0 relative (repair). Median runtime overhead +39.1% / +52.1%, with
  99.4% of tokens admitted on first check — which is why sample-and-check beats masking
  the whole vocabulary, an implementation they report times out on every instance.
  Strongest evidence in the set: multiple families and sizes, three tasks, a real
  compiler as oracle, an honest syntax-only baseline. Main confound: the constrained side
  is not full TypeScript.

- **Design implications.**
  - Constrain on types, not grammar. The 6%-vs-94% split is load-bearing: a YAML-shaped
    grammar buys almost nothing by itself.
  - Build Vibra's checker as an incremental completion engine over *prefixes* from the
    start, not a batch validator retrofitted later. Their own recommendation to language
    designers is that compilers ship an incremental completion mode.
  - Mandatory annotations are a feature: they make prefix-time type search terminate and
    feed the model more information. Requiring explicit signatures is defensible on
    decoding grounds alone.
  - Sample-and-check rather than pre-masking the vocabulary — cost scales with the ~0.6%
    rejection rate.
  - Residual failure mode is non-termination: steered off a bad member access, models
    loop unconstructively. Constrained decoders need expression-complexity or step limits.

- **Caveats.** Open-weight models only, since constrained decoding needs next-token
  distributions. TypeScript subset; higher-order types deliberately under-covered;
  11 kLoC of hand work per language. Gains are on compilability and pass@1, not logic —
  a well-typed wrong program is still wrong.

## Cross-cutting synthesis

**Agreements.** All five relocate reliability from the model to the interface. Anka, Pel,
and LLMON converge independently on three primitives: one canonical form per operation,
explicitly named intermediate results, and prefix-position annotations that steer rather
than describe. Anka's `INTO`, LLMON's instance names bound by `exec`, and Pel's
`def`-plus-pipe are one idea — identifier-based reference beats positional or implicit
reference — and it is the only claim with independent support on both code and
instruction following. Pel, Compiled AI, and Type-Constrained agree the surface must be
small enough to enforce mechanically; the latter two agree enforcement must be staged,
since syntax is near-worthless and semantics is where errors live.

**Contradictions.** Sharpest is *constrained language vs constrained decoding*. Anka
claims complementarity but tests only the former, and its 99.9% parse rate shows syntax
was never its bottleneck — consistent with the 6% figure, and thus an argument that
Anka's gains came from naming and step discipline rather than constraint per se. Pel
stakes its whole safety story on grammar-level restriction, which Type-Constrained's data
suggests is the weak half of the problem. Second, LLMON wants flattened structure and a
minimal token set while Anka wants verbose English keywords; these pull opposite ways on
token budget and have never been tested against each other. Third, Compiled AI treats
runtime LLM invocation as a defect to eliminate while Pel makes it a first-class
control-flow primitive — and Compiled AI's own DocILE numbers show the eliminationist
position failing by ~60 points on noisy input, with Code Factory a concession to Pel's
side.

**Open questions.** (1) Nobody separates canonicalization from named binding from step
scaffolding — Anka bundles four principles and ablates none, so the design cannot be
prioritized from evidence. (2) No paper combines a designed-for-LLM language *with*
type-constrained decoding; the obvious experiment is untried. (3) All language-design
evidence comes from small or mid-size models; LLMON flags that larger models may already
internalize structure, which would shrink or invert these gains. (4) No human
readability data, though three of the five assume humans review output. (5) The ~11 kLoC
cost of a type-aware completion engine for one language subset is unamortized — whether
a language can be *designed* so its completion engine is cheap is the question Vibra is
positioned to answer, and none of these papers asks it.
