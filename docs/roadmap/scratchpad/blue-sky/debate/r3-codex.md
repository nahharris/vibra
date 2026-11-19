# Round 3: harness and participant setup

Status: Luna X1 writing (120 trials) and X6 reading (20 sessions) complete;
Haiku writing and reading independently graded. Human readability and X2–X5
remain unrun.

[measured] The standard-library Python 3.14.2 harness is in
[experiments/x1/harness/](experiments/x1/harness/README.md). Its 69 unit tests pass.
All four card examples parse into one canonical tree. Each of the nine complete
reference tasks also has one tree across D0/K/P/B, and every T10 prefix+suffix
reconstructs T8 byte for byte. The 40 reference trials pass all 1,764 case-runs:
441 distinct private cases across four surfaces. No reference file needed a
grammar or algorithm correction. This validates the experimental harness and
references, not a model-writing success rate.

[measured] [validation.json](experiments/x1/harness/validation.json) records tree,
source and private-fixture hashes. [reference-results.json](experiments/x1/harness/reference-results.json)
contains phase/case outcomes without copying private input/expected fixtures.
The runner accepts explicit external test paths and manifests, preserves raw,
extracted and assembled source hashes, and keeps structural instruction scores
separate from primary functional success. Fuel and executable contract faults
are distinct. It never executes generated source as Python.

[measured] The Luna setup uses `gpt-6-luna`, effort `low`, fresh ephemeral
processes, and no available tools. Its prompt audit contains only the fixed
one-line participant instruction and the user prompt. The successful non-task
echo probe returned ISOLATION_OK with 707 input tokens, 8 output tokens and
0 reasoning-output tokens. See [setup metadata](experiments/x1/harness/luna-setup/probe2/response.json)
and [launcher](experiments/x1/harness/luna_runner.py). This probe is excluded
from the scored cohort. Disabling feature flags alone was insufficient;
the copied local model metadata also disables the default tool and multi-agent
exposure. Credentials were not copied.

[argued] At setup, no hard 4,096-token output-request cap was verified in this
installed CLI. The launcher explicitly records `output_cap_enforced: false`.
Scored runs waited for the agreed amendment recorded below; neither runner
establishes matching enforced generation caps. Actual provider snapshot identity remains
unknown when only the requested model alias is reported.

[argued] I independently checked all 40 X6 keys against the contract meanings.
All agree. The eleven uniqueness answers have complete per-question arguments,
not merely bounded searches. c4's example fixes its shown input even though
the postconditions alone permit two results. The reduced 40-question allocation
for Hannah is accepted, with the within-person/item confounding stated; model
readers retain all eight contracts per surface/session. Details and the static
Haiku-runner audit are in [peer-artifact-audit.md](experiments/x1/harness/peer-artifact-audit.md).

[argued] Before scored calls: preserve immutable prompts/prefixes on resume,
retain failed assigned trials with the registered retry limit, verify the
artifact freeze, and record actual model/effort/cap metadata. Claude owns the
corresponding fixes in run_claude.py, completed before freezing.
No scored participant call, canonical spec
edit, commit, or push was made in this setup pass.

## Accepted budget amendment and freeze readiness

[argued] Both reviewers accepted an identical analysis threshold of 4,096
provider-reported aggregate output tokens, including reasoning once. Missing,
invalid or conflicting counts fail budget eligibility; they are not zero.
Raw functional success remains a separate column. Generation caps are
asymmetric: Luna has no verified hard request cap. This threshold does not
make generation budgets equivalent and may penalize surfaces differently.

[measured] The additional tests cover budget accounting, immutable resume
artifacts, two attempts across restarts, and reader scoring. All 69 tests and
all 40 reference trials / 1,764 case-runs pass. `run_luna.py` schedules the
registered 120 writing trials and 20 reader sessions with the same prompt
builders and seeded assignment order as the peer cohort. Each call is a fresh
isolated process; no tests or answer keys enter participant prompts. Interrupted
dispatch reservations consume an attempt, and completed wrong answers receive
no repair calls. Both public-artifact and harness-source freezes are verified
before dispatch. `budget.py` and `readers.py` retain raw and eligible scores.

[measured] `harness/freeze.json` pins all Python harness sources, isolation
assets, shared prompt producers and the audited X6 key builder. The peer
`x1/freeze.json` pins cards, API, tasks, schema, references and reader material.
No scored calls have occurred at this freeze-readiness checkpoint.

## Scored X1 writing results

