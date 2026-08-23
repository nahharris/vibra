# Verification, Retrieval, and Repair: Reading Notes

Sources in `papers/vibra-research/code/`. All paraphrase; no tables or figures reproduced.
Lens: a YAML-surface, statically typed, LLM-targeted language and its tooling.

## NL2VC-60: NL to Verified Dafny (Erfan, Chowdhury, Ryan, Rahman; arXiv preprint, Apr 2026)

- **Problem.** Writing the Dafny spec (pre/postconditions, loop invariants, `decreases`
  measures) is harder than writing the code, and Dafny is data-scarce (~779 GitHub repos vs.
  millions for Python).

- **Core mechanism.** Benchmark plus a three-tier prompting ladder. NL2VC-60: 60 hand-written
  Dafny programs from UVa Online Judge problems (~179-word descriptions vs. ~19 LoC in
  Clover/MBPP-Dafny), with contest "presentation flavor" (line counts, magnitude bounds)
  manually stripped so only semantic requirements remain; ~300 person-hours to author the
  reference specs. Tiers: (1) contextless — NL only; (2) signature — NL plus a fixed method
  signature (names, types, arity given; body and annotations not); (3) self-healing — raw
  verifier diagnostics fed back, ≤10 rounds. Verification is Dafny 4.11 → Boogie → Z3.
  Critically, a second oracle blocks vacuity: every verifier-accepted program is also run
  against uDebug community test suites built for boundary/extreme inputs. Two-layer contract —
  SMT proves consistency with the spec, tests prove the spec was non-trivial.

- **Evidence.** Seven open-weight models (9B–120B), five temperatures, 11 problems per cell,
  verify@k. Contextless: five of seven score flat 0% at every temperature; Gemma 4-31B peaks
  54.55% verify@5. Adding only the signature: GPT-OSS-120B 0% → 63.64%; Qwen3.5-9B hits 72.73%,
  beating models 13x larger. Signature-based self-healing: GPT-OSS-120B 81.82%, Gemma 4-31B
  90.91%. Contextless self-healing leaves most models at 0%. Error mix shifts syntax →
  semantic/type → verification as scaffolding improves. Direction is large and consistent across
  seven independent models; magnitudes are not — n=11 means one problem is 9 points, and 90.91%
  is 10/11. Ignore cross-model gaps under ~10 points.

- **Implications.**
  - A typed signature beats model scale. Falsifiable: feeding the target symbol's signature
    (schema or `vibra query`) before generation should raise per-attempt success on unseen stdlib
    tasks more than a 10x larger model does.
  - Gate repair loops on "already parses and type-checks against a known signature." Contextless
    self-healing was worthless — diagnostics are unusable without a skeleton.
  - Errors migrate rather than vanish; track `schemas/linter-codes.json` category mix over time.
  - Never treat "checker passed" as done. Pair every static gate with `vibra test`; models satisfy
    weak specs with constant returns.

- **Caveats.** Dafny- and scarcity-specific. Signature prompting leaks information the model would
  otherwise invent. Ten repair rounds/problem is unpriced. Codestral-22B and Qwen3-Coder-30B
  regressed into *more* syntax errors during repair — feedback loops can diverge.

## ProjAgent: Procedural Similarity Retrieval (Chen, Imani, Ahmed; UC Irvine, arXiv preprint, Jul 2026)

- **Problem.** Repo-level generation needs context *useful for implementing* the target, but
  BM25/dense retrieval finds context *lexically or semantically near* it. Two guard-clause
  validators in different modules can share a computational pattern at 0.38 BM25, 0.59 embedding
  similarity, and no call-graph edge.

