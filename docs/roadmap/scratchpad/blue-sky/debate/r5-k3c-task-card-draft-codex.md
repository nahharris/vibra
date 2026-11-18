# X-K3c: task-set draft and full-card outline — Codex

Draft for Claude's registration/card work, 2026-11-16. **Not frozen, no participant calls authorized by this draft.** Ten new task candidates below use the converged A-26 guard forms and avoid X1's defective continuation task. Claude owns the registration; this proposes its common materials and validation obligations.

[cited] Basis: [OT section D](r4-open-topics-codex.md), [the effective design](effective-design.md), [guard rules](r5-guards-static-codex.md), [effect/interface rules](r4-effect-ceiling-form-codex.md) and [pure library decisions](r4-stdlib-pure-codex.md). Actors/scoped concurrency are now owner-delegated to our joint default; this syntax pilot does not need a scheduler or actor surface.

## 1. One treatment difference

[argued] Suggested arm names **K3-E** (current endlet default) and **K3-N** (no closer for ordinary let). Same semantics, names, types, contracts, API, examples, task statements and model isolation.

| Construct | K3-E | K3-N |
| --- | --- | --- |
| Let without else, including ordinary pattern binding | `let pattern [: Type] be expression endlet` | `let pattern [: Type] be expression` |
| Refutable let with else | `let pattern [: Type] be expression else terminatingStatements endlet` | Identical |
| Typed pure module value | `let name: Type be expression endlet` | Same declaration without endlet |
| Contract consequence | `requires C else V`, `ensures C else V`, `where P else E` | Identical |

[argued] A no-else let whose pattern is proved irrefutable by constant evaluation follows the ordinary-let treatment too. Bindings appear after the completed binding statement in both arms, never in their initializer or fallback. K3-N is still layout-insignificant: a complete expression and the next statement/section token determine the boundary. It gains no semicolon/newline terminator. Neither arm changes be, whitespace separators, scope, constructor labels or type inference.

[argued] Generate both prompts from the same material tree. Do not replace arbitrary endlet strings: nested function bodies can contain a refutable let-else whose closer must remain. The parser must enforce the arm, not accept both spellings. One example tree rendered into each card must compare equal after parsing; same for all references. Freeze before calls.

## 2. Common prompt and claim boundary

[argued] Each participant gets one arm card, one **common** API description, the complete task-specific import/type/signature scaffold, and one task statement. Return the complete program file, with clearly marked body/definition holes filled. No hidden fixtures, other arm, repository tools, memory, past outputs, feedback or repair. This is a fresh writing pilot, not the deferred repair arm.

[argued] Every input type, error case, helper, operation, entry name and generic bound mentioned below must appear in that scaffold/API. The final prompts cannot assume a function from an omitted prefix or invent a parse entry not named in the task. These English drafts are **requirements**, not complete executable prompts yet. Reference parsing, type/effect/guard checking and public-example execution must finish before those prompts freeze.

[argued] Recommend making **protected public claim conformance** explicit in the new registration: signatures, type representations, guard clauses and declared ceilings supplied as fixed are not writable holes. Check their normalized trees, not byte formatting. Record raw functional success separately. Algorithm/style requirements (such as using a particular helper where another valid implementation has the same contract) are separate compliance columns. If Claude chooses a different primary endpoint, fix it prospectively and label it; do not silently apply X1's secondary-only structural policy to a new protected-claim task design.

[argued] Exact normalized claim matching is a study instruction boundary, not a proof that every alternative syntax-equivalent claim is wrong. The two arms get exactly the same boundary. Include functional-only and claim-conformant tables so copying a correct contract but failing behavior, or computing behavior while deleting a contract, cannot be confused with full completion.

## 3. Shared API material to prepare

