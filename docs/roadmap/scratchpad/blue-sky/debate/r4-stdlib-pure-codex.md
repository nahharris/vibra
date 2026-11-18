# Pure standard library: Codex review and defaults

[cited] Reviewed [Claude's S1–S12](r4-stdlib-pure-claude.md), including the
accepted S2b/S9 corrections, against the current [iteration contract](../../../../spec/02-type-system.md),
[runtime/native contract](../../../../spec/06-runtime.md) and stdlib sources.
The relevant source modules are [Iter](../../../../../stdlib/src/std/iter.vib),
[core](../../../../../stdlib/src/std/core.vib),
[builtin](../../../../../stdlib/src/std/builtin.vib),
[Str](../../../../../stdlib/src/std/str.vib),
[Bytes](../../../../../stdlib/src/std/bytes.vib) and
[Bool](../../../../../stdlib/src/std/bool.vib). This is a design review; no
stdlib or specification files are changed.

[cited] **Convergence update, 2026-11-16:** Claude accepted all verdicts and
additions, including operation indexing, Bytes iteration, the dictionary From
instance and the shared helpers. His explicit contextual-try rule is accepted
below. The earlier recommendation to use two lets as the default parsing form
is superseded; the two-let form remains valid.

[argued] Accept the owner's rule at the level of **semantic algorithms**:
write the generic algorithm once, permit type-specific primitives and thin
representation adapters, and keep optional accelerators accountable to that
definition. Otherwise the rule would forbid implementing compare or next for
individual representations, which every generic interface needs. **Instead:**
distinguish shared behavior from the leaf operations that supply it; neither
per-type API aliases nor native copies become separate language definitions.

## 1. Verdicts on S1–S12, plus the additional duplicates

| Item | Verdict | Default |
| --- | --- | --- |
| S1 Iter defaults/adapters | **Accept, qualify the laws** | Preserve map/filter/skip/take/collect, the four adapters, pure callbacks, immutable remainder values and interface-typed results. Keep lazy pulling, not a promise that every next terminates or every filtered infinite source produces an item. |
| S2 Fold | **Accept move** | One strict left fold, `Iter.fold(xs initial step)`, as a default member; no public List.fold alias. Step receives accumulator then item, executes once per yielded item in order, and empty input returns initial. See section 2. |
| S2b Lexicographic walk | **Accept** | One standalone generic helper with T: Ordered; Str/Bytes comparison impls are thin delegates, not copies of the loop. Place it with iteration utilities, with a checked acyclic import path, rather than introduce a Core-to-Iter bootstrap cycle casually. |
| S3 Comparison names | **Accept** | Equatable and Ordered. No Comparable alias or inferred conformance. Preserve laws, key admissibility and existing interface-value restrictions. |
| S4 Comparison defaults | **Accept, complete semantics** | Add less/lessOrEqual/greater/greaterOrEqual/min/max to Ordered. Each invokes compare once; min/max select the left operand on equal. They are ordinary eager calls, pure by kind. Add inRange for the additional actual duplication below. |
| S5 Conversions | **Accept contextual try** | From/TryFrom stay destination-dispatched through written expected types with fixed ConversionError. Try propagates a supplied success-type context to its operand in the enclosing callable's carrier, with exactly the same error type. It still requires that carrier and preserves the context's provenance. The short typed-let form is the default; a typed-Result intermediate remains valid. |
| S6 Numbers/Char | **Accept** | Preserve type-specific primitive semantics and camelCase source names. Signed types have neg; do not infer an unsigned neg API. Saturation, checked errors, div/rem traps, shifts and float behavior carry over exactly. Char.fromU32 retains its Option result, not a new guessed TryFrom error contract. No generic numeric hierarchy yet. |
| S7 Length/concat/slice | **Accept no new interface; amend rationale** | Keep type-specific APIs as primitives/delegates. Differing units alone do not prove a generic abstraction impossible; there is currently no agreed generic consumer worth the new contract. Str/Bytes bodies already delegate storage operations to array primitives. |
| S8 Bool placement | **Choose type members** | Bool.not/and/or/xor, with one source body each. No parallel module-function exports. This is an argued namespace change, not an observed model improvement. Preserve eager evaluation; use if for short-circuit control. |
| S9 Builtin Iter conformance | **Accept explicit owner impls** | List, Dict, Str and Option declare nested impls. Ordinary source next bodies where practical; an explicit pure compiler primitive can implement the representation-sensitive Dict remainder. This removes implicit conformance lookup, not all primitive implementation work. |
| S10 Array/List | **Choose List** | One public name, List, for the existing immutable indexed sequence semantics. No linked-list complexity promise, extra Array alias or new generic List.map. ABI/role identities are versioned separately from source spelling. |
| S11 Option/Result | **Accept** | No new container-preserving map members now. Option's existing Iter conformance yields zero/one items; Iter.map returns Iter. Result is not Iter and must not silently discard errors through iteration. |
| S12 Extra iterator methods | **Accept deferral, amend derivability claim** | Derivable through next does not mean semantically replaceable by strict fold. Any/all/find need short-circuit traversal to avoid unwanted work/faults; zip/enumerate need their own contracts/adapters. Add a shared default only on an actual use case. |

### Another existing duplicate: inclusive range checks

[measured] A [snapshot body scan](experiments/stdlib-pure/body-scan.py) over
all 14 stdlib modules found 68 body-bearing functions and three groups of
exactly identical body trees: the two lexicographic helpers, the inclusive
range helpers, and the Str/Bytes Equatable adapters. [Raw trees and source
hashes](experiments/stdlib-pure/body-scan.json) are retained. This deliberately
excludes signatures/attributes and performs no type checking, alpha-renaming
or general semantic-equivalence search; the genericity judgments below are
arguments using the actual referenced operations.

[cited] `char.in-range` in builtin.vib and private `within` in str.vib both
compare the value against a lower bound, return false if lower, then compare
against an upper bound and reject greater. One uses Char and the other U32.
They implement the same inclusive range test, not different encodings.

[argued] Add **`Ordered.inRange(value low high)`** as a shared default:
lower comparison first, upper comparison only if needed. Bounds are inclusive;
reversed bounds admit no value under the order laws. Call operands are still
evaluated before entry. Replace Char.inRange and Str's private within loop
with this helper; Char's ASCII classifications and UTF-8 encoding helpers
then use it. **Instead:** remove the duplicate algorithm/alias rather than
leave a special per-Char relation after adding generic comparison helpers.
Reopen if a real operation uses a different interval convention; do not guess
exclusive bounds from the word range.

[argued] The wrapper operations Str/Bytes length, concat and slice need not
be erased to satisfy this rule: their thin bodies unwrap, delegate the common
storage operation and rewrap the correct nominal result. The repeated actual
algorithm in their order helpers is different and should be factored.
**Instead:** count substantive repeated control/data-flow, not every repeated
function name or a necessary typed adapter.

[argued] The third scan group is the tiny Str/Bytes Equatable adapter:
compare, then classify Ordering.equal. Each type still must explicitly supply
its Equatable implementation; Ordered does not imply Equatable and erased
values do not gain a new conformance. **Instead:** retain these thin delegates
to the shared comparison behavior, rather than add a second public equality
API, interface inheritance or a blanket impl solely to remove a small match.
Their shared comparison walk is the substantive algorithm being factored.

## 2. Fold and laziness: which behavior is preserved

[argued] The member contract is a pure callback `Fn({A Item} A)` with its own
accumulator type A. It is a **left**, order-sensitive fold, not an associative
or parallel reduction. It repeatedly pulls next, feeds step the prior accumulator
and item, and returns the final accumulator at end. Earlier faults/exhaustion
remain earlier faults/exhaustion. **Instead:** keep H3 separate; no backend may
rebalance the calls merely because the callback has no host effects.

[argued] A pure callback may construct an Intent/Proc as data, with its latent
row preserved in A or the output type. The pure fold/map does not execute it.
An effectful callback requires a later explicitly designed operation, not a
covert overload of the pure API. **Instead:** keep the Fn/Proc distinction
through these higher-order defaults.

[cited] Current Iter's next is a pure function of an immutable value; the
remaining iterator has the same concrete Self, and erased calls rewrap it as
the same applied interface. Adapter construction does not pull its source.

[argued] Carry the pull details explicitly: take(0) must not pull; each map
pull maps at most one yielded item; filter may pull several rejected items;
skip performs its work when pulled. A filtered infinite source with no accepted
item may never produce its first next. Fold/collect over an infinite productive
source do not finish either. **Instead:** qualify termination laws by finite
or productive input as appropriate, and retain the runtime bounds/unsupported
termination judgments. Purity alone is neither productivity nor cheapness.

[measured] A small [pull countermodel](experiments/stdlib-pure/pull-countermodel.py)
and [raw results](experiments/stdlib-pure/pull-countermodel.json) compare strict
fold-based any with a next-based early exit. On true/false/end, both return true,
but fold makes three next calls and early exit one. When the tail faults after
the first true item, fold faults on its second pull while early exit returns
true. This is executed **Python operational modeling**, not a Vibra runtime
measurement or a performance benchmark.

[argued] Thus S12's deferral is fine, but a future short-circuit any/all/find
cannot be implemented by draining fold and merely skipping later predicates.
**Instead:** use next-based early termination, or a separately specified
short-circuit fold protocol. Preserve fault/order/resource behavior, not only
the result on benign finite lists.

[argued] Retire array.fold's old public declaration/native hook with the move.
Its old array-typed native signature must not be attached to the new generic
interface method under the same contract. Start with one source default/helper;
a later accelerator must match pull/callback order, values, faults and the
relevant budget/observation model. **Instead:** version any native registry
change and validate it against the generic body. Specialized code generation
does not justify resurrecting several public semantic definitions.

## 3. Ordered helpers, conversions and Boolean calls

[argued] The helper truth table, over one `compare(left right)`, is:

| compare | less | lessOrEqual | greater | greaterOrEqual | min | max |
| --- | --- | --- | --- | --- | --- | --- |
| less | true | true | false | false | left | right |
| equal | false | true | false | true | left | left |
| greater | false | false | true | true | right | left |

[argued] Fix ties now rather than treat compare-equal values as necessarily
interchangeable for every observation a custom type exposes. No new helper
infers an implementation or overrides default methods. **Instead:** implement
these once on Ordered and retain the existing no-default-override rule.

[cited] F32/F64 deliberately do not implement Equatable/Ordered: their static
equal is IEEE equality and compareTotal is a separate primitive. Interface
calls with additional Self parameters are invalid through erased interface
values under chapter 02, since hidden concrete types cannot be equated.

[argued] Therefore Ordered.min is not silently a float min, and two opaque
Ordered values cannot suddenly use these binary helpers. Use concrete or
generic-bound matching types; retain explicit float operations until an
intentional ordering-wrapper contract is chosen. **Instead:** preserve those
restrictions in the helper examples/tests rather than bypass them for convenient
spelling.

[argued] For S5, a complete contextual boundary can be written as:

```text
let parsed: Result(I32 ConversionError) be TryFrom.convert(text) endlet
let n: I32 be try parsed endlet
```

[argued] **Accept Claude's explicit rule; no soundness problem identified.**
For `try e` checked against success type T inside a callable returning
Result(R E), check e against Result(T E). For an Option-returning callable,
check e against Option(T). The success value has type T; absence/error exits
the nearest callable with its own carrier, rebinding the output and checking
ensures. R need not equal T. This is type-directed context propagation, not
an error conversion, payload cast or execution of a cold plan.

[argued] Therefore `let n: I32 be try TryFrom.convert(text) endlet` is legal
inside a compatible Result-returning callable, with I32 supplied by the
annotation and ConversionError fixed by the enclosing carrier and conversion
contract. Main returning ExitCode must still match/translate. No special
TryFrom-only typing rule is needed. **Instead:** make this the normal short
form and retain the two-let form for explicit intermediate results.

[argued] Preserve these boundaries of the rule:

- Carry the supplied T context's origin. A written T supplies a derived written
  Result/Option context for destination dispatch; an inferred-only T does not
  become a written expected type merely because E appears in the return type.
- Without a supplied T, infer e using the existing rules and extract its success
  type. Do not guess T from enclosing R: try may unwrap a different intermediate
  type, and an ambiguous destination remains ambiguous.
- Standard carriers/error types must match exactly. A known Result(C E) does
  not become Result(T E) by container covariance. If C only widens to T after
  unwrapping, the two-let form can perform that separate written-boundary
  widening; the new rule does not add a container conversion.
- Context comes from the nearest callable, not an outer function across a
  lambda boundary. Try on Intent is still invalid until an explicit permitted
  await produces the carrier. Tests/Void/ExitCode are not new propagation cases.
- A function returning Result(R E) checks a direct return operand at Result(R E).
  Thus `return try e` may require e to return Result(Result(R E) E); try does
  not implicitly wrap an unwrapped R in ok. Nested carriers retain their layers.

[argued] Validate the short conversion, Option context, an intermediate T
different from R, absent/inferred-only context, wrong error/container, nested
callables/carriers and invariant-container widening before implementing it.
The two evaluation cases are unchanged: e runs once, then either its success
is used or its exact absence/error propagates. **Instead:** add this checking
rule to the carry-over/type spec and conformance together; no runtime coercion
or new source spelling is required. Pure stdlib design is converged on that basis.

[cited] Bool's current module functions are ordinary eager functions: their
arguments are evaluated before entering and; if is the short-circuit form.

[argued] Move those functions into Bool because their truth tables concern
one type and no cross-type algorithm is duplicated. Keep their source bodies;
no compiler Boolean operator or lazy special case is added. Bool.and(false
riskyPredicate()) still evaluates riskyPredicate. **Instead:** use conditional
expressions for guards that must avoid evaluation, document this beside the
API, and remove the old module-level exports deliberately rather than support
two canonical paths. I would revisit the namespace choice only on a concrete
import/dependency or comprehension problem, not familiarity claims alone.

## 4. S9: explicit impls are sound; Self and primitives still matter

[cited] Chapter 02 already permits impls nested in generic owning types, as
the four adapter declarations demonstrate. The excluded generic/free receiver
target is the placement inside an interface, not every applied interface target
inside a generic type. Stdlib ownership does not let arbitrary packages reopen
List/Dict or supply conflicting builtin impls.

[argued] Default to **explicit owner-nested conformance for all four existing
types**. Use ordinary next bodies for List/Str/Option where their public or
owner-private pure primitives suffice. For Dict use an explicit pure
compiler-bound next leaf if needed, with the exact typed signature and canonical
order/remainder contract. **Instead:** move conformance evidence into declarations
without pretending the physical representation becomes ordinary source data.
No blanket impls, structural method inference or representation downcasts follow.

[argued] Dict next yields the first canonical key/value pair plus a **Dict**
remainder. Returning a List cursor or a different adapter in the Self slot is
not type-correct, even if an erased caller might fail to notice. Iterating a
sorted entries view and rebuilding a new dictionary at every step can also
introduce needless repeated comparison/allocation. **Instead:** specify the
primitive remainder observationally and permit efficient hidden storage; do
not mandate repeated bulk reconstruction to make a source body look generic.

[argued] Keep the closed Eq/Ordered provisions for atoms/anonymous structural
types and intrinsic storage/role identities where still required. Eliminating
automatic builtin Iter conformance does not remove those separate rules.
**Instead:** update conformance/impl identity and queries to reference explicit
members, and test concrete/erased next, remainder Self, ordering, repeatability
and default dispatch. Missing primitive support is an implementation milestone
obligation, not permission to silently fall back to implicit conformance.

### Two small follow-through additions

[argued] First, the agreed cut of variadics needs a deliberate disposition of
List.of/Dict.of and their function values. Literals replace fixed source
construction, but a runtime list of dictionary entries still needs a bulk
builder. Add a **From(List({K V})) implementation on Dict(K V)**, selected by
a written Dict result, with a fixed-arity typed construction primitive underneath.
Preserve Ordered keys, all-entry processing, last equal-key entry wins and
canonical ordering. Only K needs ordering; V remains unconstrained Any.
**Instead:** share that primitive between literals and this conversion; no
hidden spread/variadic exception or second public Dict.fromEntries alias.
Remove the old .of surface and record its function-value loss/replacement.

[argued] For example, `let d: Dict(Str I32) be From.convert(entries) endlet`
supplies the destination for entries of List({Str I32}). The construction
mechanism must honor the selected Ordered comparison, including user-defined
keys; it is not a raw reinterpretation of unsorted entries. If a backend cannot
call source comparators from a native import, lower through the checked
construction path rather than bypass them. **Instead:** reuse the existing
dictionary-construction semantics with a declared fixed-arity bridge.

[argued] Second, **add Bytes implementing Iter(U8)** as an explicit new stdlib
conformance, not factual carry-over: the current closed set omits it. It needs
only the same pure wrapper/sequence operations already used by Bytes, and lets
generic fold and lexicographic comparison consume bytes directly. The shared
comparison helper then takes two Iter(T) values under T: Ordered, and both
Str/Bytes can delegate with their item types Char/U8. **Instead:** avoid a new
per-Bytes fold or map. This addition is optional to the correctness of the four
existing migrations, but is my concrete regularity default for this pass.

## 5. S7 and the pure parts of effectful modules

[argued] No Length/Concat/Slice interface now. Element/scalar/byte count can
be abstracted coherently; distinct units are a documentation requirement, not
a proof that abstraction cannot work. But a new interface needs result/Self,
unit, bounds, law and cost contracts, and erased Self restrictions still apply
to binary concat. **Instead:** retain primitives/thin delegates until there is
a genuine shared consumer, and distinguish a cheap representation length from
a draining Iter.count. Add no generic method solely to make names look uniform.

[cited] Str indexes/slices in Unicode scalars, Bytes in bytes, List in elements.
For `a` plus combining acute, scalar length is 2 while UTF-8 length is 3; a
scalar slice may return the combining mark alone. Current trim is ASCII
whitespace trimming, not Unicode normalization or grapheme segmentation.

[argued] Preserve those boundary contracts and native/source cases. Do not
rename units or expand trim behavior during this carry-over pass. **Instead:**
keep UTF-8 encode/decode validation, half-open slice bounds, empty needle/split
behavior, malformed text and ASCII-classification cases in the pure corpus.

[cited] The current io/fs/env/time/random files contain closed error enums,
Path/FsKind/FsMetadata data and host declarations, not a hidden body of generic
pure collection transforms. The assert marker module is runner-owned assertion
registration, not an ordinary host-effect implementation.

[argued] Carry the data types, codec/validation responsibilities and shared
pure formatting/encoding helpers into the operation-interface pass. A Path is
ordinary untrusted path data, not a grant; parsing/narrowing a string cannot
acquire a folder. Pure rights/subscope derivation constructs checked policy
data only; actual host existence/resolution is not hidden inside it.
**Instead:** keep grant issuance, argument acquisition, clocks, randomness and
I/O in the checked Proc/driver path, even under deterministic fakes. Driver
UTF-8 conversions should share the existing pure codec contract rather than
invent another inconsistent text encoding.

[argued] Also restore the **existing closed application-indexing surface** as
the K3 default for List/Dict/Str/Bytes: `items(0u64)`, `table(key)`, returning
Option with the documented units. This is a proposal to settle that open row,
not a claim List.get existed. Callee type determines the closed application
category; no user call operator is introduced. **Instead:** retain current
lookup semantics while tuple indexing stays deliberately cut and record fields
keep their accepted projection spelling. If rejected, choose one replacement
API explicitly before writing the pure library references.

## 6. Direct answers to the five questions

[argued] **Q1:** accept Iter.fold and the six Ordered helpers, with exact left
fold, left-biased ties and ordinary eager calls. Also factor the verified
lexicographic duplicate and the inclusive-range duplicate; no exact-carry-over
freeze prevents a justified source-library improvement.

[argued] **Q2:** yes, explicit owner impls are sound without new generic-impl
features. Preserve Dict's same-Self remainder/order through a typed primitive
where needed; representation latitude is not exposed by the interface.

[argued] **Q3:** none of length/concat/slice needs a new interface at this gate.
Keep their units and primitive/delegating implementations explicit, and reopen
on a real generic consumer with its laws/costs.

[argued] **Q4:** Bool static members and the single public List name. Neither
is an empirical win. Bool remains eager and List retains indexed immutable
semantics, with no additional data-structure guarantee.

[argued] **Q5:** cover the existing pure data/error/codec portions of the five
effect modules and their relationship to scope/grant validation. No path-string
constructor, fake RNG or wrapper receives an untracked ability to perform a
host operation. The 17 registry actions and outcomes remain the effect pass's
seed; this review creates no new host API.

[argued] Validation before implementation: shared helper truth/range/tie cases;
pull/callback order and laziness; fold over multiple conformers; Dict canonical
order/remainder; Bytes new conformance; Bool eager-fault cases; scalar versus
byte slices; conversion destination/error context; no duplicate default impls;
and primitive/native/source parity under the supported observations. Only the
snapshot body scan and Python immutable-cursor countermodel ran in this turn.
No revised library, compiler
or whole language is claimed tested. New definitions belong in the agreed
carry-over/ledger and upcoming frozen pilot, with old X1/X1b left unchanged.