- **Core mechanism.** A third retrieval axis with a non-obvious index key. Functions are
  LLM-decomposed into *steps* — (description, snippet) pairs — validated by docstring substring
  match, else ROUGE-L > 0.7, else LLM entailment; snippets separately checked for line presence and
  description/snippet embedding cosine > 0.75. The retrieval key is not a text embedding but the
  backbone's last-layer hidden state over response tokens while reasoning about the step, projected
  onto a *reasoning subspace*: SVD of the unembedding matrix, dominant right singular vectors span
  the semantic subspace, the remainder (energy threshold 0.98, ~5% of hidden dims) spans the
  reasoning subspace. Projections are anisotropic, so PCA debiasing subtracts mean and PC₁.
  Retrieval is two-stage: projection similarity ≥ 0.75 generates candidates cheaply, then an LLM
  verifies same-computational-operation; seed expansion demands agreement from ≥ 2 seeds at ≥ 0.65
  to suppress coincidence, and an agentic explorer (ls, grep, read_func, read_lines, search_func,
  propose_func) reaches functions never decomposed offline. Semantic retrieval runs alongside:
  imports transitively traced via AST into a symbol pool of what the target can actually reference,
  scored 0.5·BM25 + 0.5·dense on a query enriched with retrieved step descriptions. Generation
  closes with a deliberately conservative static loop (AST syntax, then method-call, field-access,
  variable-method, standalone-call resolution), ≤10 iterations, reporting only confirmable errors.

- **Evidence.** REPOCOD: 980 problems, 11 Python repos, Qwen2.5-Coder-14B-Instruct, greedy. Pass@1
  41.14% vs. SpecAgent 34.52%, dense 28.83%, sparse 26.58%, same-file 14.98%. Ablation (85 Astropy
  problems): no procedural −15.4 points, no semantic −8.9, no static loop −0.9. The signal is a
  weak classifier: on 9,598 LLM-labeled pairs (370 human-validated, κ=0.86 human, 0.82 vs. LLM)
  best promoted-group F1 is 0.090 — 5.4% precision, 27.1% recall — while the *rejected* group holds
  ≥97.9% precision. Only ~35% of target steps have any procedurally similar context at all.
  Exhaustive offline decomposition instead of budgeted search adds 3.96 points. Single backbone,
  single ablation repo (cost-forced, stated).

- **Implications.**
  - Measure retrieval by *elimination* power, not ranking precision: 5% precision with 98% negative
    precision is a strong first stage behind a verifier. Build Vibra context tooling as recall-first
    filter + cheap exact check (type resolution against the symbol table).
  - Build the symbol pool from the module's real import graph. Vibra can do this *soundly at compile
    time* — imports and visibility are static — strictly better than their AST heuristic.
  - The static loop was worth 0.9 points *because* dynamic typing forced conservatism. Strongest
    argument here for a statically typed target; their future-work section asks for exactly that.
  - Index procedures, not names. Falsifiable: indexing stdlib functions by decomposed step behavior
    should beat identifier search when task vocabulary differs from the stdlib's.

- **Caveats.** Python-only, one backbone; the projection depends on that model's hidden-state
  geometry and may not transfer. Headline rests on LLM-judged labels. Very expensive: offline
  decomposition of thousands of functions, per-step LLM verification, agentic search.

## Recall Before Rerank (Venuta, Tosoni, Ferragina; Sant'Anna Pisa, arXiv preprint, Jun 2026)

- **Problem.** In recall-then-rerank code search, anything the first-stage embedding misses is
  unrecoverable — and that stage had never been systematically benchmarked for code-to-code.

- **Core mechanism.** Benchmark, not system: 17 models (125M–30B; bimodal encoders, unified
  encoder-decoders, contrastive code embedders, decoder-only LLMs in embedding mode) × 4 datasets ×
  5 languages, ~920 runs, ~403 GPU hours. Ground truth by exhaustive sequential scan rather than
  ANN, deliberately separating embedding quality from index-compression loss (Relative Distance
  Error proposed as the separate metric). Metrics: Precision@k and NDCG@k (k=50; 20 on MultiPL-E)
  plus throughput in KB of raw source/sec. Datasets: BigCloneBench Type-2/3 (Type-4 dropped for
  label unreliability), CodeNet (functional equivalence), MultiPL-E (positive iff same HumanEval
  problem and passes unit tests), xCodeEval. A style arm has Qwen2.5-Coder-7B rewrite code four ways
  (LLM cleanup; minus comments; plus identifiers renamed v1,v2…; both), applied query-side,
  corpus-side, or on the diagonal.

