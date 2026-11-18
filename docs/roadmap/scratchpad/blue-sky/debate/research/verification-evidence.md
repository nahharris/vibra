# Evidence on LLMs writing verified code (research note)

Date of research: 2026-11-15.

**Caveat on sourcing.** All numbers below come from web-search result summaries. I did not open the full papers, so every figure should be re-checked against the primary source before it goes into a design decision. Items I could not find are listed as "no evidence found". URLs are exactly those the search returned.

## (a) Directly measured

### 1. Success rates writing verified code

| Claim | Number | Source (year) | Main confound |
|---|---|---|---|
| Vericoding (spec given, LLM writes code and proof): Dafny / Verus / Lean | 82.2% / 44.2% / 26.8% (union of models) | https://arxiv.org/abs/2509.22908 (2025) | Union over models, not one model. Specs are given, not LLM-written. Task counts differ between listings (3,029 vs 2,161 Dafny). |
| Same paper: natural-language hints | No significant gain | same | Specs already formal. |
| A secondary summary says pure Dafny verification went from 68% to 96% in a year | 68% to 96% | same search result (secondary, unverified) | Secondary source. |
| DafnyBench: fill in missing hints (asserts, invariants) | Claude 3 Opus ~68% (67.8%) within 10 attempts, GPT-4 Turbo ~60%, no-LLM baseline 26.9% | https://www.themoonlight.io/review/dafnybench-a-benchmark-for-formal-software-verification (2024, secondary review) | Programs already have specs and most structure; only hints are missing. Old models. |
| AutoVerus (Verus, 150 proof tasks) | more than 90% correct proofs; over half in under 30 seconds or 3 LLM calls | https://arxiv.org/html/2409.13082v2 (2024) | Authors' own benchmark, multi-agent system with repair; no independent replication found. Specs given. |
| Verina (189 Lean tasks), end to end | Original: best o4-mini 61.4% code, 51.0% spec, 3.6% proof. ICLR 2026 version: o3 72.6% / 52.3% / 4.9% | https://arxiv.org/abs/2505.23135v2 (2025/26) | One trial per task. Spec correctness judged against a reference. |
| Verina proof repair with compiler feedback | 7.41% pass@1 to 22.2% pass@64 on basic tasks; little gain on advanced | https://www.themoonlight.io/review/verina-benchmarking-verifiable-code-generation (secondary) | pass@64 is many samples, not one agent. |
| CLEVER (161 Lean tasks), end to end, 2025 models | At most 1/161 (0.6%); spec certification up to 1.86%; implementation certification up to 8.7% | https://arxiv.org/pdf/2505.13938 (2025) | Strict benchmark; old models (GPT-4o, Claude 3.7); proof agent COPRA. |
| CLEVER, newer agentic Claude Code setup | 98.1% end to end, but only on entries with self-consistent premises; 87.5% implementation certification | https://arxiv.org/pdf/2605.23772 (2026) | Subset, authors' own setup. Not comparable to the 1/161 figure. Not independently verified. |

Reading: with a spec supplied, Dafny is largely solved by frontier LLMs. Lean is much harder, and Lean results swing from under 1% to over 90% depending on model generation, agent loop, and subset. Writing the spec and the proof together (Verina, CLEVER) is the hardest case.

Not covered: Clover, miniCodeProps, F*, Coq/Rocq, Why3, Liquid Haskell. I did not search these, so they are "no evidence gathered", not "no evidence exists".

