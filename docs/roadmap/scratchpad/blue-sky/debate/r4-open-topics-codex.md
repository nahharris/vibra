# Settling the remaining topics: Codex position

[argued] Choose the defaults below under protocol rule 9. These are decisions
for the redesign, not changes to the normative spec or frozen experiments.
Accept the owner's goal of settling them between us. I request no owner ruling
now; Claude can accept these concrete amendments or identify a counterexample.
The important amendments are parameter patterns, honest inference exclusions,
complete binding scope, and the effect-boundary defaults in section C.

[cited] **Convergence update:** Claude accepted all 17 verdicts and sections
B/C/D without amendment in the t13 message of 2026-11-16. The three substantive
counterproposals at the end are therefore settled. Section E below folds in
the subsequently supplied charter/tooling/runtime/diagnostics inventory and
explicitly identifies the necessary scope corrections; it does not reopen
the accepted surface choices by omission.

[cited] Sources for carry-over semantics are current [01](../../../../spec/01-source-language.md),
[02](../../../../spec/02-type-system.md), [03](../../../../spec/03-effects.md)
and [04](../../../../spec/04-programs-and-packages.md). The latest effects
position is [the concrete ceiling review](r4-effect-ceiling-form-codex.md),
including positional rows, `performs` and restored interface values. Earlier
generic-only and all-constructor-label recommendations are superseded here.

## A. Verdict per proposal

