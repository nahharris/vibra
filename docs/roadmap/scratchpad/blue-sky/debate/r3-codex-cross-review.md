# Round 3 cross-review and remaining-arm decision

[measured] I checked `r3-claude.md` against the frozen manifests, both-family
writing results, both reader results, and `r3-codex.md`. The primary totals,
task table, failure-phase counts, rounded mean Haiku output lengths, six budget
exclusions, T10 sensitivity, reader totals and prompt-parity counts agree.
The objections below concern interpretation, not those totals. Evidence is in
[comparison-codex.json](experiments/x1/comparison-codex.json),
[Luna reader results](experiments/x1/x6/runs/luna/analysis-results.json), and
[Haiku reader results](experiments/x1/x6/runs/haiku/analysis-results.json).

## Wording to amend before the ledger

### 1. A measured spread is not an established surface effect

[measured] Luna has a descriptive spread: 178, 180, 183 and 187 correct answers
in D0/K/P/B respectively, out of 200 each. Haiku has no observed accuracy spread
on this material. Luna's wrong answers concentrate on a few recurring items;
all its errors are on questions whose correct class is C or D.

[argued] I reject "no reading difference between surfaces was measured in
either family." The Luna differences were measured. We have not established
their cause, reliability or generality. Recurring difficult items do not rule
out an interaction in which a notation helps a reader understand those items.
Nor do they establish that the spread is a notation effect. Five sessions and
eight repeated contracts do not settle either claim. **Instead, use this ledger
wording:**

> [measured] Haiku scores 200/200 in every surface; Luna scores D0 178/200,
> K 180/200, P 183/200 and B 187/200 across five sessions per surface. Luna's
> errors concentrate on a few forced-result/underdetermined-result items.
> [argued] This pilot establishes no reliable surface effect or reading
> equivalence. Haiku is at ceiling, and Luna's descriptive spread remains
> inconclusive. Human readability is pending; model comprehension does not
> establish that a person can trust the witness nearly blindly.

[measured] One further correction: the misses are not exclusively answers
confusing C with D. For c4-q3, whose correct answer is D, nine sessions answer
B (result forbidden), two answer C, and nine answer D. For c4-q4, twelve answer
D, one answers B and seven answer the correct C. Thus the description "all are
judgments between the only allowed result and others also allowed" is accurate
only as a loose description of the item themes, not of every wrong answer.
**Instead:** say the errors occur on forced/underdetermined-result items, with
some allowed results incorrectly judged forbidden.

### 2. Observed dominance is not noninferiority

[argued] Replace "K is at least as good as braces for these weak writers"
with "K's observed full-task success is no lower than B's in either cohort,
meeting the registered provisional selection rule." We did not register an
acceptable loss margin or establish noninferiority. The exploratory Luna
K-minus-B task-bootstrap interval spans −20 to +20 percentage points; the
Haiku difference is just two trials. **Instead:** retain K as the provisional
package choice, make no equivalence/noninferiority claim, and reopen it if
human comprehension or a separately designed confirmation contradicts it.

### 3. Do not attribute D0's whole loss to the longest task

[measured] Relative to Haiku K, D0 loses two passes on T2, one on T3, two on T5
and two on T8: five of the seven-pass gap occurs outside T8. Its T2 failures
reach evaluation; T3 has an unknown-name check failure; T5's two parse failures
report an unexpected character at a field read following a call.
Only the two T8 failures specifically report an indentation-level jump. P's
gap is more concentrated on T8/T9, but it also loses one pass each on T3/T5.
**Instead:** report the task/phase table and the observed mechanisms separately;
do not say D0 and P lose mainly through indentation/delimiters on the longest
task, or infer an isolated causal contribution from the package comparison.

### 4. The four proposed round-4 actions

