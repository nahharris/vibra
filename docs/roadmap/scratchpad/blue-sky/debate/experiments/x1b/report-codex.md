# X1b: Codex results and decision

Status: both 81-trial cohorts complete and graded; the registered primary rule
retains K. The result is materially sensitive to the output-budget endpoint.

## What ran

[measured] The accepted registration was executed: T1–T9, K/K2/K3, three fresh
sessions per task/surface/family, 81 trials per family and 162 total. Every
trial in both cohorts completed on its first transport attempt. All usage
counts are available. Haiku reports `claude-haiku-5-5`; Luna exposes the requested
`gpt-6-luna` alias but no returned snapshot identity. Both request low effort.
No participant received tools, private fixtures, earlier answers or repair
feedback. Actual provider generation seeds are not claimed.

[measured] All 81 matched prompt files are byte-identical between families;
see [prompt-parity-codex.json](prompt-parity-codex.json). The two shared API
edits are exactly the accepted surface-neutral wording changes. The original
X1 sources/public freeze still match, as do the new 33-artifact public freeze
and 77-asset source freeze. The source freeze includes the accepted registration,
the printer/golden, both producers and every imported original helper.

[measured] Before dispatch, nineteen X1b tests and all sixty-nine original
tests passed. All three card examples and all 27 reference programs match
the immutable frozen K trees. References pass all 1,116 external case-runs;
the separately hand-written golden checks all three trees and four explicit
order/field-permutation cases. These are oracle validations, not participant
success observations. [validation.json](validation.json) and
[reference-results.json](reference-results.json) retain their evidence.

[argued] The tested K2/K3 use **labeled constructor calls**, with braces only
for typed declaration shapes. Brace construction was withdrawn before any
measurement and was not an extra surface. Tuple/dictionary/anonymous-record
semantics, prefix type application, naming changes and the deferred X2–X5/
repair arms were not tested. Do not assign them measured verdicts from X1b.

## Primary and raw functional endpoints

[measured] Each cell below has 27 assigned trials. Primary success includes
the registered aggregate-output eligibility limit; raw functional success
omits that budget filter but retains extraction/type/runtime/case failures.

| Surface | Haiku primary | Haiku raw functional | Luna primary | Luna raw functional |
| --- | ---: | ---: | ---: | ---: |
| K | 27/27 | 27/27 | 21/27 | 21/27 |
| K2 | 26/27 | 26/27 | 22/27 | 22/27 |
| K3 | 26/27 | 27/27 | 24/27 | 24/27 |

[measured] The frozen [selection implementation](summarize.py) returns
`selected: k`, `basis: retain-baseline`, `eligible: []`. Both challengers score
one below K in Haiku's primary endpoint, so neither meets the preregistered
requirement of no lower primary success in either family. Luna's gains alone
cannot satisfy that rule. The complete task/phase/uncertainty tables are in
[comparison-codex.json](comparison-codex.json).

[argued] **Retain K as the provisional prototype under the accepted primary
rule.** This is not a proof that section colons or commas improve writing,
that K is noninferior in a population, or that K3 is unsuitable. The fresh
Luna control is 21/27, compared with the earlier X1 K's 24/27 after the labeled
T10 exclusion; the prompts/API and runs are different, so do not pool those
controls or attribute the change to a particular wording edit. **Instead:**
keep this decision and both endpoints visible, with the limited corpus and
generation variability explicit.

### The cap is decisive, not incidental

[measured] Two Haiku outputs exceed 4,096 aggregate output tokens:

| Trial | Output tokens | Raw functional result | Primary result |
| --- | ---: | --- | --- |
| `haiku-k2-t8-r2` | 4,433 | Parse failure | Failure |
| `haiku-k3-t8-r1` | 5,212 | Passes all functional cases and structural checks | Budget-ineligible failure |

[measured] No Luna output exceeds the limit. No trial in either cohort has a
missing/unverifiable budget count. The 5,212-token K3 exclusion is the entire
Haiku primary difference between K and K3. Both generations retain the agreed
asymmetric/unverified request-cap conditions; an identical analysis threshold
does not make the generation limits equivalent. Reasoning is included once
in the provider aggregate; no subset is added again.

[measured] Applying the same selection formula to the **raw** endpoint would
make K3 the only eligible challenger: it ties Haiku K at 27 and improves Luna
from 21 to 24. [sensitivity-codex.json](sensitivity-codex.json) records that
calculation. Raw functional success was prespecified for reporting; applying
the adoption formula to it is a **post-hoc sensitivity**, not the registered
selection or a rescoring of any trial.