| Item | Verdict | Settled default and reason |
| --- | --- | --- |
| Owner: `let ... be ... endlet` | **Accept, with clarified scope and measurement** | Use `let pattern [: Type] be expression [else statements] endlet`, where square brackets in this description mean optional grammar. No source `=` form. It is a declaration statement, not an expression or a new ordinary lexical block. Initializer bindings appear only after endlet, in the enclosing sequence. Else is permitted only for a refutable pattern and all fallback paths must leave with Never. See section B. Uniform delimitation is a defensible choice; its error/cost benefit is unmeasured. |
| 1. Explicit return | **Amend support, accept behavior** | Every normally returning fn/proc body path returns explicitly, or propagates by supported try. Genuine Never paths need no manufactured return. Return binds the named output and runs ensures; it targets the nearest callable. Tests retain their separate Void/no-direct-return contract. The cards contained explicit return; they did not compare it with tail expressions or prove it better. |
| 2. Optional let annotation | **Accept, with inference limits** | Without an annotation infer the initializer locally, then check the pattern. With an annotation it is a written expected type: it fixes inference and permits the existing explicit widening relations. No inference of interface joins, row widening, numeric defaults or types from later uses. An ambiguous unsuffixed number, empty collection, absent Option value or generic specialization needs context/annotation. No expression `as` at this gate; typed lets supply boundaries, with the sequencing rule below. C8 changes deliberately. |
| 3. Labels/defaults; no variadics | **Accept, with complete spelling** | Keep `labelled {verbose: Bool be false retries: U8 be 3u8}` after required takes; each optional label has a constant default and a real local name. Call as `f(value verbose: true)`. Defaults belong to callable values; types record labels/names/types but no default values. Use `Fn({I32 verbose: Bool} Bool)` and `Proc({I32 verbose: Bool} Void {DirWrite})` for required positional entries followed by optional label/type entries in the input vector. No additional named type-argument slot. Cut variadic declarations/calls at this gate; collection literals and explicit collection parameters replace them. Review List/Dict construction APIs before freezing the standard library, not after implementation. |
| 4. Untagged unions and narrowing as | **Accept** | Defer both explicitly. Tagged variants and patterns remain. No accidental union injection, dynamic type tests or inferred common-supertype joins. Reopen on a gate-1 program needing ad hoc heterogeneous values that tagged variants cannot reasonably represent, or at the library gate. |
| 5. Intent and await | **Accept** | Calling Proc constructs a cold Intent with its complete latent row; await executes it and charges that row to the enclosing execution context. Fn may build/return plans but cannot await. Argument evaluation effects count separately. Store/capture/return must preserve latent bounds and authority provenance. |
| 6. Guards | **Accept semantics; complete type-guard input below** | Requires/ensures are pure Boolean runtime guards, not proofs. Type where is a pure checked predicate on the fully evaluated representation. False, fault, unsupported or exhaustion is not success; all ingress paths validate it. Runtime guards remain unless validated proof or a sound dominating check justifies erasure. Guard budgets and failure categories remain visible. |
| 7. Concurrency | **Accept** | Library operations with structured lifetimes, bounds, cancellation/outcome and ordering contracts. Names wait for the gate that ships them. No hidden eager tasks or unbounded fan-out. H rows stand. |
| 8. Infix | **Accept** | None. Arithmetic/comparison are named operations. `be` replaces the let separator, not equality as a concept; use typed equality operations. Keep numeric/negative-literal boundaries explicit. |
| 9. Small spellings | **Amend details, accept defaults** | Char uses single quotes and exactly one decoded Unicode scalar. Use common string escapes plus the active quote delimiter escape: `\'` in Char, `\"` in Str; reject empty/multi-scalar/surrogate Char values. Dict is `[key: value ...]`, empty `[:]`; `[]` is only List. Preserve ordered-key admissibility, evaluate all entries in written order, last duplicate key wins. Given bounds use `given {T: Printable}` over header-declared parameters. No call-site type arguments at this gate, with the expressiveness exclusion below. No tuple index operation: destructure. Discard `_` only. Selective brace imports are deferred; direct declaration imports suffice. |
| 10. Constructor labels | **Accept; withdraw my prior broader rule** | A field name in the shape yields a label in construction/patterns. Record/case fields are labeled; wrapper payload and tuple components are positional. Empty cases take no payload. `Celsius(21.5f64)` / `Celsius(c)` are coherent again. No implicit casts, wrapper field forwarding or constructor-as-function values. |
| 11. File and atom segments | **Amend operational details, accept case choices** | One ASCII identifier grammar, `[A-Za-z][A-Za-z0-9]*`; declaration casing conventions depend on entity kind, not parsing. File/module segments may be camelCase or PascalCase as proposed; a default import alias retains the actual last segment, without making a capitalized module a type. Atom components follow the actual path/entity names and exact case. Reject filesystem paths that collide under case-insensitive comparison, including on non-Windows hosts, and reject case-mismatched references on Windows. Keep no module declaration, no extension/index guessing, no relative-path fallback or import cycles. |
| 12. Patterns | **Amend: retain parameter patterns** | Reuse the same Pattern `: Type` entry already needed by let; a bare name is the simplest pattern. Required takes entries are ordered positional entries, not argument labels. This preserves an existing feature and the earlier owner ruling, and avoids mandatory helper bindings on every product callback. Optional labeled parameters still require actual names, not destructuring. Exact binding, coverage and scope rules are in section B. |
| 13. Evaluation order | **Accept** | Evaluate callee once first, then every supplied runtime operand in written order, including labeled operands and collection/constructor fields. Bind operands to their declared slots without changing that execution order. Unambiguous interleaving can be accepted; reject missing/duplicate/unknown bindings. Fmt may change layout but never reorder runtime operands. Defaults are already checked constants, not caller-side effectful evaluations. This is changed, not kept. |
| 14. Drivers/fakes/entry/delivery | **Amend to incorporate latest agreement, otherwise accept** | The previous K3 review's model/host-origin split, conservative audit and driver grounding stand. Also adopt restored closed ceilings/interface values and the concrete defaults in section C. Required binding is build-checked, runtime acquisition can fail, optional grants use Option, and typed unknown completion is not a known refusal. |
| 15. Chapters 05–07 | **Accept** | Inventory affected tooling/runtime/diagnostics behaviors and map each to a kept/changed/deferred/dropped disposition. Existing guarantees do not disappear because the source table omitted them. Use stable inventory ids and explicit many-to-one coverage links. |
| 16. Confirmation | **Amend method, accept one pilot** | One newly frozen study over the current gate-1 language, with a matched let-closer comparison. Old K3 references/data remain historical controls, not parsers for the expanded language. Rewriting may start from the agreed table after owner authorization; the pilot does not establish normative correctness or universal readability. Section D gives the comparison contract. |