| API group | Minimum frozen content |
| --- | --- |
| Values | [argued] Sized I32/U32/U64, Bool/Str/Void, structural products, List/Dict, field-bearing Option.some(value: …), Result.ok(value: …), Result.err(error: …), fieldless none(). No implicit .value unwrap. |
| Arithmetic/order | [argued] I32.add/sub are saturating; checked forms return Result with named errors. Ordered.less/lessOrEqual/greater/greaterOrEqual and Equatable.equal, resolved to certified primitive operations for the fact checker. No Int.ge or per-List map. |
| Collections | [argued] List.length/append; lookup by application returning Option; Dict.entries; Iter.map/collect/fold and explicit source/accumulator/step order; From.convert from List({K V}) to a written Dict(K V), last duplicate wins, canonical key iteration. No new Dict.insert API. |
| Certified predicates | [argued] `isPositive(I32)` means strictly greater than zero; `inOrder(I32 I32)` means first less than or equal to second. Publish their typed definitions/certificates. `positiveOk` for T6 means Result.ok with positive I32, false for every Err. Exact resolved identities matter for dominating facts. |
| Errors | [argued] Each task names all of its ordinary nominal error cases. Fake filesystem FsError cases include missing, refused, failed and outcomeUnknown, with definitions and case labels fixed. This pilot API is versioned; it is not a claim the full new stdlib has shipped these spellings. |
| Grants/operations | [argued] DirRead.readText with explicit Self receiver and RelativePath returns Result(Str FsError); DirWrite.writeText with explicit receiver, path and content returns Result(Void FsError), creates/replaces within a held scope. Each owns its operation-family row; bodies cannot mint grants. Both use conservative @once delivery. Every explicit await is a fresh invocation. |
| Fixture paths/world | [argued] RelativePath is an immutable checked path passed by the fixture, not a raw string granting authority. Each test receives fresh broker-issued, scoped fake grants or None. Read/write outcomes and ordered completed events are specified; valid typed operation failure differs from pre-dispatch scope refusal. No ambient host access. |

[argued] Operations, predicates, collection primitives and fake grants must be implemented/checked in the new study harness before scored calls. The old X1 evaluator lacks these semantics. It is not enough to accept their tokens, call Python functions by guessed names, ignore contracts or supply arbitrary records as grants. The profile can omit production OS/network implementation, but must say so.

[argued] Task-dependent certified helper definitions belong in that task's fixed scaffold: in particular orderedRange mentions T4's representation, and positiveOk mentions T6's GateError result type. They must not become undeclared magic API names. Where a helper/type is supplied, participants do not redeclare it; where a definition is a hole, its exact required header/representation is supplied. Use explicit public visibility on grader-invoked entries so test imports do not rely on bypassing private access.

## 4. Ten candidate task statements

### T1 — Packet score: nested patterns and an exact pair

[argued] Scaffold supplies a constant module value stopCode = 99i32 and nominal Packet cases: empty; item with field payload of structural type `{points: List(I32) code: I32}`. Implement public `packetScore(packet: Packet) -> Option(I32)`.

[argued] Empty packets return None. For item, destructure payload into its points and code. Match code against the named module constant stopCode; it returns None. Any other code binds a fresh ordinary name. Require exactly two points using a list pattern with let-else; wrong length returns None. Otherwise return Some of `I32.add(I32.add(first second) code)`, in that order. Do not confuse the constant pattern with a catch-all binder.

[argued] Public examples: item([2,3],7) gives Some(12); code 99 gives None; one or three points gives None; empty gives None. Freeze actual K3 constructor renderings, with no commas, in the public example file. Include negative points and saturation in the oracle domain; no arbitrary integer mathematical addition.

### T2 — Normalize a pair: parameter patterns, default and value blocks

[argued] Implement public `normalizePair` with a **record parameter pattern** binding lower/upper from an input structural `{low: I32 high: I32}`, and labelled includeDelta: Bool default true. Return structural `{first: I32 second: I32 delta: Option(I32)}`.

[argued] first is the lesser input and second the greater; equal inputs retain their values. delta is Some of saturating `I32.sub(second first)` when includeDelta is true, None otherwise. Use a tuple value to carry the ordered pair, destructure it, and a value-if/do block for the output choice. The typed takes scaffold fixes the parameter pattern so its binding semantics are exercised independently of local lets.

