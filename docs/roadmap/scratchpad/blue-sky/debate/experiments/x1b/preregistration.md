# X1b: K, K2 and K3 follow-up registration

Status: accepted in full by both reviewers after human X6 and owner feedback.
Scored calls wait for grammar/oracle validation and the new artifact freeze.

## Treatments and exclusions

[argued] Compare three packages with identical types, scope, contracts,
evaluation and API behavior:

| Surface | Declaration/block syntax | Calls, constructors and lists |
| --- | --- | --- |
| K | The frozen X1 K surface: section colons; ordinary ordered parameter lists; typed closers. | Existing comma-separated calls and lists; field-bearing user constructors are labeled calls. |
| K2 | No section colons; nonempty braced typed pair lists for `takes`, `fields` and case fields; bare fieldless case declarations; typed closers. | Comma-separated calls and lists. Constructors remain labeled calls, as in K. Braces never occur in expressions. |
| K3 | Exactly K2's declaration/block grammar. | No separator commas in calls, list values or pair lists. Separate items with whitespace. Constructor labels and all semantic rules are unchanged. |

[argued] K2/K3 braces declare ordered parameters or nominal fields; they do
not introduce an anonymous-record type or one record argument. Ordinary calls
are positional. User record/case constructors keep every field labeled, in
any order; standard functions such as `option.some`, `option.none` and `w.of`
retain their existing argument rules. Zero-parameter functions omit `takes`;
fieldless cases use their zero-argument constructor call. Pair lists are
nonempty; K2 allows no trailing comma. Duplicate fields/bindings are rejected,
not silently overwritten.

[argued] All surfaces retain `list(T)` and `option(T)`, existing names and
implicit binders, annotated locals, named functions, `try`, example equality,
comments and nominal identities. Structural tuples, anonymous record types or
values, dictionaries, contextual bare record literals, infix operators,
lambdas and local annotation inference are absent. `[]` is only an empty list
in this arm; no dictionary inference or parser ambiguity is introduced.

[argued] K2's earlier brace-construction design was considered and withdrawn
**before measurement** after owner feedback and an argued preference for one
application syntax. It has no measured loss. Its old review/printer history
is not an extra scored surface. K2 remains a bundled punctuation/declaration
change relative to K; K2 versus K3 varies separator grammar, with the same
semantic rules, public examples and task algorithms. No result proves human
readability improvement or isolates a colon's contribution to the whole
K-versus-K2 package.

## K3 lexical boundary rules

[argued] The final card must say spaces **or line breaks** separate successive
arguments, elements and pairs. A separator comma token fails K3; commas in
text literals and comments remain ordinary data. `a-1` is one kebab identifier,
whereas `f(a -1)` supplies two arguments. Adjacent numeric tokens such as
`[1-2]` do not count as whitespace-separated items and must fail rather than
silently becoming `[1 -2]`. Likewise closing-delimiter adjacency must not
silently substitute for the registered item separator. Type application has
no new supported multi-argument type constructor in this arm.

[argued] A recursive expression parser determines where a nested call, list
or value conditional ends. Pair recognition uses identifier-plus-colon only
for the existing fixed-name fields/parameters; this is not a claim that one
token of lookahead handles arbitrary future dictionary keys. Closing keywords
must match the top open construct; crossed nesting is invalid. Strict parsing
must neither accept mixed surface punctuation nor rewrite it into the target
grammar. New strict canonical-form checks, such as rejecting an explicitly
empty parameter section, apply consistently to all fresh controls.

## Assignment matrix and prompts

[argued] Use unchanged task requirements T1–T9 and the original external
fixtures. Defective T10 is excluded prospectively. Three surfaces, nine tasks,
three fresh-context runs per task/surface/family, two named model families:
Haiku and Luna at their exposed low effort. This is **81 assignments per
family, 162 total**. Freeze the complete matrix before generation; do not
shrink, rebalance or repair it after outputs.

[argued] Assignment construction: loop tasks T1–T9, then surfaces K/K2/K3,
then runs 1–3; shuffle that list once using Python `random.Random(2026111502)`.
Both producer families use this algorithm and seed. Concurrent calls may
complete in a different order. Actual provider generation seeds are not
controlled or claimed.

[argued] Use **fresh K controls**, not old X1 results. Each prompt contains
one card, the common standard-function description and one task. T9 inserts
the surface's byte-preserved rendering of the same supplied T8 reference tree.
Cards have the same example tree and corresponding explanations. No competing
surface, earlier response, human/model reader answers, hidden fixtures or
repair feedback enters a participant context.