### Inference exclusions are decisions, not promises that context always exists

[cited] Current chapter 02 permits explicit `types:` when inference cannot
determine every argument. A written result/parameter/let type can often replace
it, but cannot fix an unused generic parameter that appears in neither the
arguments nor result. A written expected callable type cannot fix such a
parameter either.

[argued] Accept the gate cut: **calls with undetermined type arguments are
rejected**, not instantiated arbitrarily. No claim that removing call-site
type arguments preserves every currently expressible call. Keep ordinary
parametric functions and supported generic bindings; do not add higher-rank
inference or invent default type arguments. **Instead:** report the unresolved
parameters and useful annotation sites. Reopen explicit type arguments on the
first necessary standard-library or application call for which every existing
written boundary is insufficient. This does not need an owner ruling now.

[argued] Removing expression ascription also assumes there is a place for the
typed temporary on the executed path. Retain current expression sequencing as
`do statements finalExpression enddo`: its final expression is the value of
that block, **not an implicit function return**. It creates a local scope.
**Instead:** put contextual typed lets inside that block when needed in an
argument or selected branch; do not hoist effectful work from an unselected
branch, change operand order or add a lambda that changes an early-return
target. This settles the previously proposed do cut in favor of carry-over.

### One explicit input for type invariants

[argued] Settle the wrapper/record where-input gap with **`where predicate`**,
where predicate is a pure value of `Fn({RepresentationType} Bool)`. It may be
a named helper or an ordinary anonymous fn. The fully evaluated raw
representation is passed once before the nominal value escapes. This avoids
an implicit `self`/`value` binder and one special free-field scope per shape.
For example:

```text
fn nonNegative
  takes {amount: I64}
  returns valid: Bool
  do
    match Ordered.compare(amount 0i64)
      case Ordering.less()
        return false
      case Ordering.equal()
        return true
      case Ordering.greater()
        return true
    endmatch
endfn

type Cents of I64
  where nonNegative
endtype
```

[argued] For a record, the representation is its structural record type; a
predicate may destructure its input. This amends the earlier free-field Boolean
shorthand; do not ship both spellings. It changes no promise about universal
proof: a predicate may fault or exhaust and construction then fails explicitly.
No broad host capability is minted by a constructor or by a predicate passing.
**Instead:** use one callable input rule for the new type-invariant feature;
do not claim current generic `where:` already defined it. Complete every
retained type-expression's representation before accepting an invariant over
that shape; unsupported representation forms are rejected, not unchecked.

## B. Exact binding and pattern defaults

[argued] Adopt the following shared rules; none depend on casing:

1. **Name classification.** `_` discards and never binds. A bare name resolving
   to a visible module value with a syntactically constant initializer is a
   constant pattern. Otherwise it introduces a binder, subject to ordinary
   no-shadowing/reserved-name rules. A local variable is not converted into a
   comparison pattern. Dotted names resolve constructors/constants, never
   introduce locals. Constructors in patterns use their construction syntax.
2. **Constants.** Retain current decidable classification: literals, atoms,
   void, constructions of constants and references to constant module values;
   arbitrary pure function calls are not constant expressions. Expand constant
   patterns to their value shape for coverage. A name resolving to a nonconstant
   module value/function is not a constant pattern and cannot shadow it as a
   binder. Document the existing consequence: adding a constant can turn a
   prior catch-all-looking name into a value match. Public constant values used
   in contracts/defaults/patterns and inferred public types participate in the
   checked approval/dependency identities; they cannot be changed invisibly.