[argued] Public examples: (8,3), omitted flag gives {3,8,Some(5)}; explicit false gives {3,8,None}; equal inputs give Some(0). Cross-check labelled omission/explicit flag, reverse inputs and I32 extrema. Style constraints beyond the supplied parameter claim are secondary, not hidden functional failures.

### T3 — Proof-required Positive: bare where and bare ensures

[argued] Scaffold supplies certified isPositive. Complete a nominal Positive wrapper of I32 with **bare where isPositive**, public constructor, and the protected signatures of `makePositive(raw: I32) -> Result(Positive PositiveError)` and `readPositive(Positive(inner): Positive) -> I32`, the latter with bare ensures isPositive(result).

[argued] makePositive must branch on isPositive(raw). In its true branch construct Positive(raw) and return Ok; false returns PositiveError.nonPositive(). readPositive returns its unwrapped inner value, with its postcondition established from the type invariant. There is no implicit standard error or dynamic fallback on the bare constructor.

[argued] Public examples: 1 and 7 become Ok(Positive(value)); 0 and -1 become Err(nonPositive). Fixtures call readPositive only with legitimately constructed Positive values. Harness controls separately reject direct Positive(0) and unchecked Positive(variable); they accept the same constructor after a dominating isPositive check.

### T4 — Fallible Range: where-else and preserved constant Err

[argued] Scaffold supplies RangeError.invalid(), certified predicate orderedRange over structural `{low: I32 high: I32}`, and protected Range declaration with `where orderedRange else RangeError.invalid()`. Implement `makeRange(low high) -> Result(Range RangeError)` and `spanOr(low high fallback) -> I32`.

[argued] makeRange uses the ordinary labeled constructor. spanOr binds its Ok case through let-else; on failure return fallback; on success destructure the valid Range and return saturating high-minus-low. Supply a public test/example that explicitly matches the Err from a constant reversed range; this handled constant error is legal, not a compile-time rejection.

[argued] Public examples: (2,5) makes Ok range and span 3; (5,2,-7) yields -7; equal bounds yield span 0. Constructor result is always Result for this declaration, including proven successful constants. The constant-error example is a required public validation case, not a hidden request to fabricate an impossible Range.

### T5 — Relational precondition: establish the same predicate

[argued] Scaffold gives protected `chooseUpper(low: I32 high: I32) -> I32`, bare requires inOrder(low high), bare ensures inOrder(low result). Implement its body by returning high. Also implement `orderedChoice(low high) -> Result(I32 OrderError)`.

[argued] orderedChoice branches on inOrder(low high); true calls chooseUpper and returns Ok, false returns OrderError.reversed(). The body/postcondition proof is exact substitution, not a new arithmetic theorem. Do not swap the arguments or use a different predicate whose relation the checker does not know.

[argued] Public examples: (2,5) gives Ok(5); (5,2) gives Err(reversed); equal inputs are accepted. Controls reject unguarded calls with unknown inputs, swapped calls under only inOrder(low high), and a constant-false precondition. No source-level proof token or assume statement exists.

### T6 — Public consequences: preflight rejection and candidate repair

[argued] Scaffold supplies GateError.disabled(), GateError.nonPositive(), certified positiveOk and these protected clauses for `gateAmount(enabled: Bool amount: I32) -> Result(I32 GateError)`: requires enabled else Err(disabled); ensures positiveOk(result) else Err(nonPositive). Fill the body to return Ok(amount). Implement `viaGate` with the same input/output types by passing gateAmount through a helper whose parameter is the corresponding **plain Fn type**, then calling it.

[argued] Required behavior: disabled returns disabled even if amount is positive or negative, skipping the body and ensures. Enabled positive returns Ok(amount); enabled zero/negative returns nonPositive after checking the candidate. Because the requires has else, no caller proof obligation prevents the plain callable conversion; its entry/exit checks remain executable.

[argued] Public examples include (false,4) -> disabled, (false,-4) -> disabled, (true,4) -> Ok(4), (true,-4) -> nonPositive. These distinguish bypassing postprocessing on preflight rejection and retaining consequences through callable conversion. The public signature is not a promise that positiveOk is true of the final Err.

