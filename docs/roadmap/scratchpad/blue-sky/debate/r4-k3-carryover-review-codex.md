# Codex review of the K3 carry-over table

[measured] Reviewed `k3-carryover.md` snapshot SHA-256
`72b9cd3442bfb88900d5e4e77a43756865f466110f07f606abcbbc4fb815d7e8`
and the inventory's relevant source anchors. The hash identifies the reviewed
snapshot; it is not evidence for the merit of a design.

[argued] This supersedes the affected recommendations in my earlier
[syntax-sheet review](r4-k3-syntax-review-codex.md) and supplements the
[atoms/imports review](r4-carryover-atoms-imports-codex.md). Accept the
inventory-first process, the nested method/impl placements, existing Result
try, declaration imports and private-by-default access. Withdraw my restriction
of general `of` to records **if its existing nominal wrapper meaning is
retained explicitly**. A defined general form answers the alias objection;
an undefined general form did not. No pilot score establishes the merit of
removing a feature absent from its cards.

## 1. Section 5: patterns need a shared checker contract

[cited] [Source chapter](../../../../spec/01-source-language.md), let and
let-else, and [type chapter](../../../../spec/02-type-system.md), Constant
patterns, distinguish ordinary binders from names resolving to constant module
values. The table repeats the inventory's contradiction: "Binder name; never
compares" versus "Constant pattern ... Same." Existing prose also has this
inconsistency; copying both rows does not settle it.

[argued] My default is the detailed existing resolution rule: an unqualified
name resolving to an eligible module constant is a constant pattern; otherwise
it is a binder subject to no-shadowing. Dotted names resolve a constructor or
constant and never introduce a local. Literal/constant matching is structural,
not an arbitrary equality function with effects. **Instead:** replace "never
compares" by this qualified rule, and include the known consequence that adding
a constant can change an earlier apparent binder into a value match. A new
explicit-binder notation could avoid that consequence, but would be a deliberate
unmeasured change, not something to infer from casing.

[argued] Keep constructor patterns with labeled fields, exact-arity tuple
patterns, record patterns with omitted fields ignored, exact-length list
patterns, literal/constant patterns and one discard. Constructor omission must
not invent rest matching for tuples, wrappers or lists. `_` is a reasonable
single discard spelling, but the stated reason is wrong: current `-` is already
lexically distinct from `-1`. **Instead:** justify `_` by familiar single-form
notation, reserve it as a non-value pattern token, and separately update the
identifier/atom lexer and canonical uncovered-pattern diagnostic spelling.

[argued] The bare-case narrowing form should give way to one constructor-pattern
match form. `case Option.some(value: x)` binds `x` only inside the arm; the
original scrutinee is not implicitly rebound or promised a `.value` projection.
Qualified constructor resolution is checked at the scrutinee's nominal type.
**Instead:** keep single evaluation, ordered arms, reachability/exhaustiveness,
duplicate-binder rejection and inhabited-type rules in the common checker.
Do not translate old `case some` programs by textual substitution alone.

[argued] Match on `Atom` still needs a catch-all; finitely many literal arms
cannot cover all atoms. A singleton atom type can be covered by its one value.
Literal admission and uninhabited payloads also affect coverage. **Instead:**
carry those inventory rules into the mapping, rather than reducing coverage
to whether every case name appears once.

### Parameter patterns are not a reason to require anonymous tuple values

[argued] Destructuring does **not** require anonymous value literals. A function
can destructure a declared nominal record constructed with a labeled call.
Therefore section 4's "Destructuring needs them or a keyword form" is too broad.
**Instead:** keep nominal constructor patterns independently of the decision on
anonymous structural types, literals and their notation.

[argued] Do not remove parameter patterns merely because the first brace example
looks dense. A parameter binder can be an irrefutable pattern under the same
expected-type rule as let. One coherent proposed generalization is:

```text
takes {Pair(left: a right: b): Pair(I32 I32)}
```

[argued] Here the outer braces hold positional **pattern/type** entries; they
are not runtime labeled arguments. Ordinary `takes {x: Int}` is its simplest
case. Pattern entries introduce every contained binder, preserve parameter
order, and must be irrefutable. Fn type input vectors remain type-only. This
relaxes the global "name: Type" sentence and is unmeasured, but it preserves
the existing capability without adding a new keyword. **Instead:** use this
generalization as the working default, or explicitly mark name-only parameters
as a gate cut. Body destructuring is an equivalent workaround for many programs,
not evidence that the existing parameter feature was retained unchanged.