3. **Pattern forms.** Names, `_`, admitted literals, constants, nominal
   constructors, positional tuple patterns, labeled record/case patterns and
   exact-length List patterns. No rest/spread/or/guarded patterns or implicit
   dynamic tests. Union-narrowing as is deferred with unions. Record/case
   fields may be omitted and are ignored; tuple/wrapper positional arity is
   exact. A missing named field is not an extra positional slot.
4. **Type-directed matching.** Check patterns at the actual scrutinee/argument
   type. Equal-shaped nominal declarations remain different. A pattern never
   narrows by cast or invents interface conformance; private constructors and
   unwrapping patterns remain private. Match interface values only through
   their public permitted operations, not hidden concrete-type patterns.
5. **Required binding coverage.** Ordinary let and required parameters require
   irrefutable patterns. Tuple/record/wrapper patterns are irrefutable when all
   components are; a variant constructor is so only when it covers every
   inhabited alternative. A fixed-length List pattern is refutable for the
   variable-length List type. Refutable let requires else, and an irrefutable
   let with else is rejected as redundant rather than given a second meaning.
6. **Match coverage.** Evaluate the scrutinee once. Arms are ordered and
   arm-local. Reject an arm already covered by earlier arms and reject missing
   coverage with a concrete uncovered shape where supported. Respect inhabited
   cases and finite structural combinations, not just the list of case names.
   Bool's prelude constants cover its two alternatives. Atom needs a catch-all;
   its singleton type can be covered by its one value. Numeric, Char, Str and
   Bytes literal lists do not close their whole type. Float/Void/Never literal
   patterns remain excluded; a discard/binder can cover inhabited Void.
7. **Unknown coverage.** Arbitrary where predicates do not prove arms exhaustive
   or unreachable. Use the supported structural checker over an overapproximation
   of valid representations. If it cannot establish required coverage within
   its bounds, report unsupported; never accept by sampling or silently assume
   an invariant removes the uncovered values. Static exhaustiveness is distinct
   from a general behavioral proof.
8. **Binder collisions.** No duplicate binder within one pattern, no shadowing
   any visible binding, and no rebinding reserved prelude names. Repeated `_`
   is allowed. Sibling scopes may reuse names when neither sees the other.
   Generic binders/constraints follow their own declaration scope; a pattern
   does not introduce type variables.
9. **Let initializer and scope.** Infer/check the initializer once before the
   new bindings exist. With Type, that type is its written expected type;
   without Type, no later-use inference or guessed widening. After successful
   endlet, bindings live to the end of the directly enclosing sequence. Else
   cannot see the successful bindings and has its own scope; every fallback
   path must leave with Never. Function/branch/do scopes still bound their
   declarations. An endlet is the end of a declaration statement, not a scope
   surrounding all uses of its result. Top-level let retains current def's
   single-name pure module-value role; top-level else/return escape is not
   admitted. Put doc/visibility after that header and before be, inside endlet;
   do not use a line boundary to attach attributes.
10. **Parameter scope.** Required parameters use Pattern `: Type` entries in
    takes. Every parameter pattern is checked independently at its written
    type. All resulting binders enter the callable body/contract scope with
    duplicate/no-shadow checks across parameters. Defaults are constants and
    do not see runtime argument values. The output binder is fresh and exists
    only in ensures, not the body or requires. Named self recursion uses a
    named declaration; a let initializer cannot see its own future name.

[argued] Example of retaining parameter patterns without a new keyword:

```text
takes {Pair(left: a right: b): Pair(I32 I32)}
```

[argued] This is the same Pattern `: Type` production as let; `takes {x: I32}`
is its simple case. A literal pattern there is usually rejected as refutable,
not reinterpreted as a label. Required takes entries are positional, while
`labelled` entries have actual names/defaults. **Instead:** keep that one
checker judgment and teach the nested pattern spelling. I would change this
default if representative callbacks show substantial confusion that survives
diagnostics, or a checker/type-soundness counterexample defeats the common
production. Lack of a pilot score is not evidence of a feature's inferiority.