### T7 — Optional read grant: absence is a fallback, refusal is an error

[argued] Implement public `proc readOptional(path: RelativePath reader: Option(DirRead) fallback: Str) -> Result(Str FsError)` with **performs {DirRead}**. None returns Ok(fallback) and performs no operation. Some supplies the granted interface value: create and await its readText plan, returning its Result.

[argued] Do not use fallback on a Some grant whose operation returns missing/refused/failed/outcomeUnknown; propagate that typed outcome. Do not turn None into ambient filesystem access. The fake broker supplies only the delegated scope. No source code constructs an arbitrary grant from path/record data.

[argued] Public worlds: None -> fallback with zero completed events; permitted existing file -> its text with one event; admitted read returning missing -> Err(missing), one event. Additional private cases must distinguish valid typed operation refusal from policy denial before adapter invocation, with their specified event counts. This measures fake-grant semantics, not real Windows confinement.

### T8 — Reusable cold plan with a dynamic Proc guard

[argued] Scaffold supplies `proc emit(enabled: Bool writer: DirWrite path: RelativePath content: Str) -> Result(Void FsError)` with requires enabled else Ok(void) and performs {DirWrite}. Fill its body to await writeText. Implement pure `prepare` with those same typed inputs, returning `Intent(Result(Void FsError) {DirWrite})` by forming emit's plan; implement `proc runTwice` with those same inputs and performs {DirWrite}, returning Result(U32 FsError), which stores that plan once and awaits it twice in sequence, using try/match to propagate the first failure and returning Ok(2) only after both complete.

[argued] prepare must run no guard/body/host operation. Each explicit await invokes the plan afresh. Disabled calls return Ok(2) with no writes; enabled two-success calls return Ok(2) with two writes. If first fails/has unknown outcome, do not execute a second invocation; if second fails, preserve the first completed write and propagate the failure. No implicit retry, rollback or memoization.

[argued] The evaluator must test prepare's coldness using an internal executable-plan action, not serialize a live Intent as a persistent value. It must support the actual declared row, capture environment and guard entry. For source/card writing this is a small sequential task; it introduces no concurrency API.

### T9 — Generic Option transform and an anonymous closure

[argued] Implement generic `mapOptional(A B)` with explicit given {A: Any B: Any}, taking Option(A) and Fn({A} B), returning Option(B): None remains None; Some transforms once. Scaffold also supplies `clampPositive(value: I32) -> I32`, requires isPositive(value) else 0i32; fill its body by returning value.

[argued] Implement `adjust(item: Option(I32) offset: I32) -> Option(I32)` by using mapOptional with an anonymous Fn that captures offset, applies clampPositive and adds offset with I32.add. A plain Fn can call/pass clampPositive because its precondition has an explicit consequence. Do not perform host operations in that closure or invoke it for None.

[argued] Public examples: None stays None; Some(3), offset 2 -> Some(5); Some(-3), offset 2 -> Some(2). Validation controls separately reject conversion of an otherwise identical **bare-requires** function to the same plain Fn when its input type does not establish that obligation.

### T10 — Latest entry index using Iter and destination selection

[argued] Scaffold supplies nominal Row with labeled key: Str and amount: I32. Implement `indexRows(rows: List(Row)) -> Dict(Str I32)` by mapping rows to positional `{Str I32}` pairs, collecting, and using From.convert at a written Dict(Str I32) expected-type boundary. Empty input gives a typed empty dictionary; a later equal key replaces an earlier value. Implement `latest(rows key) -> Option(I32)` by dictionary lookup through application.

[argued] Use the shared Iter algorithm, not a nonexistent List.map/Dict.insert. Public examples: rows [("b",1),("a",2),("b",3)] produce entries [("a",2),("b",3)]; latest b is Some(3), unknown key None; empty input empty dict. The API must fix Str order and duplicate-key semantics. Functional oracle checks those behaviors; algorithm-compliance checks stay separately reported unless the registration explicitly protects a supplied helper call.

## 5. Feature map and corpus coverage