[argued] The old API contains `int.lt(a, b)` in a note, which supplies K/K2
punctuation to K3. Before freezing, make **one common surface-neutral API copy**
for this arm: describe first/second arguments in words, preserving every
function and semantic rule. Do not rewrite the old X1 API. Freeze the new
copy's exact diff and use it identically across all three surfaces/families.
If any further binder/example/scope wording is clarified, apply that shared
explanation equally in the fresh controls and record it. Task requirements
stay unchanged; English punctuation in prose is not a competing code example.

## Transport, scoring and evidence

[argued] Retain fresh isolated processes with no participant tools, fixed
one-line instructions, no project/memory/skills context, at most two transport
attempts across restarts, immutable prompts/prefixes/attempt records, and no
retry or repair for a completed wrong answer. Record actual model identities
where exposed, requested effort, finish status, provider usage and unavailable
fields. Missing/failed assigned calls stay failures in the full denominator.

[argued] Primary endpoint is assigned-trial full-task success, with extraction,
parse/check/example/runtime failure and budget ineligibility retained. Structural
task instructions remain secondary. Output eligibility uses provider aggregate
output including reasoning once, at most 4,096; missing/invalid/conflicting
counts fail eligibility, never become zero. Preserve raw functional success
separately. Generation caps remain asymmetric/unverified as appropriate.

[argued] Use the existing extraction and fixture semantics without changing
expected results. Report phases, task-level outcomes, actual token counts and
resource/transport failures separately. Nine task clusters, not private cases,
are the unit for descriptive paired uncertainty. The T1–T9 corpus is known and
reused; the treatment was proposed after inspecting X1 and human comments.
This is a follow-up pilot, not a held-out confirmatory comparison.

## Proposed decision rule, to accept before dispatch

[argued] Keep the two-family baseline protection from the two-surface proposal:
a challenger K2 or K3 is **eligible** to replace K only if its primary success
is no lower than fresh K in either family and strictly higher in at least one.
All surfaces have the same denominator, 27 per family.

1. No eligible challenger: retain K as the existing reversible default.
2. Exactly one eligible challenger: select it provisionally.
3. Both eligible: select one if it dominates the other under the same rule.
4. Both eligible but tied or crossing: choose K2 as an explicit smaller-change
   **default**, not a measured winner. It preserves K's separator convention.

[argued] This prevents retaining an observed dominated K merely because K2
and K3 tie above it. It also avoids substituting a challenger that trades a
loss against K in one family for a gain in the other. The tie/crossover choice
is a recorded migration/default judgment; there is no token, card-size or
preference tiebreaker. Publish every model/task table, including any excluded
challenger's crossover. No rule here establishes significance, equivalence,
population noninferiority or causal attribution to punctuation.

[argued] Before outputs, test the rule on synthetic count vectors (Haiku,
Luna): K=(24,24), K2=(26,26), K3=(26,26) selects K2 by default; K2=(27,23),
K3=(24,25) selects K3 as the only eligible challenger; all three=(24,24)
retains K. These are rule checks, not participant observations.

## Parser/oracle validation and freeze

[argued] All new sources live under **`x1b/`**, outside frozen `x1/harness/`.
Pin every imported original helper/checker/evaluator/isolation asset by hash.
K2/K3 parsers must return the immutable trees obtained from frozen K references,
not trees generated by their own parser or printer. Claude's printer is the
reference renderer; Codex's parser is independent. Include a separately
hand-written same-typed parameter/field-order golden and explicit expected
tree/result. Card examples and all 27 task references must check and pass the
unchanged external tests; private fixtures remain absent from participant tools
and prompts.

[argued] Positive/negative controls cover nested functions/branches/value-if,
typed pairs, unit cases, empty/trailing-comma rules, options/wrappers/try,
generic nesting, positional parameter order versus field-label permutation,
text/comments containing punctuation, whitespace adjacency and negative
literals, crossed closers, and rejected anonymous/brace value forms. The fresh
K wrapper may add only explicitly documented symmetric card-form validation;
any other checker/evaluator change requires a new agreed amendment.

[argued] Freeze this accepted registration, all cards, common API, exact task
texts, references/printer/golden, parsers/runners/tests, model isolation assets,
imported original-source hashes and external fixture hashes before scored
calls. Both reviewers inspect parity and controls first. Verify that old X1
freezes still match. No replacement of completed X1 files or reinterpretation
of its results is permitted by this follow-up.

[argued] Split: Claude owns cards/printer/references and the Haiku producer;
Codex audits them, implements the isolated parser/grader/Luna producer, and
validates oracle parity. Both producers use the same frozen prompt artifacts
and registration. The previous `experiments/k2/preregistration.md` is an
unfrozen historical draft superseded by this file.