[argued] Endlet is not uniquely unusual in exporting a name beyond its closer:
a local named fn remains bound in the enclosing sequence after endfn, and
module declarations likewise outlive their syntactic ending. **Instead:**
teach declaration scope versus branch/body scope explicitly, rather than claim
every opening keyword starts a uniform lexical scope. Additional endlet cost
and clarity remain measured questions.

## C. Close the remaining effect-boundary choices

[argued] Adopt these defaults now, extending item 14 and the latest effects
review. They are implementation obligations with settling cases, not requests
for more owner taste decisions:

- **Operation identity:** an interface with Proc operations owns one nominal
  declaration-level family, independent of its generic instantiation/resource
  values. Proc members contribute that owner plus written additive performs;
  ordinary Proc declarations write their complete ceiling. Proc/Intent types
  always carry the complete latent row. Fn-only interfaces are not effect roots.
  Use narrow operation-specific standard interfaces when read/write/list/send
  must be distinguished. An impl cannot launder NetSend through DirWrite.
- **Representation access:** add type metadata `representation @private` or
  `representation @public`, defaulting to the declaration's visibility. Private
  representation hides construction, unwrapping patterns and raw projections
  outside the defining module; public methods are exported individually.
  Thus a public LogDir can explicitly hide its representation while ordinary
  public data stays constructible. Intrinsic host grant representations remain
  sealed regardless of metadata; ordinary source cannot override that. This
  is one explicit proposed addition, not behavior already present in chapter 02.
- **Grant composition:** a record is a product of existing leaf grants, retaining
  the operation/resource relation. An erased ReadDir view cannot be downcast,
  but actual sealed grants and checked impl contracts still enforce derivation.
  Rights/scope/limits never increase. Child grants share parent quota accounting
  by default rather than each resetting the budget; the parent remains valid.
- **Subscope creation:** default `sub` to a pure checked construction of a
  narrower relative-path policy token, returning Result for invalid/out-of-scope
  requests. It does not claim the directory exists or resolve links now. Every
  eventual operation enforces root confinement against links/races at the
  driver boundary. An API promising to open/resolve immediately is a separate
  Proc with its own row and typed outcome. No I/O is hidden in Fn derivation.
- **Issuance/config:** raw paths never grant authority. A protected startup
  provider derives from held roots or an explicit reviewed broker, validates
  config/scope and certifies LogDir. The build validates binding recipes and
  versions; acquisition refusal can still fail startup. Optional refusal maps
  to the configured None path. Requests beyond the approved bound are denied
  with typed outcomes by default; interactive host approval is not implicit.
- **Delivery:** `@once` forbids automatic redispatch of a submitted logical
  invocation; `@repeatable` authorizes retry only under the operation's stated
  duplicate-safety contract; `@unknown` supplies no such promise. Known-before-
  submission failure differs from may-have-executed. At gate 1, both once and
  unknown block automatic retry after possible submission. No exactly-once or
  cross-crash dedup guarantee without the later durable profile. No consuming
  read is called repeatable merely because it reads.
- **Binding/world:** foreign drivers come from the narrow typed versioned
  registry/target binding path, never arbitrary source-level host calls.
  Model targets use isolated fake worlds, no transitive real grants/bindings.
  Native code remains in the stated trusted base. Interface-value calls retain
  the chapter-02 restrictions and wrap permitted Self results inside cold
  Intent without erasing rows or admitting forbidden returned-callable inputs.
- **Records/identities:** one versioned row representation keyed by resolved
  interface identities; targets pin code, providers, grant policy, world profile
  and config acquisition/evidence identities. Unknown schemas are unsupported.
  Configured paths and authority checks are runtime facts, not nominal type
  equalities inferred from the string LogDir.

[argued] Reopening cases are concrete: a needed generic operation requires
instance-sensitive roots; a public data API cannot express the representation
boundary; a child escapes or resets limits; relative-path grants cannot satisfy
a necessary object-identity policy; a deployment needs interactive approval;
or a real retry/delivery contract contradicts these classifications. **Instead:**
amend the affected rule on that evidence; do not leave each boundary unchosen
until an implementation accidentally defines it.