| Feature | Main writing tasks | Required independent controls |
| --- | --- | --- |
| Product/constructor/constant/list patterns | [argued] T1–T4, T9–T10 | Duplicate/no-shadow binders, record omission, tuple arity, exact-list length, constant-versus-binder precedence, inaccessible private constructor/pattern, exhaustive/unreachable match. |
| Ordinary let vs let-else | [argued] All, especially T1/T2/T4/T8 | Same common trees; closers only removed for no-else statements; failed-pattern bindings not visible in else; every fallback terminates; crossed closers and wrong-arm syntax rejected. |
| Bare/else guards and author error | [argued] T3–T6/T8 | Wrong static construction, preserved constant Err, proof unknown versus refuted, candidate-dependent fallback countermodel, fallback evaluated once, exact error type/no widening. |
| Callables/generics/defaults | [argued] T2/T6/T9 | Same-typed positional order, label permutation without operand reorder, defaults pinned, plain callable retains guard entry, bare-obligation conversion rejected. |
| Effects/cold plans/Option grant | [argued] T7/T8 | Fn await rejected; empty or wrong ceiling rejected; NetSend callback denied under DirRead/DirWrite; stored/captured Intent row retained; repeated await fresh; no grant forgery or foreign-world acceptance. |
| Written typing / interface values | [argued] T7/T9/T10 | Written concrete-to-interface boundary only; no container covariance, no chained erasure, erased value implements no interface, permitted Self result handling. |
| Other full-card forms | [argued] Corpus controls rather than every writing task | Import/case/visibility, nested owner impl, docs, test Proc context, do/if/match expressions, all literal categories, checked arithmetic and observation errors, nominal recursion/finite-size versus import/initializer cycles. |

[argued] Include explicit negative mutants for each semantic check before any model call. A task passing despite a disabled effect/guard check is a harness defect. The two arm parsers/checkers/evaluators share semantics but have independent surface tests. References come from independently reviewed common trees, not solely the printer/parser pair agreeing with itself.

## 6. Full K3 card outline

[argued] This is a **complete outline**, not the frozen card text. Distinguish the language's full admitted grammar from the experiment's executable profile. A construct omitted from writing tasks still needs card explanation and corpus validation if advertised as executable. If the grader cannot implement one, name the exclusion prospectively; do not call the pilot a test of the full language.

1. [argued] **Source/lexical:** one file/module, imports/visibility, typed opener/closer nesting, no comma/infix/grouping, whitespace item boundaries, identifier versus atom grammar, negative literals, comments, exact string/char/doc-string rules. Only the ordinary-let row varies by arm.
2. [argued] **Declarations and names:** import, type, interface, nested impl, fn, proc, typed module let, test; defaults/private representation; no shadowing, forward references, permitted symbolic recursion and banned prerequisite cycles. No top-level impl, method receiver sugar or implicit environment names.
3. [argued] **Types/construction:** sized primitives, structural products, named variants/records/tuples/wrappers, label rule, List/Dict/Option/Result, empty literal context, Fn/Proc vectors/closed rows, generic header/given bounds, explicit widening/Self restrictions, no call-site type arguments.
4. [argued] **Bindings/patterns:** exact OT section B rule text updated for constant-evaluated irrefutability, plus arm-specific let line; module constant resolution, no casing-based binding, exact list/tuple patterns, omitted record fields, parameter irrefutability, match coverage and let-else scope/termination. Show bindings remain in scope after endlet.
5. [argued] **Expressions/calls:** callee then operands in written order, record field access versus path, application lookup, if/match/do values, return and carrier-correct try, explicit discard of fallible values, labelled defaults, anonymous captures and generics. No per-type map or hidden numeric promotion.
6. [argued] **Contracts — exact A-26:** bare requires at call/plan construction; bare ensures at all accepted-body returns; bare where constructor T; explicit requires-else preflight bypass; ensures-else candidate binder, single fallback/no revalidation; where-else constant author error and Result type; unchanged types under proof; certified pure/fault-free expressions and seven deterministic proof rules.
7. [argued] **Proc/effects:** cold construction, per-await dynamic entry guard, reusable plan/fresh invocation, exact latent rows, interface owner plus additive member row, no await in Fn, tests execute under runner-owned Proc context. Authority is a sealed value, distinct from ceiling names.
8. [argued] **Optional grants/outcomes:** required versus Option grant, None fallback versus request refusal, typed denial/unknown outcome, binding/fake-world boundary, no minting from policy/path, @once versus explicit repeated await, completed-event limitations. Do not promise production confinement from the fake profile.
9. [argued] **Library/API table:** exact versioned functions, parameter/result types and units; generic Iter behavior, left fold, dictionary ordering/last-wins, eager Bool operations, checked div/rem. No helpful competing-arm code snippets in the common prose API.
10. [argued] **Worked example:** one complete non-task program rendered from a common tree, combining product/constructor patterns, at least two ordinary lets, one let-else, bare and explicit guards, typed module constant, optional grant and a cold plan. Both cards must parse/check to that same tree and pass independently authored examples. No ellipses or undefined helpers in the actual card.