- **Evidence.** No universal winner. Lightweight encoders (StarEncoder >100 KB/s, sub-10 ms) run
  ~47x faster than Qwen3-Coder but lose up to 80 P@k points on xCodeEval, 30–60 on CodeNet, only
  10–15 on saturated MultiPL-E. Qwen3-Embedding-600M is the Pareto answer: 1/50th the size of
  Qwen3-Coder-30B, 97 vs. 52 on Python CodeNet, 93 vs. 8 on Java xCodeEval, 74 KB/s, 1024 dims.
  Training objective beats parameter count; decoder-only generative embedders are Pareto-dominated on
  both axes. Notable inversion: on BigCloneBench Type-3 (gapped clones) lightweight encoders score
  46–70% and top semantic embedders 29–42% — the authors attribute this to strong models
  over-normalizing away the lexical signal gapped clones need. Style normalization is an equalizer:
  rewriting *both* query and corpus lifts weak Code Llama 28–29 points (Java 20→48, Python 44→73)
  while top models move ~0–5; query-only rewriting gains almost nothing; stripping comments costs
  CodeXEmbed 15 points in Java. Careful design with exact ground truth; the authors' own main threat
  is benchmark contamination in pretraining.

- **Implications.**
  - Budget by KB/s and index dimension first, pick the smallest adequate model, spend savings on an
    exact reranker — for Vibra, the type checker or `vibra query`.
  - Normalize corpus and query identically or gain nothing. `vibra fmt` over both indexed sources
    and queries is the cheap version of their result.
  - Do not strip comments or normalize identifiers before indexing; their combined transform was the
    worst performer. Docstrings and names carry learned signal.
  - Evaluate on the unsaturated case: MultiPL-E clusters at 94–100% and discriminates nothing.

- **Caveats.** Code-to-code only; five mainstream languages, none low-resource or YAML-surfaced —
  Vibra has no pretrained embedder at all, so the transferable content is the *methodology*, not the
  model ranking. One GPU. No end-to-end two-stage experiment is actually run; the recommendation is
  inferred from stage-one numbers.

## Tool-Guided Retrieval-Augmented Repair for C (Sriram, Pradhan, Saha; Penn State, WIP preprint, Jul 2026)

- **Problem.** LLM-generated C is both uncompilable and insecure at high rates, and single-source
  feedback (retrieval alone, or one analyzer) fixes neither reliably for embedded targets.

- **Core mechanism.** Four stages, repairs ordered by *impact* — security before compilation.
  (1) Generate from NL plus a short security checklist; no type signatures or platform details.
  (2) GCC compile; on failure, diagnostics summarized. On success, CodeQL queries buffer overflow,
  out-of-bounds access, unsafe library calls (`gets`, `strcpy`, unbounded `sprintf`), unchecked
  returns, integer overflow/signedness; each finding stored as rule id + message + location.
  (3) Repair against a growing execution-time repository of prior outcomes. Entries are (task
  description, C code, metadata) with a composite quality score: 0.0 non-compiling, 0.25 compiles,
  0.50 compiles + KLEE-analyzable but has findings, 0.75 compiles + CodeQL-clean, 1.0 compiles +
  CodeQL-clean + KLEE-clean. Retrieval is contrastive — up to three entries scoring >0.60 as
  positive evidence, up to two compiling-but-insecure entries as negative, and negatives only when
  ≥3 positives exist so avoidance guidance cannot dominate. Retrieved entries are *not* few-shot
  code; they are distilled into ≤7 security practices and ≤3 avoidance hints, specifically so small
  models cannot copy examples verbatim. Dedup at cosine >0.90; repository capped at 500 entries,
  evicting lowest score. Three repair iterations, four attempts total. (4) One-shot KLEE symbolic
  execution *outside* the loop, producing no prompts; its verdict only updates the stored quality
  score, so future retrieval prefers security-clean and symbolically robust patterns.