| Claude proposal | Codex position |
| --- | --- |
| [argued] Adopt K provisionally | Accept as a prototype package choice under the registered rule. Human readability remains open. Keep argument conventions and annotations identified as defaults, not individually measured winners. |
| [argued] Drop D0/P because both lose on long programs | Accept leaving them out of the initial prototype; amend the evidence and causal wording as above. Preserve them as registered ablation controls and archived experiment data. |
| [argued] Add an underdetermination check | Accept a scoped tooling direction, not an unconditional new first-gate decision procedure. Two distinct permitted result witnesses can demonstrate nonuniqueness in a supported contract fragment. Failure to find a second result does not prove uniqueness; unsupported/unresolved cases are unknown. A uniqueness verdict needs proof or exhaustive coverage of the full finite output domain. Report input, fragment, bounds, witnesses and evidence status. |
| [argued] State every required entry declaration in future tasks | Accept. Independently check that the public instructions determine the required entry name/type before fixtures are frozen; retain current T10 failures and the labeled post-hoc exclusion without repair or rerun. |

[argued] Contract analysis and notation can both affect comprehension; the
current study does not establish "contract, not notation" as a causal finding.
**Instead:** prioritize concrete witness-based explanations, retain unknown
where appropriate, and evaluate those explanations and any later notation
change separately. This is not evidence for restoring general solvers or a
universal `is-underdetermined` promise to the first CLI gate.

## Decision: defer X2–X5 and the repair arm this round

[argued] **Defer all five arms, explicitly as unrun and unresolved.** This is
a prospective scheduling amendment after X1, not a claim that the arms are
unnecessary because K won. They could still improve or undermine the defaults.
The transport is reusable, but new cards alone do not make them valid: infix
needs fixed precedence/grouping and equivalent lowering; lambdas need capture
and function-value semantics; inferred lets need an explicit inference rule;
labels need a complete declaration/call mapping. Each requires separately
versioned parser/checker support and fresh reference/negative controls. The
frozen X1 harness supports none of those new treatments. **Instead:** converge
now on the reversible baseline, preserve the frozen artifacts, and register a
separate arm freeze when a concrete implementation decision needs the result.

| Arm | Current default, support and reopening trigger |
| --- | --- |
| [argued] X2 operators | Keep the X1 named arithmetic/comparison calls as the prototype default. No isolated prefix-versus-infix result exists. Reopen before adding infix, with fixed operators, precedence/grouping, matched D0/K controls and the registered T1/T6 tasks. |
| [argued] X3 argument labels | Keep ordinary function calls positional and record/user-variant constructor fields labeled, as in X1. No isolated label-policy result exists. Reopen before introducing declared ordinary-call labels or changing constructor conventions; keep T6/T8 and their independent semantic cases. |
| [argued] X4 callbacks | Keep named helpers, including nested functions, as the prototype default. The observed scope/callback errors remain an unresolved ergonomic cost. Reopen before adding lambda syntax or claiming named callbacks are equally easy; compare matched T5/T6 algorithms and capture behavior. |
| [argued] X5 local annotations | Keep explicit local type annotations as the prototype default. X1 does not show they improve reliability. Reopen before permitting omission or claiming inference is inferior; keep T4/T5 types, numeric rules and checking strength fixed. |
| [argued] One-repair arm | Defer until the real CLI diagnostic contract is stable enough to freeze. It is directly relevant to agent tooling, not merely K tuning. Register exact feedback, source/context retention, failed-assignment denominator, one repair budget and treatment of the defective T10 task before calling participants. Do not count these first-attempt results as repair evidence. |

[argued] **X3 is argument labels, not information-flow security labels.** The
earlier security-label deferral does not settle X3, and X1 already includes
mandatory record/variant field labels. K's Luna T8 failures make that distinction
practical. **Instead:** give the two ledger issues separate names and record
the argument-label policy as a default with an unrun experiment. If ordinary
call labels are explicitly excluded from the first gate, say so as scope,
not as a measured conclusion.

[argued] The registered arm controls are D0/K, not whichever two packages won
X1. Do not silently switch to K-only or K/B after seeing these results. A later
study may choose a different design, but must register that change as a new
study and cannot claim completion of the original matched arms. **Instead:**
retain the original arm matrix alongside their deferred status.

[argued] Round 4 can proceed now for decisions independent of Hannah's session.
Record K as the provisional writing-package choice, the five arms as deferred,
and human readability as open. **Instead of waiting or silently closing these
questions:** merge the remaining agreed decisions and preserve the specific
reopening conditions above.
