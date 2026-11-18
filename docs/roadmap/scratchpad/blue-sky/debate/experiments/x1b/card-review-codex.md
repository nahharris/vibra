# Codex review of the K2 card, before implementation

Current disposition: the original brace-construction review below is retained
as history. Before measurement, brace construction was withdrawn; the current
K2/K3 cards use labeled constructor calls and braces only for declarations.
The follow-up now uses [this three-package registration](preregistration.md).
The later [parser checkpoint](README-codex.md) records completed validation;
the review's original pre-implementation observations are retained below.

[measured] I read `card-k2.md` and compared it with the frozen K card, API,
common-tree contract and parser. No K2 parser or scored K2 call has been made
in this review. The existing X1 sources remain unchanged.
Reviewed card SHA-256:
`64637793b86eecaf1095af3244ef2b632e0b47cd26321ebbbae32b86f14333d2`.

## Package decision

[argued] **Keep brace construction in K2.** The coherent-pair-notation
hypothesis includes parameter lists, field declarations and construction;
retaining K's construction spelling would test a narrower proposal. The
comparison remains **K versus the full K2 package**, not a one-factor colon
ablation. Besides colon removal, it changes parameter delimiters, record/case
field delimiters, field-bearing construction and the spelling of fieldless
case declarations. **Instead of claiming one factor:** freeze that complete
delta and attribute any result only to the package. No factorial study is
required for this bounded follow-up.

[argued] The grammar is implementable by recursive descent with the same AST.
Statement `if` and value `if` are distinguished by context: after `return` or
on a right-hand side the parser expects an expression; inside a statement
section it expects a statement. In expression position, `name {` is a nominal
construction, never a statement block. **Instead:** parse these contexts
directly and reject mixed/unregistered spellings; do not transform responses
into K with string substitutions.

## Changes/clarifications needed in the card

| # | Attack | Concrete resolution before freeze |
| --- | --- | --- |
| 1 | [argued] The card says record and variant cases with fields use braces, while its own example uses `option.some(value)`. The API describes option as a variant. That leaves a participant a plausible contradictory rule. | Say **user-declared** record and variant constructors use braces. Standard `option.some(...)`, `option.none()` and wrapper `w.of(...)` keep their existing API call forms. No `option.some {value: ...}` or `w.of {value: ...}` treatment is being added. |
| 2 | [argued] "Always the same way" can imply record semantics for parameters, including reorderability or one record argument. Those would change the task's function interface. | Say the three contexts share pair-list **syntax**, not semantics. `takes {a: int, b: int}` declares two ordered positional parameters; `f(3, 8)` binds a=3,b=8. Labeled record/case value field order is irrelevant; duplicates are rejected rather than silently overwritten. No anonymous-record type or unlabeled brace literal exists. |
| 3 | [argued] The examples show nonempty pair lists but do not settle empty/trailing-comma forms. Unit-case and zero-parameter conventions must not be guessed by the implementation. | State the pair-list grammar and trailing-comma policy. Recommended: nonempty comma-separated pairs, no trailing comma; omit `takes` for zero parameters and use bare `case empty` / `v.empty()` for a fieldless user case. No empty record type. Any strict rejection of empty parameter sections must be applied consistently to the fresh K control, not used to give K2 a stricter unstated canonical-style penalty. |
| 4 | [argued] "Ends at the next case/else/endif/endmatch/endfn" is unsafe if interpreted as a flat scan. A nested function or conditional has its own closer. | Define the boundary as the next relevant delimiter **at the current nesting level**. A recursive parse consumes a nested function/conditional/match and its closer before looking for the parent's boundary. Keep nonempty statement blocks. |
| 5 | [argued] The introductory "every construct" also encompasses calls, literals and `let`, which do not have keyword sections and closers. | Limit that sentence to **block forms** (`fn`, `type`, `if`, `match`). The brace-list bullet should mention both `fields {...}` and `case low {...}`. This is a wording correction, not another treatment. |
| 6 | [argued] If the K2 parser maps all calls to the common `Call` node without checking their written form, it may accidentally accept K's `item(name: x)` even though the card requires `item {name: x}`. The common checker cannot recover that punctuation difference. | Retain source-form information long enough for strict grammar validation, or validate it using the resolved user-constructor heads after parsing. Reject parenthesized user record/field-bearing-case construction and brace ordinary/API calls. Forward references must continue to work; do not force declarations before uses to solve this. |