- **Evidence.** 5,000 general C tasks, greedy decoding. CodeLlama-7B: compilation failure 45.56% →
  27.50%, security defects 48.50% → 19.32%, CodeQL findings 15,088 → 2,463 (83.7%), KLEE-analyzable
  54.10% → 71.20%. DeepSeek-Coder-1.3B: 42.44% → 21.78%, 34.62% → 15.16%, 56.80% → 77.44%.
  Conditioned on compiling — the honest framing, since CodeQL cannot run otherwise — security-clean
  rises 10.9% → 73.4% and 65.4% → 80.6%. Residual findings map ~58–60% to NIST categories, dominated
  by Exceptional Condition Handling (unchecked `scanf`) and Buffer Overflow. Believability is the
  weak point, as the authors say: no ablation, so the split between retrieval, compiler feedback,
  static analysis, and impact ordering is unknown; general algorithmic C proxies for firmware; the
  shared repository makes results order-dependent with no shuffled control; the third model
  (Qwen2.5) appears only in the baseline.

- **Implications.**
  - Score artifacts on a monotone ladder of checks passed, not pass/fail. Vibra has the rungs —
    parses, type-checks, lints clean, `vibra test` green — and that score should drive both retrieval
    priority and final candidate selection.
  - Feed back distilled rules, not exemplar code; implementable as guidance keyed by diagnostic code
    in `schemas/linter-codes.json`. The paper credits this for helping the 1.3B model most.
  - Order repairs by severity, not by first diagnostic. A loop chasing the first parse error churns
    without improving safety.
  - Keep the expensive semantic check outside the loop, *labeling* the memory rather than prompting.
  - Require a quorum before negative examples — no avoidance hints until 3 positives exist.

- **Caveats.** Explicitly preliminary: no ablation, no embedded benchmark, no significance testing,
  two models in the treatment arm. The 500-entry cap and 0.90 dedup threshold are unjustified. CodeQL
  findings proxy vulnerability, not ground truth, and the 83.7% reduction partly reflects more
  programs compiling into analyzable shape. Nothing validates functional correctness.

## Typestate via Revocable Capabilities (Jia, Liu, He, Deng, Bao, Rompf; Purdue/Augusta, arXiv 2510.08889, Oct 2025)

- **Problem.** Scoped constructs (`synchronized`, `with`) are easy to reason about but impose LIFO
  lifetimes — a table lock cannot be released before a row lock it outlived. Flow-sensitive typestate
  gives fine-grained control but has demanded whole-program alias analysis, linearity, or explicit
  access-permission annotations.

