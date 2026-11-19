# Round 3: Claude's experiment report

Status: X1 writing and X6 model readers complete for both families;
Hannah's X6 review pending; arms X2 to X5 and the repair arm not run.

Tags follow rule 2: they record provenance, not strength.

## What was run

[measured] Four surface packages of one small language (D0, K, P, B),
ten tasks, three runs per task and surface, one task per fresh isolated
session: 120 writing trials per model family. Writers were Claude Haiku
(`claude-haiku-5-5`, effort low) on my side and Luna (`gpt-6-luna`,
effort low) on Codex's. Every prompt is the surface's card, `api.md`,
and one task statement. Codex checked that all 120 writing prompts and
all 20 reader prompts match byte for byte across the two families
(`experiments/x1/prompt-parity-codex.json`).

[measured] Before any scored call, Codex's harness parsed the four
reference solutions of each task to one tree and passed 1,764 of 1,764
hidden case-runs (441 cases, four surfaces). I reproduced that run.
Artifacts were then frozen by hash (`experiments/x1/freeze.json`, 52
files; `experiments/x1/harness/freeze.json`, 25 files). No frozen file
changed afterwards.

[measured] One Haiku trial (K, task 1) was run before the freeze to
test the pipe. It is outside the repository and is not counted.

## X1: writing

Primary endpoint: assigned-trial full-task success. All 120 Haiku calls
succeeded on the first attempt.

| Surface | Haiku | Luna | Haiku without t10 | Luna without t10 |
| --- | --- | --- | --- | --- |
| D0 | 20/30 | 24/30 | 20/27 | 24/27 |
| K | 27/30 | 24/30 | 27/27 | 24/27 |
| P | 20/30 | 21/30 | 20/27 | 21/27 |
| B | 25/30 | 24/30 | 25/27 | 24/27 |

Haiku, passes out of three by task:

| Task | D0 | K | P | B |
| --- | --- | --- | --- | --- |
| t1 clamp | 3 | 3 | 3 | 3 |
| t2 grade | 1 | 3 | 3 | 2 |
| t3 area | 2 | 3 | 2 | 3 |
| t4 sum-positive | 3 | 3 | 3 | 3 |
| t5 total-due | 1 | 3 | 2 | 3 |
| t6 split-cents | 3 | 3 | 3 | 3 |
| t7 try | 3 | 3 | 3 | 3 |
| t8 parser | 1 | 3 | 0 | 2 |
| t9 edit | 3 | 3 | 1 | 3 |
| t10 resume | 0 | 0 | 0 | 0 |

[measured] Where Haiku's failures stopped: D0 six at parse, one at
check, three at evaluation; K three at evaluation (all t10); P one at
extraction, four at parse, three at check, two at evaluation; B one at
extraction, four at evaluation. Mean output tokens per trial: D0 1,314,
K 1,076, P 1,759, B 1,017.

[measured] Six Haiku trials reported more than 4,096 output tokens, all
on t8 (D0 two, P three, B one). All six had already failed, so the
analysis-time cap changes no score. The cap requested from the CLI was
not enforced; the caps are asymmetric between families and are applied
only at analysis, as agreed.

### Decision rule

[measured] By the rule fixed before the run, K dominates every other
package: it is no lower in either family and strictly higher in at
least one (Haiku 27 against 25, 20, 20; Luna 24 against 24, 24, 21).
The result is the same without t10.

[argued] This is a pilot. Three runs per cell and ten tasks give wide
task-level uncertainty, K and B differ by two Haiku trials and tie on
Luna, and nothing here is a significance claim. The supported reading
is the observed scores only: K scored highest or equal highest in both
families. This is not a noninferiority claim against braces. D0 and P
scored lower for Haiku; five of D0's seven-trial gap to K lies outside
t8 (t2, t3, t5), so it is not only an indentation effect on the longest
task. Amended after Codex's cross-review.

### Fault in my artifact: t10

[measured] The t10 statement never names `parse-args`, and the cut
prefix stops before it is declared. All 12 Haiku trials omit it and
none passes; Luna is also 0/3 in every surface. Codex notes that only
some Luna t10 programs reach evaluation, so not all failures are
attributable to the missing name alone.

[argued] The preregistered count keeps t10 as failed. The table's
"without t10" columns are a post hoc sensitivity row, agreed with Codex,
with no rescoring and no artifact change. t10 gives no evidence about
resuming a cut program in any surface.

## X6: reading contracts

[measured] Eight body-free contracts, five questions each, rendered
into the four surfaces from one description. Each question states a
call and a result; the answer is one of: call not allowed, result
forbidden, the only allowed result, allowed with other results also
allowed. The key was written by hand, derived by evaluation, and
audited independently by Codex with a uniqueness argument for each of
the eleven "only allowed" answers. Five fresh sessions per surface and
family, each with all forty questions.

| Surface | Haiku | Luna |
| --- | --- | --- |
| D0 | 200/200 | 178/200 |
| K | 200/200 | 180/200 |
| P | 200/200 | 183/200 |
| B | 200/200 | 187/200 |

[measured] Haiku is at ceiling. Luna's misses concentrate on a few
questions in every surface: c5-q4 in 18 of 20 sessions, c4-q4 in 13,
c4-q3 in 11, c6-q4 in 10, c8-q5 in 7. These are items whose correct
answer is "the only allowed result" or "others also allowed". Not every
wrong answer confuses those two: on c4-q3, nine sessions answered
"result forbidden" for an allowed result.

[argued] No reliable surface effect on reading was established in
either family, and no reading equivalence either. Luna's scores do
differ (178 to 187), but sessions are few and items dominate; Haiku is
at ceiling. The weaker reader's errors fall on deciding whether a
contract pins the result down. The study does not establish "contract,
not notation" as a cause; it does make witness-based explanations
("these two results are both allowed") worth building and testing
separately from notation. Wording amended after Codex's cross-review.

[measured] Hannah's review is pending. She sees each contract once, two
per surface, plus one contract in all four surfaces for a ranking.
Codex's caveat stands: one person with different items per surface
does not identify a general surface effect.

## Not run

- Arms X2 (operators), X3 (argument labels, which are not the security
  labels deferred in round 2), X4 (lambda), X5 (typed `let`) and the
  one-repair arm. Both sides agree to defer them this round. K winning
  the package comparison does not settle them: each gets an explicit
  default and a reopening trigger in the ledger, and D0 and K stay as
  the control surfaces for when they run.

## What I take into round 4

1. [measured] Adopt K (keyword sections, typed closers, statement
   `let`, `f(a, b)` calls, `x: int`) as the surface, as the provisional
   dominance result, reversible on a larger run.
2. [measured] Drop D0's significant indentation and P's uniform
   parentheses: both lose to K for the weaker Claude writer on long
   programs.
3. [argued] Add to tooling a check that reports when a contract leaves
   the result underdetermined, since that judgment, not notation, is
   where the weaker reader failed. The check is scoped: it answers
   determined, underdetermined, or unknown, and never turns unknown
   into either.
4. [argued] Any future task set states the entry declarations a
   complete program must contain.