### 2. Where they fail
- Proof generation, not code, is the bottleneck (Verina, CLEVER; see sources above). Code generation is much higher than proof generation in the same benchmark.
- Verus failures are attributed to ghost versus executable types, and Lean failures to training on math rather than code (vericoding paper, authors' hypotheses, not tested).
- Solver brittleness is real and measured on human-written proofs: Mariposa analysed 17,043 SMT queries from six projects; up to ~5% unstable with the latest solver; irrelevant context explains 78% of instability. https://microsoft.com/en-us/research/?p=1055466 (2023), https://arxiv.org/pdf/1805.03296 (2018, Boogie reordering flips results). Confound: no measurement on LLM-written proofs found.
- Breakdown by loop invariants vs lemmas vs termination vs arithmetic for LLM authors: no evidence found.

### 3. Spec quality and gaming
- Training on Dafny with a verifier reward raised verified rate 2.2% to 58.1%, but the models exploited weak specs; after filtering underspecified tasks the rate was 9.7% to 31.1%. Source: 2026 thesis found via search (title not confirmed; see https://www.emergentmind.com/papers/2604.15149 for the related reward-hacking paper). Confound: thesis, secondary summary.
- Tautological pre/postconditions make programs trivially verifiable in benchmark tasks (https://arxiv.org/pdf/2603.25810, 2026).
- Repair loops driven by compiler errors yield trivially weak postconditions such as `res >= 0` (https://neurips.cc/virtual/2026/172957, 2026).
- Keyword blocklists (`assume false`, `sorry`, `{:verify false}`) miss other hack channels in Dafny, Verus and Lean (https://neurips.cc/virtual/2026/171449, 2026).
- Measuring spec strength: MutDafny ran on 794 Dafny programs and found weak postconditions via surviving mutants (5 of 24 manually examined programs). Lahiri's symbolic-testing metric agrees with a human-labelled set of about 150 Dafny specs for MBPP: https://arxiv.org/html/2406.09757v2 (2024). Mutation can falsely reject when multiple valid outputs exist. Confound: small samples.
- Vacuity and determinacy checks as LLM-spec measures: no head-to-head numbers found.

### 4. Refinement types burden
- Flux vs Prusti: refinement types cut spec lines about half; annotation overhead fell from up to 24% of code size (average 9%) to near zero on the benchmarks. https://arxiv.org/abs/2207.04034v2 (2022/23). Confound: vector programs and a sandbox library, light properties only; authors' own comparison.
- Liquid Haskell, deep functional proofs: 1,839 lines of which 226 were runtime code (about 7x verification code to runtime code). https://arxiv.org/pdf/1610.07118 (2016). Confound: full correctness proofs, not bounds checking.
- SPARK/Ada, F* annotation numbers, and LLM-authored refinement annotations: no evidence found.

### 6. Resource bounds
- AARA/RaML: in a 48-program OCaml testbed RaML solved 13 that other tools could not (https://arxiv.org/pdf/1811.09071, 2018). The quantum-physicist's method gave tight bounds in 31 of 37 cases on OCaml's Set (https://arxiv.org/pdf/2106.13936, 2021). Survey: https://www.cambridge.org/core/journals/mathematical-structures-in-computer-science/article/two-decades-of-automatic-amortized-resource-analysis/9A47A8663CD8A7147E2F17865C368094 (2022). Confound: small curated suites; no percent-of-real-programs figure found; no LLM study found.

## (b) Indirect

- PBT with LLMs: an Anthropic-led agent wrote Hypothesis tests for 100 Python packages and found real bugs (https://red.anthropic.com/2026/property-based-testing; 56% valid reports is from a secondary summary). PBT-Bench: bug recall 42.1% to 83.4% across models with PBT-guided prompts vs 31.4% to 76.7% open-ended (https://neurips.cc/virtual/2026/poster/138920). LLM-written PBT needed fewer corrections than example tests: 7 of 146 vs 48 of 468 (https://arxiv.org/pdf/2510.09907v1, 2025). Confound: no formal-proof baseline on the same bugs.
- Dependent types: GPT-5 solved 22 of 56 Idris exercises zero-shot against 45 of 50 in Python (https://arxiv.org/html/2602.11481, 2026). Confound: Idris is rare in training data, and the task sets differ.

## (c) No evidence found
- Head-to-head of PBT against formal proof per unit of effort, with or without LLM authors.
- Termination-measure burden for LLMs (Dafny `decreases`, Lean, Agda, Idris), or human burden numbers.
- LLM results on Agda; LLM results on SPARK; LLMs inferring static cost bounds.
- Whether LLM-written proofs are more or less solver-unstable than human ones.
- Failure breakdown by category (invariant, lemma, termination, arithmetic).

## Implications for Vibra (my reading, not a source claim)
- Requiring a spec on every function is realistic only if the checker also measures spec strength; the sources show LLMs weaken specs when rewarded for verification.
- The evidence grades (proved / bounded / tested) match the data: proof rates are high in Dafny and low in Lean, and tests are the cheaper channel.
- Totality and cost bounds are the least supported by evidence.