[argued] The complete outline will be longer than X1's small card; measure its actual tokens and participant output needs before freezing budgets. Do not advertise broad model ease from a pilot dominated by unsupported concepts, leaked reference algorithms or an unknown output cap. Keep old X1/X1b cards, runs, freezes and decisions untouched.

## 7. Hand-off to the registration

[argued] Suggested matrix for Claude to accept/amend: two arms × ten tasks × three fresh runs × two families = 120 assigned trials, no repair. This is only a proposal; task count, ordering/seed, model/effort identities, transport/budget policy, primary/secondary endpoints and retention rule belong in his frozen registration. Retain all failed assignments and raw token/parse/check/runtime/proof/status columns. Neither this draft nor a model-free parser test is participant evidence.

[argued] Before a freeze-ready notice: publish the exact two cards/common API/task prompts and reference/golden trees; independently review all ten entry/type inventories; run semantic/effect/guard mutants and both surfaces' references; pin parser/checker/evaluator/runner sources plus external fixture hashes; verify old freezes unchanged. Hidden test cases stay outside the repo and participants' contexts. Model readers/OS adapter tests are separate work, not extra arms smuggled into this matrix.

## 8. Accepted sequencing amendment — after the real M1/M2 implementation

[argued] **Accept Claude's sequencing proposal.** Write the coordinated specification with ordinary endlet as the standing default. Keep X-K3c as an accepted study design/draft registration, **not frozen and not run**. Do not build a second semantic evaluator for this pilot. The rewrite go-ahead does not wait for the study; the full new surface must continue to be labeled untested on model writers until it runs.

[argued] Use the actual v0.0.2 M1 checker/interpreter and M2 cold-plan/effect/grant test world as the common semantic engine for both arms. Only strict experimental surface parsing differs; K3-N is a syntax comparison, not an additional supported production dialect. It must produce the same checked input trees and cannot be graded by heuristic insertion/deletion of closers or permissive mixed syntax. Keep all reference/golden, negative-mutant, isolation, transport and budget requirements above.

[argued] T1–T6 and T9–T10 can be developed and reference/control-validated after M1. **Recommended timing for scored calls: all 120 after M2**, against one frozen engine/source/API/task/fixture set and one preregistered assignment scheme. Do not collect the first eight tasks against an M1 build and quietly finish the rest against a changed M2 checker. A split scored study would require an explicit prospective phase/version policy; it is unnecessary for the proposed confirmation.

[argued] At M0, record the accepted question, two arms, ten-task matrix, endpoints, endlet default and later gate. At the end of M2, complete the registration, validate both arms/references through the real engine and freeze **before** participant generation. Fix material/harness defects before that freeze; any later amendment or rerun retains its provenance and denominator. The Windows adapter feasibility record remains an independent M0 prerequisite, not deferred with the model pilot.

[argued] Update the roadmap/effective-list references that currently place the **pilot run** in M0: M0 designs it; M1 prepares the pure reference/control cases; M2 supplies the complete engine and hosts the frozen confirmation. Gate-1 product/distribution evidence remains separate. This is agreement to the scheduling change, not a freeze-ready notice or permission to claim an experimental result.