[measured] Luna completed all 120 assigned trials on their first transport
attempt. Every output-token count is present and below 4,096; raw functional
and budget-eligible totals coincide. The longest call took 123.874 seconds,
completed normally and used 95 output tokens. No answer was repaired or
regenerated. Source/public freezes still match after the cohort. Raw prompts,
completions, attempt records and provider event streams are under
[runs/luna/](experiments/x1/runs/luna/manifest.json). Reproduce grading with
the frozen `runner.py`, the supplied external fixture directory, and then
`budget.py`; the resulting [analysis](experiments/x1/runs/luna/analysis-results.json)
preserves both endpoints.

[measured] I also graded the completed Haiku manifest independently with the
same frozen grader and budget policy. These totals reproduce Claude's report.
Haiku reports `claude-haiku-5-5`; Luna reports only the requested
`gpt-6-luna` alias, with no returned snapshot id. Both request their exposed
`low` effort. There are three fresh sessions per task/package/family.

| Package | Haiku primary / assigned | Luna primary / assigned | Haiku post-hoc T10 exclusion | Luna post-hoc T10 exclusion |
| --- | ---: | ---: | ---: | ---: |
| D0 | 20/30 | 24/30 | 20/27 | 24/27 |
| K | 27/30 | 24/30 | 27/27 | 24/27 |
| P | 20/30 | 21/30 | 20/27 | 21/27 |
| B | 25/30 | 24/30 | 25/27 | 24/27 |

[measured] Six Haiku trials exceed 4,096 reported aggregate output tokens:
T8 in D0 twice, P three times and B once. All six already failed functionally,
so applying the analysis limit changes no primary success count. No Haiku
count is missing. This does not demonstrate equally enforced generation caps.
Luna writing used 32,406 aggregate output and 336,402 input tokens. Its median
output lengths are D0 157.5, K 168.5, P 157.5 and B 139 tokens. Those are actual
Luna-provider counts, not cross-provider tokenizer estimates or selection
tiebreakers.

[measured] Luna phase totals by package are:

| Package | Extracted | Parsed | Checked | Full task | Secondary structural checks |
| --- | ---: | ---: | ---: | ---: | ---: |
| D0 | 30 | 29 | 25 | 24 | 25 |
| K | 30 | 30 | 24 | 24 | 24 |
| P | 30 | 22 | 21 | 21 | 21 |
| B | 30 | 29 | 26 | 24 | 26 |

[measured] Task-level vectors, phase totals, budget flags and both-family
comparisons are in [comparison-codex.json](experiments/x1/comparison-codex.json).
Luna succeeds on all T1–T5 trials in D0/K/B. K succeeds on all T6/T7 trials but
fails every T8 trial at constructor-label checking; D0 succeeds on all T8
trials but only one T6 and two T7 trials. P fails every T9 trial with unmatched
delimiters. K parses all 60 writing programs across the two families, but that
does not imply they all meet the task or type rules.

[argued] **The registered product-selection rule selects K provisionally.**
It is no worse than each competitor in either writing family and strictly
better in at least one: versus B it ties Luna and wins Haiku by two trials;
versus D0 it ties Luna and wins Haiku by seven; versus P it wins both.
This is the specified pilot dominance rule, not a significance test, a
confirmation of general superiority, or a claim that keyword delimiters alone
caused the result. These are four whole syntax packages. **Instead of locking
the final surface:** prototype statement-sequence K, retain explicit reopening
criteria, and use the registered ablations to examine its remaining failures.

### Paired task uncertainty

[measured] To make the task dependence visible, I computed exploratory
descriptive percentile intervals by resampling the ten task clusters 20,000
times (seed 20261115), retaining all three runs per surface in each selected
task and pairing packages within tasks. This exact interval algorithm was
chosen during analysis, not separately preregistered. These unadjusted
intervals describe this small convenience task corpus; they do not justify a
population or confirmatory inference. Differences below are percentage points
in full-task success, K minus the comparator.

| Comparison | Haiku difference [95% task bootstrap] | Luna difference [95% task bootstrap] |
| --- | ---: | ---: |
| K − D0 | +23.3 [+6.7, +43.3] | 0.0 [−26.7, +23.3] |
| K − P | +23.3 [+3.3, +46.7] | +10.0 [−13.3, +36.7] |
| K − B | +6.7 [0.0, +16.7] | 0.0 [−20.0, +20.0] |

[argued] The broad Luna intervals and small K/B difference are material limits
on the provisional choice. Equal aggregate scores conceal task crossovers;
none of this establishes equivalence. **Instead:** preserve both family/task
tables and require a separately designed confirmation before making a broad
syntax-superiority claim.

### Post-hoc T10 artifact defect

[measured] Claude identified a defect in his frozen continuation task after
generation: its statement never names the required `parse-args` entry, and
every prefix stops before that entry is declared. All packages score 0/3 in
both families. The three Luna continuations that pass parsing/checking all
reach evaluation and fail with `unknown-name`; the other nine already fail
parsing or checking. Thus the omitted entry explains a real scoring failure,
but must not be claimed as the sole cause of every Luna T10 failure.