- **Core mechanism (formal core).**
  - *Typestate encoding.* A state is an abstract **type member** of the resource class
    (`class File: type IsClosed; type IsOpen`), and a capability is a value whose type is
    **path-dependent** on the resource variable — `f.IsOpen`, `g.IsClosed`. For distinct `f` and `g`,
    `f.IsClosed` and `g.IsClosed` are unrelated types, so two files' states cannot be confused. This
    is DOT doing identity tracking that reachability/capturing types provably cannot: those
    qualifiers are over-approximations saying a capability *may* refer to `f`, too weak to enforce
    "must be the same file." Because state is a value, the state space is not a finite automaton — a
    type-level list of open elements encodes context-free bracketing for DOM construction, and
    match-type-computed duality encodes binary session types with de Bruijn-indexed recursion.
  - *Revocation, operationally.* A **destructive effect**: a result type carries `@kill(c)`, and
    applying it extends a flow-sensitively accumulated killed-set. After `close(fOpen)`, any later
    mention of `fOpen` is a compile error. Nothing happens at runtime — revocation is purely a
    typing-context operation, and the capability value is typically `()` erased to `Unit`, built by a
    cast inside the API. Unlike linear types, revocation is **opt-in and selective**: `write` uses a
    capability without killing it, so no linearity discipline is imposed and capabilities may be used
    any number of times in any order.
  - *What the type system tracks.* Three things, sequenced by evaluation order: (i) the killed-set,
    extended per effectful application, requiring a later use be free of transitive overlap with an
    earlier kill; (ii) reachability qualifiers — sets of variable names, the freshness marker ♦ for
    unnamed resources, or a self-reference for resources captured in closures; (iii) path-dependent
    types identifying which resource each capability belongs to. A one-shot function escaping the
    scope of a free variable it kills has that latent kill rewritten to a self-reference (a single
    static marker `FUN` for the innermost level).
  - *Aliasing story* — the load-bearing part. Capabilities are **not** linear, so aliases exist.
    Safety comes from a separation check at each use: the used term's qualifier must be **transitively
    disjoint** from the killed set. If `fC` may alias `fA` or `fB`, its qualifier is `{fA, fB, fC}`;
    closing `fA` makes the killed set `{…, fA}`, and `fC`'s transitive reachability intersects it at
    `{fA}` ≠ ∅, so writing through `fC` is rejected — though `fC` itself was never killed. `fB` stays
    fully usable. Killing therefore invalidates the killed variable *and everything that may reach
    it*, with no whole-program analysis because qualifiers are local. The authors relate this to CPS
    (reachability types alone forbid reuse of a revoked handle by requiring continuation and fresh
    handle to be disjoint) and describe direct-style effect tracking as the flow-sensitive version of
    borrowing. Theory is inherited from Deng et al. 2025, simplified by dropping the explicit `use`
    effect — usage is conservatively approximated by the mention information already in qualifiers —
    and omitting `move`. The alias substrate is "descriptive alias tracking" (Jia et al.), i.e.
    capturing or reachability types.
  - *Ergonomics.* Three arrows: `?=>` receives a capability implicitly (existing Scala), `=!>` kills
    its argument, `?<=` returns a capability implicitly into the caller's scope. The composite `?=!>?`
    reads "implicitly take S1, revoke it, implicitly return S2" — the signature shape of a transition.
    A constructor must return a resource *and* its initial capability with a type-level dependency, so
    results are bundled in a dependent pair `Sigma` (`type A`, `type B = a.IsClosed`) — a transient
    compiler-supported wrapper, not a general dependent pair, requiring immediate unpacking. A
    **type-directed ANF transformation** does that unpacking: any non-tail `Sigma`-typed expression is
    lifted into a fresh block where the resource is bound with a singleton type ascription and the
    capability declared implicit, so the newest capability has highest resolution precedence and
    shadows revoked ones. Implicit Σ-lifting further lets a callback return a plain value where a
    (value, capability) pair is required. Net effect: no explicit aliasing or permission annotations
    in user code.

- **Evidence.** No benchmark, no user study — a design paper with a Scala 3 compiler fork
  (destructive-effect checker as a phase after capture checking, re-typing the capture-checked tree
  and inserting kill annotations; ANF transform in the typer; capture-checker changes disallowing
  kills on boxed terms and requiring explicit qualifier polymorphism). The only quantitative claim is
  non-invasiveness: the existing 373-test capture-checking suite still passes. Expressiveness evidence
  is four compiling case studies — hand-over-hand table/row locking with non-LIFO release, file
  protocols, context-free DOM bracketing, and binary session types with duality and recursion. No
  soundness proof for this system; soundness is argued by reduction to prior formalisms. Stated
  limits: destructive effects on mutable variables and object fields unsupported, `Sigma` inexpressible
  in reachability types and bolted on, safe casts needed inside API implementations.

