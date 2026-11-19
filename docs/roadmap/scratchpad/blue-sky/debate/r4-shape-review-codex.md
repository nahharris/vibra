# W/R/E/N/C: Codex's shape review before the rewrite

[argued] These proposals include semantic changes, not only renaming. None
has a measured authoring/readability advantage from X1/X1b. Those studies used
wrappers, the implicit `result`, binding contract examples, `text` and kebab
names. They remain evidence about those frozen packages. **Instead:** record
new choices as argued or owner-selected defaults, with the safety/meaning
conditions below, and preserve all experimental files and old keys unchanged.

| Proposal | Position | Evidence needed before claiming more |
| --- | --- | --- |
| W: invariant records replace wraps | [argued] Accept as a default for ordinary nominal data, with explicit constructor/boundary checks. Reject generic casting and the claim that multi-field invariants are free. | Focused constructor/nominality/ingress conformance; a matched arm only to claim easier authoring. |
| R: explicit return name | [argued] Accept one mandatory named result binder, exactly one returned value at this gate. No optional unnamed alternative and no mutable output parameter. | Scope/return-path conformance; matched writing/reading tasks for an ergonomic claim. |
| E: checked documentation examples | [argued] Accept conditionally as a semantic redesign: separate the formal result relation, protected expectations, and execution evidence. Do not silently make examples optional or claim the move removes their effective approval constraint. | Explicit policy/hash/diagnostic rules and the contrasting-result cases below; a fresh reader study for clarity. |
| N: `str` | [argued] Accept as an owner-selected canonical name, not a measured improvement. Prefer `str.*` consistently for the corresponding namespace. | Mechanical, reviewed API/diagnostic/schema migration; no LLM arm required merely to choose a spelling. |
| C: camel/Pascal identifiers | [argued] Accept camelCase value names and PascalCase nominal type names as defaults; lowerCamelCase variant cases. Case-sensitive resolution; no formatter-driven silent renaming. | Lexical/name-resolution and rename conformance. An arm is needed only for comparative reliability claims. Removing kebab does not settle infix separators. |

## W — A nominal invariant record is plausible, but not free

[argued] A one-field wrapper can be represented as a nominal record with a
checked predicate:

```text
type Cents
  fields {amount: int}
  where int.ge(amount 0)
endtype
```

[argued] Construct it with `Cents(amount: 5)` and project `c.amount`. This
removes the wrapper-specific surface forms while preserving the nominal brand.
`Meters(amount: 5)` must not become a `Cents`, even when their fields/predicates
match. Reading a public field is not a cast; creating a different nominal value
is an explicit construction with that target's checks. **Instead of adding
`Cents(5)` / `int(c)` cast overloads:** keep the ordinary labeled constructor
and ordinary projection, with explicit conversion functions where conversion
has domain meaning.

[measured] The frozen K3 representation already permits a record inside a
wrapper: [source](experiments/shape-review/existing-wrapper-range.txt) and
[results](experiments/shape-review/existing-wrapper-range.json). A wrapped
range accepts lo=1,hi=3 and rejects lo=3,hi=1 with `where-violated`.
This validates the old encoding, not proposed invariant records.

[argued] The gain is reducing declaration/projection ceremony, not making
relational predicates newly computable. Conversely, the experimental checker/evaluator
currently apply `where` to wrapped values, not records: generalizing it requires
new judgments and validation paths. "For free" would conceal that work.
**Instead:** describe the lowering/equivalence target, then implement and test
one invariant-record rule rather than retain a hidden unchecked wrapper escape.

[argued] Required rules before implementation:

- Fields are immutable. The predicate sees the complete typed field bindings,
  returns a Boolean and has no real effects. A false predicate rejects the
  construction; a fault, resource exhaustion or unsupported predicate never
  counts as success.
- Compute supplied field expressions once in a specified order, bind by label,
  then check the invariant before exposing the value. Canonical field maps must
  not silently reorder observable guard faults/resource behavior. Field-label
  permutation of values and expression evaluation order are different issues.
- Validate at untrusted ingress too: decoding, foreign-driver results, imports
  and restoration must not fabricate a well-typed record with false `where`.
  A proof may justify omitting a check under D4; a few successful tests may not.
- Public ordinary-data construction is distinct from private/opaque or host
  capability construction. A guessed field list or satisfied numeric predicate
  must not mint a file/network handle. F4's unforgeability obligation survives
  the surface collapse.
- Do not infer that every invariant type is inhabited or automatically has a
  default value. An impossible predicate has no successful constructor result.

[argued] **Settling conformance cases:** negative `Cents` construction; a
`Range` with `lo > hi`; a valid multi-field range; duplicate/missing fields;
same-shaped nominal types; invalid decoded/host-returned values; predicate
fault/exhaustion; and forged host handles. **Instead:** accept the data-shape
default conditionally on these rules, not on a new cast notation or an assumed
memory representation. One-field allocation/unboxing is a separate runtime
optimization, not part of the type's meaning.

