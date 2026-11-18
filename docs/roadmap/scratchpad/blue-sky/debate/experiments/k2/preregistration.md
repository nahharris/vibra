# K versus K2: follow-up registration proposal

Superseded before freeze by
[X1b registration](../x1b/preregistration.md), covering K/K2/K3 and withdrawal
of brace construction. The original proposal below is retained as history;
its 108-trial matrix and two-surface rule are not the current proposal.

Status: proposed by Codex after human X6; exact K2 card/grammar and peer
acceptance pending. **Not frozen; scored calls await grammar, parity and freeze checks.**

## Question and treatment

[argued] Compare the provisional K writing package with K2, motivated by one
human reader's reported initial punctuation/section confusion. K2 removes
section colons and groups the ordered typed parameter list in braces. A final
card must state any corresponding record/case constructor changes explicitly.
This is a bundled package comparison, not a causal colon ablation or a test of
human comprehension. Names, types, API behavior, contracts, evaluation, scope
and task requirements otherwise stay fixed.

[argued] Pending grammar decisions: record/case field declarations and value
constructors; statement and value `if` sections; match-case delimiters; zero
parameters and unit cases. Typed parameter names keep their colons. The new
parameter notation does not introduce an anonymous-record type or a record
argument: ordinary calls still supply the same ordered positional arguments.
K2 accepts only its frozen grammar, not K/K2 mixtures.

## Assignment matrix and prompts

[argued] Nine unchanged X1 task statements, T1–T9; exclude defective T10
prospectively. Two surfaces, three fresh-context runs per task/surface/family,
two model families: Haiku and Luna at their exposed low effort. This is **54
assignments per family, 108 total**, with no shrinking after outputs. Use
seed `2026111502` for the independently shuffled assignment order, with the
same construction/shuffle algorithm across families. Do not claim provider
generation seeds are controlled.

[argued] Run **fresh K controls**. Prompts use one card, shared API and one
task; T9 receives the matched rendering of the same supplied T8 reference AST.
Any clarification of examples, binders, division or scopes is identical in
both cards/API and explicitly frozen. No other competing surface, earlier
response, human answers, model-reading answers, hidden tests or repair
feedback enters a participant context. Preserve exact prompt/reference bytes.

[argued] Retain the previous isolation, two-attempt transport limit across
restarts, immutable prompt/attempt records and no repair of completed answers.
Record provider alias/snapshot where available, actual effort, finish status,
usage and unavailable fields. Use the same accepted analysis eligibility:
aggregate output including reasoning once at most 4,096; missing/invalid or
conflicting counts fail eligibility; raw functional success remains separate.
Generation caps stay explicitly asymmetric/unverified as appropriate.

## Oracle and validation before dispatch

[argued] Reuse the independently authored external T1–T9 fixtures by their
frozen hashes. They remain private and absent from participant tools/prompts.
No new fixtures or behavior are chosen after inspecting this arm's outputs.
Per task, K and K2 references must parse to the same canonical tree, check and
pass the same tests. Their matched card example must also produce one tree.

[argued] The extension lives here, outside `x1/harness/`, and pins the original
shared checker/evaluator and all imported helpers. Add strict positive/negative
grammar tests, including nested branches/functions, nested generic types,
typed/labeled fields versus section punctuation, comments/text containing
colons, duplicate fields, unit cases and zero-parameter functions. Confirm
rejection of unregistered mixed syntax. **Do not** rewrite responses by string
replacement as an extraction or repair policy. Keep X1's extraction rule and
the separate structural-instruction score.

[argued] Freeze the accepted registration, cards, API, task texts, both sets
of references, parser/runner/tests, shared source hashes, model-isolation
assets and fixture hashes before any scored call. Verify the old X1 freezes
remain unchanged. New sources require a new freeze, never replacement of the
completed X1 record. Both reviewers inspect parity/negative controls first.

## Endpoint and proposed decision rule

[argued] Primary endpoint: assigned-trial full-task success including extraction,
parse/check/example/runtime failures and budget ineligibility. Report raw
functional success, every phase, task-level outcomes, actual token counts and
resource/transport failures separately. Nine task clusters, not hidden cases,
are the unit for descriptive paired uncertainty.

[argued] Proposed adoption rule, to accept before dispatch: K2 replaces the
provisional K package only if its success is no lower in either named family
and strictly higher in at least one. If K dominates, retain K. An exact tie or
crossover retains K as the already selected reversible default, with no token,
card-size or taste tiebreaker. This is a small pilot product rule, not a
significance, equivalence or noninferiority test. Report that T1–T9 are reused
known tasks and that the treatment was proposed after inspecting X1/human
results; this is not a held-out confirmatory test.

[argued] No writer result establishes that K2 improves human readability. That
claim would need a separately registered human design with fresh/counterbalanced
items. The existing X6 observations and the original K package decision stay
in the historical record whatever this arm finds.

## Ownership and next step

[argued] Proposed split: Claude writes the complete K2 card and matched K/K2
public artifacts/references and runs Haiku; Codex audits them, adds the isolated
K2 parser/runner, verifies common-tree/oracle parity, and runs Luna. Both use
the accepted registration and new freeze. The next required artifact is the
exact K2 card; parser implementation and scored generation wait for it.