- **Implications.**
  - Typestate does not require linearity. For resource protocols in Vibra (file handles, effect
    handles, streams), the cheaper design is a kill-set plus a disjointness check on may-alias sets:
    permissive about aliasing and duplication, strict only at use-after-revocation.
  - State-as-type-member plus path-dependence is what supplies *identity*. Falsifiable: a scheme
    tracking only "some file is open" accepts a program that opens `a` and closes `b`; only per-path
    state types reject it. Test any Vibra typestate design against that swapped-resource case.
  - Its errors are ideal LLM repair signals — "use of killed variable `fOpen`", "expected
    `f2.IsClosed`, got `f1.IsClosed`" name variable, expected state, and actual state. Precisely the
    structured diagnostic that made paper 1's self-healing work.
  - Ergonomics were bought with implicit resolution and a compiler-inserted ANF pass, not more
    annotations. The model should write the transition; the compiler should thread the capability.
  - Context-free bracketing is checkable by a type-level list — relevant if Vibra ever needs
    statically checked well-formed nested emission.

- **Caveats.** Prototype in a research fork riding an experimental capture checker the authors note
  has no published end-to-end description. No performance or compile-time numbers, no evidence any
  human or model can write this style at scale. Mutable fields and variables — the common imperative
  case — are excluded. `Sigma` is admitted to be special-purpose. Everything requires a host with
  path-dependent types, implicit resolution, and capture tracking; a language without DOT-style paths
  can copy only the kill-set half, not the identity mechanism.

## Cross-cutting synthesis

**Agreements.** One architecture recurs in all five: a cheap step that over-produces, then a
machine-checkable gate that eliminates — Dafny's verifier, ProjAgent's LLM verifier behind a
5%-precision filter, a reranker behind a fast encoder, CodeQL/KLEE behind generation. Corollary:
evaluate first stages on *negative* precision, what they safely discard, not ranking quality. Second:
structure supplied up front dominates model scale — a method signature moved a 120B model 0% →
63.64%; a 600M contrastive embedder beat a 30B decoder by 85 points on Java xCodeEval; distilled
rules helped a 1.3B model most. Third: diagnostics are useful only when they name entities. Dafny
errors enabled 90.91% self-healing; ProjAgent's Python static analysis, forced conservative by dynamic
typing, was worth 0.9 points; the typestate system emits exactly the variable-and-state-naming errors
the first pattern wants. Static types are what make the feedback channel high-bandwidth.

**Contradictions.** (i) *Semantic depth vs. lexical signal.* ProjAgent argues surface similarity
misses useful context; Recall Before Rerank finds lightweight encoders beating top embedders on gapped
clones and identifier/comment stripping the worst transform. Likely reconciliation: abstraction helps
recall of unrelated-looking analogues and hurts precision on near-misses — arguing for hybrid scoring,
exactly what ProjAgent's own 0.5·BM25 + 0.5·dense stage does. (ii) *Retrieval vs. repair as the lever.*
The C paper's shared repair memory is order-dependent and unablated; the Dafny paper's largest jump
comes from a zero-retrieval intervention (a signature), its second from pure feedback.
Structure-then-feedback is better supported than retrieval. (iii) *Cost.* Recall Before Rerank treats
throughput as first-class and calls generative embedders Pareto-dominated; the other three run
10-iteration loops, per-step LLM verification, and agentic exploration without pricing any of it.

**Open questions for Vibra.** Does a language with essentially no pretraining corpus behave like Dafny
— near-total failure without a signature, sharp recovery with one? Most decision-relevant and cheap:
run the contextless/signature/self-healing ladder over a sample of `tests/*.vib` and measure deltas.
Does static typing convert ProjAgent's +0.9 feedback loop into something material — can a
non-conservative type-resolution check drive repair without false positives? What is the right quality
ladder (parses → types → lints → tests), and does severity-first repair ordering beat fixing the first
diagnostic? Is per-resource typestate worth its complexity for an LLM author, when the ergonomic payoff
came entirely from compiler-side implicit threading a YAML surface would have to reinvent? None of the
five evaluates a low-resource or non-textual-surface language, and none measures whether models can
*write* in the annotation styles proposed — only whether tools can check them.