## D. One confirmation study, not a new round of design by pilot

[argued] Accept one frozen study with both participating model families and
fresh contexts. Give both arms the **same new semantics**, API, task texts,
types, callbacks and guards. The current arm uses `let p [: T] be e endlet`;
the comparison arm omits the ordinary let's endlet but keeps `be`. Refutable
let-else keeps its fallback delimiter in both arms. This isolates ordinary-let
delimitation rather than silently changing both the keyword and closer. If
the control instead restores `=`, report a two-package comparison, not a
causal endlet result. Do not rerun old cards and call them this confirmation.

[argued] Freeze exact cards, independent reference solutions, parser-to-common-
tree checks, meaningful guard/effect checks, hidden cases, all assigned trials,
transport/budget policy and analysis before calls. Include product/constructor
patterns, constant versus binder cases, annotated/inferred let, defaults,
interface widening, cold plans, forbidden-effect negative cases and optional
grant fallback in the validation corpus. Feature-specific conformance is
required even when no participant task uses that feature. **Instead:** precheck
what the new harness actually supports and mark unsupported cases explicitly;
do not grade authority confinement with an evaluator that has no grants.

[argued] Report functional completion, extraction/parse/type failures, scope and
closer errors, repair status if separately assigned, and provider token counts.
Preregister the task count, repetitions and selection/retention rule in that
study; this review authorizes no silent changes to earlier registered rules.
A small pilot cannot establish noninferiority or general usability, and matched
let arms do not isolate the other newly included features. **Instead:** call
the result a dated confirmation of this package on these tasks/models, keep
owner choices labeled as such, and preserve all earlier raw artifacts.

## Disagreements to take to Hannah

[argued] **None requires an owner ruling now.** Claude has accepted parameter
patterns, nested expression sequencing and the explicit predicate input for
type guards, along with the table and sections B/C/D. The rewrite still
requires the owner's go-ahead, and the pilot still requires its own frozen
preregistration. No spec edit, implementation, cohort run, commit or push is
performed by this position paper.

## E. Runtime/tooling inventory reconciliation

[cited] Checked the relevant anchors from
[the second inventory](vibra-carryover-inventory-05-07.md) against
[runtime](../../../../spec/06-runtime.md),
[tooling](../../../../spec/05-tooling.md) and
[diagnostics/conformance](../../../../spec/07-diagnostics-and-conformance.md).
The defaults below carry over those contracts or mark the required change.
The inventory's implementation-status statements are not new independent
measurements by this review.

### E1. Atoms: code locators are not all ordinary protocol values

[cited] Current diagnostic codes and format tags include hyphens:
`@runtime.division-by-zero`, `@command.program-failed`, `@audit-trace.v1`,
`@host-world.v1` and other versioned documents. Changing them merely to obey
camelCase would change public machine contracts for no source-language benefit.

[argued] **Scope correction to item 11:** `[A-Za-z][A-Za-z0-9]*` governs code
identifier components, not the entire ordinary atom value domain. For atoms
retain hyphens and permit the new identifier case range: each dot-separated
component starts with an ASCII letter and continues with ASCII letters, digits
or hyphens. No empty components or wildcard reference syntax. Existing
diagnostic/result/schema tags retain their registered spellings. An atom's
`@` prefix makes this distinct from allowing hyphens in code names or adding
infix subtraction. **Instead:** in an entity-reference slot resolve the same
dotted path against actual canonical entity names; an ordinary protocol tag
does not become a callable, module, grant or declaration through textual
coincidence. Keep source-name style changes separate from wire identifiers.

[argued] Version an actual code/schema/meaning change under B11, not a blanket
cosmetic rename. Diagnostic aliases and old/new tag auto-detection are not a
compatibility strategy. **Instead:** preserve stable tags that need no change,
explicitly migrate those whose contract does change, and test the machine and
human views against the same diagnostic/result records.

### E2. Completed-call audit and deterministic test worlds