[argued] I accept Claude's **post-hoc sensitivity analysis**, explicitly
labeled: retain all T10 failures in the preregistered primary endpoint and
also display the whole-task exclusion shown above. There is no source repair,
rerun or rescoring. Removing an equally failed task leaves the dominance
ordering unchanged. The T10 row cannot support a claim about syntax recovery.
**Instead:** register an explicit entry signature and independently validate
the task instructions before any future held-out continuation cohort.

### Failure audit and implications

[measured] I inspected representative Luna failures against the frozen cards
and task statements. The named-function failures in T6 include a helper used
before its nested declaration and `(make-share)` called with no argument
instead of passed by name. The K T8 programs use unlabeled arguments for
field-bearing variant constructors. P has unclosed delimiters in eight
programs. These examples are genuine violations of the registered language,
not reasons to repair participant output. All failures and diagnostics remain
in the raw analysis.

[argued] K's perfect parsing score does not solve constructor naming or
scope comprehension. These errors support testing declaration-controlled
labels and clearer callable-value/scope examples, but do not isolate the
effect of any one syntax choice. **Instead:** retain frozen X1 as the baseline,
run X3/X4 with matched controls, and keep diagnostics specific enough for a
later separately registered repair arm. Do not weaken the checker post hoc.

## Scored Luna X6 comprehension

[measured] All twenty Luna reader sessions completed on their first attempt,
with valid token counts below the threshold. The forty questions are repeated
within each session and across the five sessions per surface. Raw and eligible
scores agree; the reader cohort used 5,700 aggregate output tokens. Raw answers,
prompt order and metadata are in [x6/runs/luna/](experiments/x1/x6/runs/luna/manifest.json);
question/session scoring is in [analysis-results.json](experiments/x1/x6/runs/luna/analysis-results.json).

| Package | Correct / 200 answers | Correct per session (out of 40) |
| --- | ---: | --- |
| D0 | 178 | 35, 36, 38, 35, 34 |
| K | 180 | 36, 36, 36, 36, 36 |
| P | 183 | 37, 38, 35, 37, 36 |
| B | 187 | 35, 38, 38, 38, 38 |

[measured] Across all packages Luna answers all 200 excluded-call (A) and
all 180 forbidden-result (B) questions correctly. It answers only 166/220
forced-result (C) and 182/200 underdetermined-result (D) questions correctly.
For `can-ship(2, 3) = false`, 18/20 sessions incorrectly say another result
is allowed: the contract forces false because true would require adequate
stock. For the exact-multiple rounding question, 13/20 sessions miss that the
two strict distance bounds force the exact multiple. This is a recurring
entailment problem despite high overall accuracy. The audited key remains
unchanged.

[argued] B has the highest descriptive Luna reader accuracy; K has no measured
comprehension superiority here. Neither model is a substitute for Hannah's
human result. The writing pilot's product rule and the comprehension endpoint
must remain separate. **Instead:** keep K provisional, expose implication and
uniqueness clearly in claim tooling, and require the independent human review
before final surface lock. Do not infer that a
person can safely trust a witness nearly blindly from these model scores.

[measured] Haiku's completed X6 cohort independently grades at 200/200 for each
of D0/K/P/B: every one of its twenty sessions answers all forty questions
correctly, with no missing usage or budget exclusion. Results are in
[x6/runs/haiku/analysis-results.json](experiments/x1/x6/runs/haiku/analysis-results.json).
The 120 matched writing prompts and twenty matched reader prompts are byte
identical between families, confirmed in
[prompt-parity-codex.json](experiments/x1/prompt-parity-codex.json).

[argued] Haiku's perfect result has a ceiling on this material and cannot rank
the surfaces. Luna's contrasting errors are evidence against treating either
named weak model as a generic human proxy. **Instead:** keep each cohort's
outcomes separate, complete Hannah's frozen session, and use a new preregistered
contract set if a later study needs to distinguish model comprehension further.

[argued] X2–X5, the repair arm, and Hannah's reading session have not been run
in this pass. They must not acquire measured verdicts from X1/X6. The new
language implementation, canonical specification, commits and pushes remain
outside this experiment's authorization.

## Cross-review and arm scheduling

[argued] The subsequent [cross-review](r3-codex-cross-review.md) checks Claude's
report, corrects overstatements about reading differences/noninferiority/error
causes, and defers X2–X5 plus the repair arm with explicit defaults and reopening
triggers. That scheduling decision supersedes the earlier suggestion here to
run X3/X4 next; it supplies no new measured verdict for those unrun treatments.