[argued] These are clarifications and validation obligations, not permission
to change ordinary call ordering, equality, scope, generics, wrappers, options,
contracts or evaluation. **Instead:** preserve the frozen semantics and let
all semantic/type failures remain failures in the same common checker.

## Reference printer proposal

[argued] Accept Claude's K2 printer from the canonical trees of the frozen K
references. This gives a precise semantic target and avoids hand-editing nine
long programs. It does not alone validate the new parser: a printer/parser
pair can agree on the same wrong convention. **Instead:** keep the expected K
trees immutable, implement the K2 parser independently, and verify every
printed task against the expected tree, checker and unchanged external tests.
Do not generate expected trees by parsing the K2 output itself.

[argued] Include at least one independent hand-written golden, not produced
by the printer: two same-typed positional parameters and a record with those
fields constructed in the opposite textual order. For `make-pair(3, 8)`, the
result must still be `{left: 3, right: 8}`. That distinguishes parameter order
from constructor-label order without relying on a type error. Add an explicit
expected tree/result and test both K and K2 renderings.

## Focused parser/checker acceptance matrix

[argued] Before scored calls, require the following positive/negative cases
and retain their outcomes with the new freeze:

- Nested statement `if`/`match` and nested `fn` inside a branch; every closer
  belongs to its own construct. Also nested value conditionals after `return`
  and in constructor fields/call arguments.
- User record and field-bearing variant construction, including forward type
  references, nested constructions, and field permutations.
- Fieldless case declarations/constructions and functions without `takes`;
  the explicitly settled empty/trailing-comma policy in both controls.
- `option.some`, `option.none`, matching `some`/`none`, `w.of`, `.value`, bare
  function values and `try` unchanged from K.
- Nested `list(option(int))`, comments and quoted text containing colons,
  braces and section words. Token contents are not source-rewrite targets.
- Reject K section colons in K2, K2 brace field/parameter sections in K, missing
  pair colons, malformed closers, anonymous brace types and wrong constructor
  punctuation. Reject duplicate fields/bindings through the same checker
  policy; do not normalize invalid duplicates away.
- Card examples and T1–T9 references parse to the exact immutable expected K
  trees and pass the same fixtures. T9's supplied program is a matched
  rendering of the same T8 tree; T10 is absent prospectively.

[argued] **Instead of modifying the completed X1 freeze:** keep the new parser,
strict surface validation and runner under `x1b/`, pin all imported original
sources, and create a separate arm freeze. The
[registration proposal](../k2/preregistration.md) still needs explicit peer
acceptance of its assignment seed/decision rule and the final card before
dispatch. Original X1 controls/results are historical; this arm uses fresh K
calls and changes no deferred X2–X5 treatment.

## Update after owner feedback: K3, applications and products

[argued] Accept K3 as K2 without separator commas, under the explicit lexical
boundary rules and 162-trial matrix in the registration. Keep exactly three
surfaces, with no fourth brace-construction arm. One application syntax
preserves the existing K convention and removes an added expression form;
that is a safe argued simplification before measurement, not a measured loss
for braces or evidence that the owner's preference is an accuracy result.
The original review's brace-constructor origin checks are therefore obsolete.

[argued] Keep `list(T)`/`option(T)` consistent with the current call shape as a
reversible **argued** default. X1 did not isolate type application and supplies
no evidence against `(list T)`: D0/P had other differences, and prefix type
application was consistent with their prefix value calls. There is no supported
dictionary or other new multi-argument type application to test in this arm.
**Instead:** leave the type-application comparison unresolved and change no
type or evaluator rule for K3.

[argued] Structural tuples or anonymous records do **not inherently conflict**
with nominal types: both kinds can coexist while nominal brands remain
distinct. Adding them would introduce typing/elaboration and conversion rules,
however, so they and dictionaries stay deferred. Bare record values checked
against an expected nominal type would also add a contextual elaboration rule;
they are not a punctuation-only extension of this common AST. **Instead:** use
explicit nominal constructor heads now and record any future product/collection
shape family as a candidate, not implemented or irrevocably reserved syntax.

[argued] `[]` is only a list today. Empty-list/empty-dictionary ambiguity,
dictionary key expression parsing, duplicate-key policy, key equality and
ordering belong to the later dictionary decision. Identifier-plus-colon
lookahead for fixed fields does not settle arbitrary expression keys.
Negative literals also interact with existing kebab names even without infix:
`a-1` is a name, `a -1` is two items, and `[1-2]` must not silently become two
whitespace-separated items. **Instead:** freeze these lexical boundaries and
reopen them with X2 if infix enters scope.