[argued] I do not discard the cap after seeing it affect the choice, and I do
not describe the primary result as a demonstrated syntax loss for K3. Both
would hide information. **Instead:** retain the primary K decision, label it
cap-sensitive, publish the raw K3 advantage, and require a separately registered
budget/endpoint decision before using different criteria to choose a surface.
This report does not call for another immediate round of generation.

## Task dependence, phase failures and instruction following

[measured] Every task other than T6/T8 passes 3/3 in all packages and families.
The differentiating task counts are:

| Family / task | K primary | K2 primary | K3 primary |
| --- | ---: | ---: | ---: |
| Haiku T6 | 3 | 3 | 3 |
| Haiku T8 | 3 | 2 | 2 |
| Luna T6 | 0 | 0 | 1 |
| Luna T8 | 0 | 1 | 2 |

[measured] Phase and structural totals, before the separate budget filter:

| Family / surface | Extracted | Parsed | Checked / examples passed | Raw functional | Secondary structural |
| --- | ---: | ---: | ---: | ---: | ---: |
| Haiku K | 27 | 27 | 27 | 27 | 27 |
| Haiku K2 | 27 | 26 | 26 | 26 | 26 |
| Haiku K3 | 27 | 27 | 27 | 27 | 26 |
| Luna K | 27 | 26 | 21 | 21 | 21 |
| Luna K2 | 27 | 26 | 22 | 22 | 22 |
| Luna K3 | 27 | 27 | 24 | 24 | 24 |

[measured] Haiku K3's T5 run 1 passes the functional cases while omitting the
requested map/fold structure. That is a secondary instruction-following
failure, retained separately as registered; it does not silently become a
primary failure. Conversely the correct over-budget K3 T8 program meets the
structural checks and fails only budget eligibility.

[measured] I inspected representative Luna failures against the cards.
T6 includes calling helpers before their local declaration, a top-level helper
incorrectly referring to another function's local `base`/`remainder`, and an
extra closing `endfn`. T8 repeats unlabeled arguments to field-bearing user
constructors; one K2 program is missing a block closer. The Haiku K2 failure
reports a mismatched delimiter. These failures remain in the frozen grader;
no parse repair, helper hoisting, label insertion or scope weakening was used.

[argued] The task concentration means that almost all of the apparent surface
difference comes from two known tasks. K3's better Luna parsing/checking is an
observed pilot finding, not a general statement about comma-free grammars or
human comfort. Constructor-label and scope conventions remain important costs
under all three cards. **Instead:** preserve the task-level evidence and keep
the separately deferred label/callback questions unresolved.

## Descriptive paired task uncertainty

[measured] The frozen analyzer resamples nine paired task clusters 20,000 times
(seed 2026111502), retaining all three observed runs per task/surface. The
unadjusted 95% percentile ranges below concern primary-success differences
in percentage points, K3 minus comparator:

| Comparison | Haiku difference [task bootstrap] | Luna difference [task bootstrap] |
| --- | ---: | ---: |
| K3 − K | −3.7 [−11.1, 0.0] | +11.1 [0.0, +25.9] |
| K3 − K2 | 0.0 [0.0, 0.0] | +7.4 [0.0, +18.5] |

[argued] These are descriptive intervals conditional on the observed three
draws per task, not a model of every source of generation or population
variation. Haiku K2/K3 have identical *per-task primary counts*, producing a
degenerate zero interval; that does not establish equivalence. The corpus is
reused and mostly at ceiling. **Instead:** report those limits alongside the
numbers, with no significance/noninferiority inference or pooled-family score.

## Reproduction and handoff

[measured] Run `grade.py --manifest runs/<family>/manifest.json --tests-dir`
with the previously supplied external fixture directory to verify both
freezes/private hashes, recreate `functional-results.json` and apply the
unchanged budget policy to `analysis-results.json`. Then run
`python -B summarize.py --output comparison-codex.json`. The producer's raw
prompts, completions, reservations, attempts, event/usage metadata and manifest
remain under each `runs/` directory. Haiku used 87,062 aggregate output tokens
and Luna 21,457; medians by package are in the comparison file, not used as
tiebreakers. Original and X1b freezes were reverified after grading.

[argued] The ledger can record: **K retained by the registered primary rule;
K3 has a raw-functional advantage in this pilot and the primary selection is
cap-sensitive; no measured verdict on withdrawn brace construction, deferred
type/literal features or human K2/K3 comprehension.** The human X6 collection
is complete with its earlier single-reader limits. **Instead of presenting
this as broad convergence on punctuation:** keep these precise findings and
the remaining naming/mechanism defaults separate before any specification
rewrite.