[argued] Binding `Pair(left: a right: b)` is irrefutable only if Pair has the
right record shape and all selected subpatterns are irrefutable. A constructor
of an ordinary multi-case variant or `[a b]` on a variable-length list is
refutable. **Instead:** use one irrefutability judgment in let and parameters,
and require a refutable pattern for let-else as current Vibra does.

### Let-else needs an explicit fallback boundary

[argued] `let pattern = expr else return ...` is not a complete K3 rule. It
omits C8's type annotation, restricts a general Never fallback to return, and
does not delimit a multi-statement fallback when layout is insignificant.
**Instead:** my concrete provisional form is:

```text
let Option.some(value: item): Option(Str) = candidate
else
  return fallback
endlet
```

[argued] Ordinary irrefutable let still has no closer. This refutable form has
an explicit fallback block and closer; that is a new documented exception to
the pilot's no-endlet rule. Every path through the fallback must leave control
with Never; a Never-returning call is also allowed there. Evaluate `candidate`
once; successful bindings are visible only afterward, never in the operand or
fallback. The nearest callable's declared return binder and ensures apply on
an early return. **Instead:** agree this form or another fully delimited one
before claiming let-else has a spelling. Do not use physical line breaks to
end the fallback implicitly.

## 2. Wrappers versus W is not an exclusive semantic choice

[cited] Type chapter, Nominal declarations: a non-structural `deftype` body
declares a distinct wrapper, not an alias; construction and unwrapping patterns
are visibility-controlled. The same chapter defines Str/Bytes representations
as wrappers. Section 3 cannot both remove wrappers universally and keep those
rows unchanged without a specified replacement interpretation.

[argued] My revised default is **retain general `type Name of TypeExpr` with
the existing shape classification**, and keep W's useful record invariants as
an addition to ordinary records. For example, `type Celsius of F64` remains a
wrapper, while `type Cents of {amount: I64} where ...` is an invariant record.
They have different representations and elimination rules, rather than two
spellings of one constructor. All declared types remain nominal. **Instead:**
retain wrappers pending a demonstrated reason to migrate their uses; W no longer
means wholesale wrapper deletion.

[argued] The table's `Celsius(21.5)` / `Celsius(c)` alternative conflicts with
the owner's "constructors always labeled" rule. Do not ask the owner to approve
an inconsistent package. One concrete default preserving wrappers is:

```text
type Celsius of F64 endtype
let temperature: Celsius = Celsius(value: 21.5f64)
let Celsius(value: degrees): Celsius = temperature
```

[argued] `value` is a fixed wrapper constructor/pattern payload label, not an
implicit field projection, cast or mutable variable. No `temperature.value`
projection is added. This is a proposed adaptation of the old wrapper interface,
not measured syntax. The owner may prefer another one canonical payload label;
if she rejects such a label, that decision reopens the all-labeled rule for
wrappers. **Instead:** settle that collision explicitly; do not silently bring
back positional construction only for wrappers or nominal tuples.

[argued] Keep W's ingress checks, immutable fields, written evaluation order
if adopted, and conservative predicate execution rules. Field projections are
not numeric conversions. Do not infer predicates over a wrapper representation
from the record-field `where` scope: that would need a named predicate input
rule. Public data, opaque ordinary wrappers and sealed authority handles still
need distinct access decisions. **Instead:** permit executable field invariants
on records now; leave representation predicates on wrappers unresolved unless
their scope is actually specified. A generic record constructor must never
mint a host capability.

[argued] General `of` also needs actual type-expression spellings for anonymous
enum and union bodies. Case sections alone do not define the promised arbitrary
TypeExpr grammar. **Instead:** mark those productions open and carry their old
semantics until a changed/deferred row explicitly replaces them.

## 3. Rows marked kept that change observable behavior

[argued] Correct these labels or supply the missing rules. "Same purpose" is
not the table's definition of kept:

| Row / section | Problem | Concrete disposition |
| --- | --- | --- |
| Operand order, section 6 | Current source evaluates fixed operands, then labeled operands in declaration order, then variadic operands in source order; the callee runs once first. Written order is a change. | Mark changed. My proposed default remains callee-first then written runtime-operand order; prohibit formatter argument reordering unless it preserves that order. Alternatively retain the existing full order. Pin the decision in C6/B11 and the invariant-construction rule. |
| Optional import alias, section 2 | Current imports always have aliases; deriving the last segment adds implicit binding and collision behavior. | Mark changed by the reported owner ruling, not kept. Apply canonical identity and namespace kind to the derived binding. A prelude List already in scope may require `as IntList`; do not make an accidental duplicate import silently override it. |
| Arbitrary attribute order, section 2 | Accepting only canonical section order rejects formerly accepted orders. | Choose: accept arbitrary order and format canonically, or mark canonical-only parsing as a deliberate change with a grammar/version entry. |
| Void, section 1 | `Void` supplies a type spelling but no replacement value spelling; a type is not implicitly its one value. | Use `Void` for the type and `void` for its value unless the owner explicitly changes the value syntax. `return void` remains a valid ordinary function return. |
| Top-level let/doc/visibility, section 2 | Replacing def by let needs a complete declaration boundary and attribute placement; a local let is not a top-level value with export attributes. | Retain pure initializer rules, constant classification and cycle rejection. Specify the exact top-level grammar and binding scope; do not assume a line boundary closes it. |
| Return on every path, section 6 | The pilot measured explicit return syntax, not its superiority over current tail-expression results. Never paths and try propagation need exceptions. | Mark an argued choice within a measured package. Require explicit value returns on normally returning body paths; propagate carrier failures through try and permit genuine Never paths. Keep ensures on every actual returned value. |
| Applied array becomes List, section 3 | Renaming cannot change indexed storage, scalar/string indexing, key admissibility, complexity or collection semantics unnoticed. | Keep representation/API semantics or mark individual changes. Do not infer list linked-storage or alter index numeric types from the name. |
| Projection and tuple rows, section 4 | Removing literal-index tuple projection and requiring complete destructuring changes an existing operation, possibly for generic callers. | Keep a named tuple projection operation or record an explicit gate cut with examples. Do not call the removal merely a spelling change. |
| Any / interfaces, sections 3 and 7 | "Interface values wait" and "static closed-world dispatch same" do not account for all current explicit widening/value behavior. | Distinguish static generic dispatch from erased interface-value use. Preserve the existing unification, applied-target identity, ambiguity rejection and default-member rules where retained; mark other operations deferred. |
| Dependencies/project data untouched, section 10 | Entry parameters, driver/resource bindings, effect-consent replacement and model profiles change target data. Atom/identifier changes may also change unit and path rules. | Mark the project/target schema as retained infrastructure with explicitly changed fields and interpretation versions; pin the new target identity. No claim that project data is wholly untouched. |

[argued] Dropping `do` is not automatically harmless merely because arms now
hold sequences: current `do` is an **expression**, including in argument and
other nested expression positions. Function-level sequences do not replace
that use. The cards also omitted general expression statements, while existing
test bodies call assertions directly and a let-else fallback can call a Never
function. **Instead:** map expression sequencing and effect-free Void/Never
expression statements explicitly. My default is to retain a delimited
`do ... enddo` expression where its last expression supplies the block value;
ordinary fn bodies still return explicitly. If it is cut, record that
restriction and check evaluation-preserving rewrites, rather than claim all
grouping became unnecessary. Also settle whether discard statements retain
C8's annotation or have a specific checked-inference exception.

[argued] The broad inventory-to-table coverage claim also needs traceability.
Covering every section heading is not proof that all 299 construct behaviors
received a disposition. **Instead:** assign stable inventory ids and map each
to a carry-over row (many ids may share one row), with no unmapped ids. That is
a small coverage check, not an additional language feature or model experiment.

## 4. Construction, patterns and function values are separate judgments

[argued] If anonymous tuple/record values use braces, accept that as a reversal
of the prototype's expression restriction. Empty braces need one meaning: the
existing empty tuple is available, while current record types are nonempty.
Nested product patterns should share component rules with their types, but
record-import punning must not introduce ambiguous constant patterns.
**Instead:** pin zero/one/many tuple arities, record pair/pun expansion, nominal
versus structural identity and brace-context grammar together.

[argued] `Pair(1 "x")`, `UserId(1)`, and a positional wrapper pattern cannot
coexist unqualified with "all constructors labeled." Nominal tuple construction
also needs a label policy or an explicit exception; anonymous tuple components
are positional by their structural nature. **Instead:** keep that owner rule
scoped to record/case fields until the wrapper/tuple policy is settled, or
design the additional labels before claiming the broader rule is implemented.

[argued] Pattern constructors are not necessarily function values. Labeled
`Option.some` therefore cannot simply replace a positional callback of type
`Fn({T} Option(T))`. The source's closed constructor/function distinction should
remain. **Instead:** use an explicit anonymous function to construct inside a
callback, retaining arbitrary expression callees such as `choose(c)(x)` and
single evaluation of the callee. Do not accidentally narrow application to
bare name calls because most printed examples use names.

[cited] Current function types also record labeled parameter names/types and
variadic shape; defaults belong to the callable value rather than its type
(type chapter, Nominal declarations). A positional vector alone is not the
whole carried-over function-type contract. **Instead:** keep `Fn({T...} R)` as
the positional core and explicitly map labeled/variadic callable types when
those parameters are retained. Their remaining syntax is open, not implicitly
excluded by acceptance of the vector notation.