## R — Name the one result, without introducing an output variable

[argued] Prefer one canonical signature form:

```text
fn absolute
  takes {n: int}
  returns {magnitude: int}
  ensures int.ge(magnitude 0)
  do
    return if int.lt(n 0) then int.neg(n) else n endif
endfn
```

[argued] The braces here declare **one result binder and its type**, not an
anonymous record returned to the caller. `return` still takes an expression
of type `int`, not `{magnitude: ...}`. Do not add tuples/multiple results by
accident. Always naming the result costs an author choice even when unused,
but avoids a second header grammar and an implicit fallback name. That is an
argued consistency choice, not a measured win.

[argued] The binder exists in `ensures`, after binding the returned value.
It is not available in `requires` and is not a mutable variable implicitly
assigned by the body. Reject duplicate input/result binders that would hide
an input and turn `ensures eq(x x)` into an accidental tautology. Apply an
explicit lexical-scope rule to captures; body locals do not become postcondition
bindings merely by sharing the result name. Every normal return path, including
`try`'s early return, must bind the result and run the same postconditions.
**Instead:** add that one postcondition binder to the core/diagnostics and
keep the existing return-expression semantics.

[cited] Dafny is an existence example for named results, not an ergonomic
experiment: its manual describes named method out-parameters as mutable body
variables. That is not the behavior proposed here. See the
[Dafny reference manual, method and return statements](https://dafny.org/latest/DafnyRef/DafnyRef.html#sec-return-statement).

[argued] `Result` as a type and a result-value name need not inherently clash:
type/value positions and namespaces may distinguish them. Casing alone must
not substitute for a namespace rule. The stronger reason for R is explicit
discoverable binding. Specify how binder-only renames affect canonical claim
identity/approval; do not accidentally give the binder a nominal identity or
client-visible record field. **Settling cases:** no postcondition, several
postconditions, branch returns, early `try`, a type named `Result`, a duplicate
parameter/result name, an invalid precondition reference and a binder rename.
**Instead:** choose the uniform named form now as a default and measure it
only if claiming that weak authors/readers perform better.

## E — Moving examples changes the formal relation, not necessarily admission

[measured] I evaluated the small c4 countermodel and saved
[countermodels.json](experiments/shape-review/countermodels.json). At input
`(7, 5)`, the strict distance bounds require `2 < result < 12`; the only
multiples of five are 5 and 10. A binding contract example `7,5 -> 5` reduces
the allowed results to `{5}`. If that example is documentation only, the
formal relation permits `{5,10}`. If a protected expected example remains a
mandatory acceptance check, an implementation returning 10 at that input
still fails that check. This is a finite logical model, not a new language
implementation or participant experiment.

[argued] Thus E can be coherent, but it does not delete the expectation's
effective constraint on approved witnesses. It moves the constraint between
roles. Distinguish:

1. **Formal contract:** types/`requires`/`ensures` determine permitted normal
   results. Documentation examples are not assumptions in this relation.
2. **Protected expectations:** an example has explicit input and expected
   output. The approval/build policy requires those checks before accepting
   a changed witness. Failed, unrun, unsupported and exhausted are not passed.
3. **Evidence:** after execution, record what was checked against which witness,
   inputs, tool, mode and budget. A checked illustration is sampled evidence,
   not universal correctness or an observation invented before a run.

[argued] If examples instead become optional comments that implementations may
ignore, E genuinely weakens the old sample-point guarantee. If checks remain
mandatory, saying they are "not part of the contract" must not imply that an
approved witness can disregard them. **Instead:** state both the logical
relation and the approval policy, and display both layers accurately.

[argued] Keeping docs/examples protected is reasonable, but "inside claim
hash" must be defined. One protected public bundle can contain semantic clauses
and documentation/test expectations without making every hashed item an axiom.
Use explicit components/digests or change classifications for semantic-clause,
documentation/example-only and witness-only edits. Bind test evidence to the
actual witness and expected-example digest. This does not require new source
declaration kinds or editable expectations under implementer authority.
**Instead:** preserve the approval boundary and distinguish a public-bundle
hash from what a prover/contract-entailment query may assume.

[argued] A small attached doc form can keep code as syntax rather than strings:

```text
doc
  prose "Rounds to a nearby multiple of step."
  example roundTo(7 5) = 5
enddoc
```

[argued] Place it in the protected public part of the function before `do`.
`prose` holds ordinary `str` text; only prose is in a string. The example call
and expected value are typed expression nodes. The dedicated example equality
can retain structural comparison for records/lists/variants without adding
general infix operators or forcing an integer-specific `.eq`. Use bounded
isolated execution, real effects unavailable except explicitly selected fakes.
Do not execute the doc block at every ordinary invocation. **Instead:** retain
one attached documentation shape and existing example-check infrastructure,
with checked/unrun/failed status and protected expected values.

[argued] Hannah's inconsistent answers motivate clarification, not proof that
this relocation improves comprehension; the old reader instructions explicitly
said examples bind. **Do not rescore X6.** Under E's new formal-contract meaning,
c4-q1 would change from C to D and c4-q2 from B to D. Those are new semantic
keys, not corrections to old participants. **Settling cases:** a formally
allowed result that fails a protected example; body-only changes that pass/fail
examples; an attempted implementer edit to an expectation; an unrun example;
and a reader asked separately what the contract permits and what the approved
example checks. **Instead:** version the semantics and evidence rules together.

## N — Accept `str`, migrate consistently, preserve old evidence

[argued] Accept `str` as an owner-selected spelling. There is no comparative
evidence that it produces better programs than `text`. Prefer the corresponding
namespace `str.*`, with casing consistently applied to member names, instead
of making authors memorize unrelated type/namespace spellings. This is an API
rename, not a new encoding, indexing, Unicode or escape policy. **Instead:**
rename the primitive, namespace, cards, diagnostics and public API together,
with one spelling in the new specification; leave frozen old experiments intact.

## C — Casing policy is not automatic semantic renaming

[argued] My default is camelCase for functions, values, fields, parameter labels
and return binders; PascalCase for declared nominal types; lowercase primitives
and builtin namespaces; lowerCamelCase for variant cases, which are value-level
members (`ParseResult.missingValue(...)`, `case missingValue`). A record
constructor uses its nominal type's name (`Cents(...)`); that is type-directed
construction, not authority granted by a capital letter. **Instead:** choose
this uniform convention without claiming that PascalCase cases are empirically
worse or that capitalization defines the type system.

[argued] Use a case-sensitive identifier grammar. If hyphens are removed from
identifiers, that is a lexical decision; capitalization preferences can be
style diagnostics over resolved declarations. The lexer can use one identifier
category rather than guessing type/value semantics from case. Underscore policy
should be stated once if admitted for interop/generated names. `fmt` may format
syntax, but must not silently rename public fields, cases, functions, imports
or approved claim bindings. Renaming requires a capture-aware project refactor
and any necessary API/schema/nominal-identity migration. **Instead:** enforce
conventions with precise diagnostics; do not hide a semantic rename in L4's
mechanical formatter.

[argued] I reject the stronger reason offered for C: kebab does not inherently
make infix impossible, and dropping it does not settle K3's separator problem.
The raw illustration in `countermodels.json` shows `[a -1]` at `a=5` could mean
the current two-element list `[5,-1]` or a future one-element subtraction list
`[4]`, depending on the chosen new grammar. This is **not an ambiguity in
current K3**, which has no infix. Removing `-` from names only removes the
`a-1` identifier interpretation. **Instead:** keep X2 deferred until operator,
grouping, negative-literal and item-boundary rules are independently specified;
do not describe that arm as reopened cleanly by a casing change alone.

## Arms, conformance and what to add

[argued] None of W/R/N/C needs an LLM arm merely to choose a safe reversible
argued default. W/R **do need semantic conformance and an independently checked
lowering/oracle** before implementation can claim the old guarantees. E needs
an explicit agreement to its changed logical/approval meaning. An arm becomes
necessary if the decision is justified by improved authoring or comprehension,
not by the default rule. **Instead:** freeze the actual design choices first,
then measure only a specific unresolved claim rather than changing all five
factors and attributing a combined result to one.

| Optional focused comparison | Valid scope |
| --- | --- |
| W | [argued] T5/T6 with old wrapped data versus invariant records; hold naming, return binding, docs and algorithms fixed. Add independent multi-field/invalid-ingress conformance, not just successful cents examples. |
| R | [argued] T1/T6 with implicit versus declared result binders, including helper functions with no ensures; hold types, naming and example meaning fixed. A human-readable-claim benefit requires fresh matched reader material too. |
| E | [argued] New reader material explicitly separating formal allowed results from protected-example acceptance; keys must match the new semantics. A like-for-like old-key writing comparison cannot settle the meaning change. |
| N/C | [argued] A pure renaming study can use a reviewed bijective map for task/API/reference/fixture identifiers, preserving behavior. It need not invent new oracle logic, but it must not silently repair old or misspelled participant names. W's wrapped-to-record encoding is a larger oracle change than renaming. |

[argued] Add or update ledger decisions for W constructor/ingress safety, R's
single postcondition binder, E's formal relation versus mandatory protected
examples, documentation shape, and identifier/style policy. Update D1/D4/D7,
E1/E4/E8/E9/X6-4, B2/B3/B11, C4/C5/C6 and L4/L6/L9 coherently. The remaining
identity/ABI policy must say which rename changes a brand and which changes
only its presentation. Keep capability confinement, runtime guards and resource
budgets; neither fewer keywords nor a documentation move establishes them.
**Instead:** rewrite once from the explicitly amended ledger after the agreed
scope decision; do not mutate the completed experimental language in place or
claim it already validates this new shape.
