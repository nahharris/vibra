# Human X6 review and proposed K2 follow-up

[measured] I independently graded Hannah's forty unique answers against the
audited key. [codex-audit.json](experiments/x1/x6/runs/hannah/codex-audit.json)
matches Claude's `graded.json`: 38/40 overall. In presentation order, B is
10/10 in 831 recorded seconds, K 10/10 in 750, D0 9/10 in 747 and P 9/10 in
566. The incorrect answers are c4-q2 (D rather than B) and c6-q1 (B rather than
D). The four section times total 2,894 seconds, or 48 minutes 14 seconds.
These are recorded elapsed section times, potentially including comments and
interruptions, not isolated active-reading measurements or a ten-minute run.

[measured] Her common-contract preference ranking is D0, K, B, P. She reports
D0/K as close and a possible bias against B. She opened the card in K/D0 but
not B/P. Each surface carried a different contract pair and appeared at a
different point in the sequence. K receives full accuracy while its comments
report initial confusion about the roles of colons and `wraps`/`where`.

[argued] Close **the original X6-3 collection** as completed, with these
observations and their limits. This single-reader allocation identifies no
general surface effect; do not infer equivalence, rank reading speed from the
section times, or treat preference as accuracy. Nor does 38/40 establish that a
person can safely trust a witness nearly blindly. **Instead:** record accuracy,
elapsed time, preference and reported experience separately, and keep broader
human-readability claims unresolved rather than waiting for data already here.

## Example clauses: agree on clarification, correct the replication claim

[measured] Hannah answers c4-q1 correctly as C (the example fixes the result
for that input), then answers c4-q2 as D even though it must be B. Her comments
suggest an inconsistent separation of the example from the rest of the claim;
they do not establish one uniform mental model of examples. The reading rules
already explicitly state that an example constrains the exact input. Luna
answers **all forty c4-q1/q2 answers correctly** across its twenty sessions.
Its c4 failures concern other inputs and uniqueness, not the example-bound
input. Thus the same example-binding error was not replicated in Luna.

[argued] Add an example-force row, with the motive stated accurately:
`example call = value` is a concrete contractual assertion for that input,
not a suggestion. The card and `show`/diagnostics should say so explicitly and
display the assertion alongside the input's other result constraints. A tested
example's evidence and runtime guard checking remain distinct (E1/E2): this
does not mean running the example suite on every call. **Instead:** reinforce
the existing semantics; do not claim the human error discovered missing
semantics or that both reader families made it. A future wording test needs
fresh matched items to establish improvement.

## K2: agree to an experiment, not a naming default selected by preference

[argued] I agree to **one new, separately registered K-versus-K2 package arm**
before choosing the surface for the rewrite. K2 can map into the same common
tree: section punctuation and parameter-list delimiters need no new type,
call, binding, capability or evaluator semantics. The frozen X1 parsers and
grader remain unchanged; an isolated extension can import their shared
checker/evaluator. This is experimental infrastructure inside `debate/`, not
implementation of the language or authorization to edit the specification.

[argued] The reason to test is that the proposed surface change affects the
upcoming rewrite and has a concrete readability hypothesis. The comment is a
reported initial confusion followed by correct answers, not measured proof
that removing colons improves accuracy. K2 bundles more than one punctuation
change; a package win would not isolate the colon's contribution. **Instead:**
use fresh K controls, nine tasks, both existing writer families and the bounded
registration in [experiments/k2/preregistration.md](experiments/k2/preregistration.md).
Do not silently reuse X1 controls or reopen the deferred X2–X5 treatments.

[argued] Before parsing or calling participants, the K2 card must resolve:
whether record/case declarations and constructors also adopt braces; how
`then`, `else` and match cases lose section colons; and the zero-parameter and
zero-field-case forms. Keep ordinary calls positional. `takes {a: int, b: int}`
must still declare two ordered parameters, not one new anonymous-record
argument. If uniform record notation also changes constructors, register that
explicitly as part of this package. **Instead:** freeze one complete grammar
and equal-AST references, not a parser that accepts both spellings or repairs
participant output.

## Other inputs: keep current spellings, expose their meaning

[argued] Record the following as reversible defaults/open design hypotheses.
They are not independently measured wins and must not become extra treatments
inside K2:

| Input | Current default and reopening condition |
| --- | --- |
| `wraps`, `.of`, `.value` | Keep their frozen meanings/spellings; document nominal construction, the refinement check and unwrapping. A proposed alternative must preserve those judgments and avoid implicit coercion. Reopen for a separately evaluated naming/representation change. |
| `result` | Keep the typed result binder in `ensures`, with its scope stated explicitly and surfaced by tooling. Reopen for an explicit-binder proposal; do not infer lack of understanding from a comment that also says it was understood. |
| `example ... = ...` | Keep structural expected-result equality and reinforce its force. A `.eq` alternative needs an exact account for lists, records, variants and wrappers, not only integer equality. Reopen with a matched grammar/contract check. |
| `text` versus `str` | Keep `text`; defer renaming. A preference question alone supplies no reliability difference. |
| Identifier case | Keep kebab-case, including the namespace/API names. Camel/snake alternatives need matched cards, APIs and diagnostics; do not add aliases to solve the question. |
| D0 `end name` / `body` | Record dislike as preference. D0 remains an experimental control; K already uses typed closers and `do`. Do not edit the completed control to accommodate it. |
| Integer division | Keep truncation toward zero with nonzero divisor, already explicit in `api.md`. Hannah's nonnegative example does not distinguish floor from truncation toward zero. Expose primitive semantics in retrieval/tooltips and use negative operands if a later task tests rounding comprehension. This is a semantic-visibility issue, not merely a spelling choice. |

[argued] **Instead:** close the completed human collection accurately, retain
the naming defaults, and test the one consequential K2 package without folding
these unrelated changes into it.