[cited] Runtime, The audit trace, defines structured `@audit-trace.v1` events:
kind, registry op symbol, canonical data arguments in parameter order and a
canonical result value. Each completed call, including an error result,
appends an event after return; a trap or host event appends none for that call.
Tests use fresh deterministic in-memory worlds including stdout/stderr.

[argued] **Carry both over.** The trace is the runtime observation complement
to checked ceilings and manifest/driver grounding, not a replacement for them.
It records completion, not pre-dispatch consent, and absence of a completion
event is not proof that an external action did not happen. **Instead:** retain
the completed-call observation contract and fake/real consistency cases while
keeping F8's unknown-completion distinction visible.

[argued] New capability receivers cannot be blindly serialized as ordinary
audit arguments. Record the primitive driver's data arguments/effective resource
observation through a defined projection, not private tokens, closures or raw
parent grants. New grant/invocation/outcome metadata needs a versioned schema:
adding fields to the existing host event is a major change under its current
rules; a new event kind follows the documented extension rules. **Instead:**
pin the projection/format with the new binding ABI and compare both backends
on identical data. Do not call a changed event `v1` while changing its meaning.

[argued] Keep sequential call-order traces for the first gate. Concurrency
later needs an explicit invocation/completion ordering and comparison contract;
it cannot silently inherit a sequential total-order promise. **Instead:**
make that an affected H/runtime gate obligation, not a reason to discard the
existing audit facility or require durable replay for every CLI.

### E3. Seed the operation library from all 17 host rows

[cited] Runtime, The host registry, specifies 17 exact symbols/signatures and
closed error enums across stdin/stdout/stderr, file read/write/list/metadata,
environment/arguments, clocks and random bytes. All operations return Result.

[argued] Carry those rows and their success/error corpora as the initial
operation-interface inventory. Retain each primitive action's identity:
text/byte operations may affect the same resource but are not silently one
registry row, since their traces and failure behavior are observable. Root
groups may be offered as explicit products, not inferred grants. **Instead:**
map every existing row to its receiver/grant, interface root, data signature,
outcome and driver binding before choosing additional names or collapsing APIs.

[argued] Network metrics is an additional typed-driver integration, not one
of these 17 rows; an abstract NetSend interface does not create a network
provider. The existing set is the gate-1 seed, with fake/type-level network
cases available for checking. **Instead:** add a real network row only with
its explicit integration gate, authority/outcome contract, model and conformance
cases; do not smuggle it through an old filesystem or generic native symbol.

### E4. Arithmetic and tail calls: preserve the complete contract

[cited] Runtime, M3 compiler intrinsic registry: ordinary integer add/sub/mul
and related operations saturate; div truncates toward zero, signed-minimum
divided by minus one saturates, and ordinary div/rem with zero divisor trap.
The checked counterparts return Result with the specified arithmetic errors.
Remainder, shifts and float operations have their separate stated rules.

[argued] Carry that arithmetic meaning over under the new source names, such
as `addChecked`, with the existing primitive registry/equivalence checks.
**Instead:** replace the incomplete carry-over sentence by the full table's
disposition. Do not replace saturation with Python's unbounded integers,
wraparound, or a promise that every plain operation reports a typed failure.
Retain exact zero/overflow/shift/rounding cases in the new corpus. This is
consistent with D4 and the explicit absence of a pure-code-cannot-fault guarantee.
Preserve comparison dispatch through Equatable/Ordered rather than mistake the
pilot's static `int.ge` convenience API for an existing source contract; the
predicate example above is updated accordingly without changing its meaning.

[cited] Runtime, Tail calls, requires activation reuse for every semantic tail
call, including indirect, closure and interface calls. It defines tail position
as returning without further computation; widening already prevents it.
The old text also calls every return operand tail, before language ensures
guards existed.