[argued] Defer infix while importing the existing numeric families. A plain
Int needs a width/overflow/serialization/default-literal contract; it is not
defined by familiarity or the experiment's Python integer implementation.
**Instead:** retain sized numeric semantics as the default until a separate
plain-Int contract is agreed. Unsuffixed literals remain context-checked, and
porting fixtures requires preserving their numeric assumptions explicitly.

[argued] Typed lets are not a full replacement for expression ascription.
Destination dispatch and empty/generic constructions may need an expected type
inside an argument or return expression. A helper binding can often supply it,
but is a deliberate restriction. **Instead:** keep a general static expression
ascription form, for example `as(Type expression)` as a reserved form, or mark
its deferral honestly; independently defer union-narrowing patterns if unions
remain deferred. It inserts no cast or implicit conversion.

## 5. Interfaces, cold plans and tests

[argued] Accept nested impls and unqualified nested method declarations as the
working ownership rule. My previous top-level `fn Cents.add` suggestion is
withdrawn. The existing no-override defaults, exact inherited signatures,
same-module impl-placement restriction, non-overlap and applied-interface block
identity must accompany the nesting; nesting alone does not settle them.
**Instead:** preserve those constraints explicitly, including bounds inherited
from owners and the absence of blanket/generic impl extensions at this gate.

[argued] Sections 6/9 must retain the accepted cold-plan interpretation: a Proc
call constructs Intent(R); await executes it, and pure Fn may construct but
not execute plans. A Proc may delegate to captured capabilities or intents,
not only names literally appearing among its parameters. **Instead:** preserve
F3 provenance, explicit actual root grants, transitive broad/unknown summaries
and model-target isolation. Do not translate "proc is effectful" into eager
dispatch, or claim that surface await sites are an exact list of host effects.

[argued] On tests I amend my previous preference: **retain `test ... endtest`
as the default**, given the existing discovery/assertion/isolation contract.
Ordinary manifest-selected functions were a way to avoid inventing a test form;
that argument has no force against an existing useful one. This is an argued
reversal, not a measured win. **Instead:** preserve test name/selection identity,
assertion failure and unsupported/exhaustion behavior, private access rules and
fresh deterministic fake-world isolation. Protect the bodies, oracle helpers,
fixtures, selection and runner policy in the approval path regardless of form.

[cited] Programs chapter, Tests, specifies Void result bodies, no direct return
or try, and isolated deterministic in-memory worlds (including its later
isolation paragraph). Keeping this form is not keeping the proposed zero-input
Bool function protocol. **Instead:** make tests a distinct declaration kind
and amend B5's count, rather than force return-every-path into assertion bodies.
Capability-backed test fakes and their binding policy are still changes from
erased row-based test consent, even if the isolation goal is retained.

## 6. My recommendations for the six owner rulings

[argued] These are reasoned defaults, not measurements:

1. **Keep wrappers and general `of`; add invariants to records.** W need not
   delete representation wrappers. Settle labels and opacity explicitly.
2. **Keep sized numeric types; plain Int remains unresolved.** Add it only with
   an exact arithmetic/default/encoding contract.
3. **Accept structural product brace notation in principle**, with complete
   empty/one/many, record-label and pattern-context rules. It changes C1, but
   restores existing semantics rather than inventing structural products.
4. **Keep mandatory Fn input vectors.** `Fn({Int} Int)` has one Int input;
   `Fn({{Int Int}} Int)` would have one structural tuple input, while
   `Fn({Int Int} Int)` has two. No scalar-input shorthand is needed.
5. **Pattern match only**, with real nested binding and coverage rules; retire
   the special narrowed-scrutinee pilot form in the new grammar.
6. **Keep test declarations**, preserving their existing useful contract and
   strengthening the protected-oracle policy as already agreed.

[argued] For each, the corresponding counterexample is a concrete gate-1
program or conformance case that the proposed rule cannot express safely or
unambiguously. In particular check: wrapper versus equal-shaped record and
cross-module opacity; constructor label consistency; numeric overflow and
literal context; empty products and one-tuple callbacks; constant/binder
collisions and let-else escape; test tampering, isolation and discovery. Matched
model arms would be needed for new claims of model ergonomics, not to correct
these factual mappings. Keep X1/X1b artifacts and results unchanged.

[argued] Before calling the replacement specification complete, reconcile the
affected tooling/runtime/diagnostics chapters too. A 01–04 construct inventory
is a sound starting scope, not proof that the rest of the language contract
survives unchanged. **Instead:** extend the same traceable disposition method
to the affected 05–07 behavior before the final rewrite is approved.