[argued] **Necessary adaptation for item 1/6:** an actual pending ensures check
is further computation. Ordinary `return f(...)` must not automatically promise
constant-space recursion while retaining a distinct caller guard for every
invocation. Preserve mandatory reuse where no post-return work remains;
classify calls with pending guards as non-tail unless a checked transformation
preserves all required checks without that work. Requires runs before the
callee and is a different issue. **Instead:** update the old syntactic tail
propagation clause alongside the new guards and test both unguarded tail cycles
and guarded recursion. Never erase a guard merely to make a call tail.

[argued] Heap-storing a chain of pending postconditions is not a constant-space
solution; it can still grow with recursion depth. Cold `return await plan`
also needs to account for remaining guards/scope cleanup rather than assume
await implies tail delegation. **Instead:** retain explicit activations and
bounded-memory/host-event behavior, state this contract-related cost, and
require backend parity. This is a semantic reconciliation of two guarantees,
not a preference to send to the owner unless she requires both unconditionally.

### E5. Module values and canonical observable data

[cited] Runtime, Module-value initialization, rejects initializer dependency
cycles including callable aliases and initializes a module value lazily once
per execution, on first read. Fresh executions start fresh state. Function-only
recursion is not automatically an initializer cycle.

[argued] Carry this over for top-level let. Optional annotations change how a
type is obtained, not when a value is executed: infer structurally without
running the initializer. Public inferred types/constant dependencies still
participate in classification and evidence identity. **Instead:** retain cycle,
lazy reuse and fresh-run cases; do not silently make top-level let an eager
load-time side effect.

[cited] Runtime, Canonical value encoding, gives one typed VIBON data encoding
for results/assertions/events. Function values have no encoding; statically
observable function-containing results are rejected, and hidden function
observation has a defined runtime trap.

[argued] Carry the canonical data principle and function-value restriction
over. Live Proc, Intent and host-grant state likewise must not acquire an
authority-bearing persistence encoding accidentally. A versioned nonauthorizing
descriptor for tooling/auditing is a distinct data projection, not serialization
of the callable or grant itself. **Instead:** test observations through interface
erasure and wrappers, retain structural value ordering, and version the changed
type/row/grant interpretation explicitly. Encoding snapshots is not evidence
that live capabilities can be restored into another target.

### E6. Entry results versus the CLI's own status

[cited] Tooling fixes process exits 0–6: ok; diagnostics/test failure; invalid
input; operational failure; unavailable; trap; program-failed. Current entry
Err maps to `@command.program-failed`, exit 6, with its canonical programResult
and no trap/diagnostic. Changing main to return ExitCode is an explicit entry
contract change, not carry-over of that Result entry signature.

[argued] Settle the translation as follows: ExitCode is a pinned ordinary U8
wrapper, zero for application success and nonzero for application failure.
`vibra run` retains the command envelope and reserved exits: application zero
maps to `@command.ok`/0; application nonzero maps to
`@command.program-failed`/6. The **actual application code** remains in canonical
programResult. Tool/host failure and traps retain their distinct statuses.
A distributed standalone entry adapter returns the application's code itself.
**Instead:** do not let application code 2 masquerade as CLI invalid input or
erase the requested application status by always serializing only 6.

[argued] This chooses one ordinary representation, `ExitCode(2u8)`, rather
than making earlier sketch variants success/failure normative. Entry Result
signatures are deliberately replaced, not accepted through a compatibility
bridge. Mark the entry/run/standalone interpretation version and update every
affected roadmap/conformance observation together. Protected tests cover
application zero/nonzero, ordinary operation Err handled by main, startup
failure, host event and trap. **Instead:** compare the result atom, program
value and process status separately, not just the shell's integer exit.

### E7. Convergence status after the inventory

[argued] The 17 open-topic verdicts and three counterproposals are accepted
between reviewers. Sections E1/E4 explicitly correct the atom-grammar scope
and adapt tail position to real post-return work; E6 supplies the previously
unstated entry adapter. The remaining rows are carry-over/mapping obligations
for the forthcoming table and ledger pass. No additional owner taste question
is proposed. Claude should confirm these technical reconciliations in that
pass; do not silently backdate them as experimentally measured decisions.
