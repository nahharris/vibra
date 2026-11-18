# Codex attack on the complete K3 syntax proposal

[measured] Reviewed `k3-syntax-sheet.md` snapshot SHA-256
`fc83ac99661da63e4e41803e60c74075aa05b9a7d0cf936074aa75fe6888429e`,
plus the later owner/Claude proposals about intents, delivery values, record
`of`, anonymous functions, documentation/tests, function types and imports.
Later proposals supersede the relevant earlier sheet alternatives below.

[argued] Accept the naming defaults: **Int/Str/Bool/List/Option/Result/Unit**,
their type-associated namespaces, camelCase values/fields/cases, and one
mandatory **`returns name: Type`** in declarations. I have no naming dissent.
Keep case-sensitive resolution and casing diagnostics; `fmt` must not rename
symbols. W/R's accepted invariant/binder conditions remain. None of this new
spelling was measured in X1/X1b. **Instead:** version the final grammar and
preserve the old experiments, rather than describe the complete sheet as a
tested language.

## 1. The sheet's examples are not complete programs yet

[argued] `splitCents` calls `share` without declaring it. This repeats the
helper/scope failure found in the T6 cohort. Define a helper before its local
use, capture preceding `base`/remainder bindings, or pass those values to a
top-level helper. Its abbreviated body must not be copied as a complete
reference program. **Instead:** supply checked complete examples, or mark
unelaborated snippets explicitly as sketches.

[measured] The frozen checker rejects an analogous `try List.get(args 0)` in
a function returning an exit-code variant, with `invalid-try`: the existing
Option propagation rule requires an Option-returning enclosing function.
[Source](experiments/syntax-sheet-review/try-carrier.txt) and
[diagnostic](experiments/syntax-sheet-review/try-carrier.json) are retained.
This is an old-language analogue, not an implementation of new `proc` syntax.

[argued] The sheet's `main` has the same carrier mismatch: it returns ExitCode,
not Option. It also ignores the write outcome and returns success regardless.
That can be a deliberate best-effort policy, but does not demonstrate N1's
typed-failure/exit-status acceptance case. **Instead:** match a missing argument
explicitly into an exit code, match the read result, and explicitly handle or
declare the policy for known/unknown write outcomes. Do not invent an implicit
Option-to-ExitCode conversion.

## 2. Types: accept the restricted record spelling, retain its checks

[argued] Accept `type Cents of {amount: Int} where Int.ge(amount 0) endtype`
as an argued replacement for the record `fields` section. `of` currently
accepts a **record shape**, not an arbitrary type expression: `type X of Int`
must not silently mean either an alias or a new wrapper/cast. Tagged variants
keep their case form; enums are fieldless cases. Untagged unions, tuple types
and anonymous record values remain deferred. **Instead:** keep two explicit
data shapes under `type`, not a general type-expression facility whose
nominality and conversion rules are unstated.

[argued] W still needs immutable fields; pure Boolean predicates; written-order
field evaluation; checks before a value escapes; checks at decode/foreign/
restore boundaries; and sealed capability construction. Predicate fault or
exhaustion is not success. Ordinary `.value` may remain a **declared** Option/
Result field; W removes the implicit wrapper projection, not that identifier
everywhere. **Instead:** preserve the previous shape review's conditions and
make host opaque/sealed types explicit exceptions to public data construction.

[argued] Pure does not imply cheap or automatically proved. Permit calls to
supported pure functions in `where`, with bounded constant evaluation where
available and runtime checks otherwise. Only a checked static result or sound
dominating check justifies erasure (D4). Define constant-false diagnostics and
unknown/exhaustion behavior; do not promise complete compile-time execution of
every closed predicate. **Instead:** distinguish predicate evaluation, static
evidence and runtime enforcement, as E2/E3 already require. Pure, total and
statically proved true are separate judgments: accepting arbitrary pure calls
does not establish fault-freedom or termination. A statically total predicate
fragment would need its own supported rules; otherwise faults/exhaustion reject
construction and bounded constant evaluation can return unknown. Do not restore
the abandoned "accepted pure code cannot fault" guarantee through this wording.

[argued] The experimental tree normalizes labeled field order for comparison;
that does not license production evaluation in sorted-label order. Construction
may fault or exhaust even in pure code. **Instead:** preserve written evaluation
order separately from any canonical field map or structural equality order.

## 3. Generics need instantiation and inference rules, not just headers

[argued] `type Pair(A B)` / `Pair(Int Str)` and `fn firstOr(T)` are reasonable
first-order parametric syntax defaults. Header parentheses **bind** type
parameters; use-site parentheses apply constructors, so the global claim
"parentheses always apply" needs this qualification. Start with type parameters
only—no value/dependent parameters, higher kinds or higher-rank polymorphism.
Check each body uniformly under its declared bounds, not only one specialization.
**Instead:** define binder scope, arity, duplicate names and instantiation.

[argued] For functions, infer type arguments from value arguments plus the
expected result/callback type. No runtime type arguments are smuggled into
`firstOr(...)`. Reject unresolved variables with an annotation diagnostic.
`Option.none()` and `Result.error(error: e)` particularly need expected types;
the latter may determine E but not T. A standalone use with no determining
context is not grounds to choose a default type. Generic function values are
instantiated from their expected callback type; reject unresolved/higher-rank
escapes at this gate. **Instead:** use the already required local/signature
annotations and explicitly bound prelude identities, not ad hoc inference.

[argued] Publish the exact prelude Option/Result case and field declarations
alongside their pinned identities before using their projections or propagation
rules. `Float` in a suggested function-type example also introduces no settled
numeric semantics. **Instead:** retain the existing numeric scope until literal,
arithmetic, equality and fault policies for additional numeric types are agreed.

## 4. Function types: settle arity before future tuples

[argued] Accept the owner's `Fn(Inputs Output)` family, but **not its scalar
shorthand mixed with a brace argument list**. My concrete default is one
mandatory outer parameter vector:

```text
Fn({Int} Int)
Fn({Int Str} Bool)
Fn({} Int)
Fn({Pair(Int Int)} Int)
Proc({Str} WriteOutcome)
```

[argued] The first operand is an arity vector, not a proper tuple type: the
last Fn above has one Pair argument, while `Fn({Int Int} Int)` has two Int
arguments. Do not also admit `Fn(Int Int)` as a second unary spelling. If
tuple types later use braces, one tuple parameter must remain nested inside
the mandatory vector; the output remains **one value**, even when its type
eventually is a tuple. This explicitly extends braces beyond named pairs in
this type-syntax context; it is proposed, not a rule already measured in K3.
**Instead:** choose this exact distinction now, rather than reserve an ambiguity
for the tuple milestone. Do not ship both open function-type alternatives.

[argued] Function types describe argument/result types, not result-value
bindings; the mandatory result name belongs to declarations/contracts. Fn is
a pure callable; Proc is an effectful computation builder as defined below.
Their types do not expose exact captured resource families (F3). No ordinary
record or capitalization trick constructs a callable. **Instead:** retain
J5's typed higher-order core and explicitly typed closure conversion/captures.

## 5. Functions as values were not abandoned; anonymous forms are new

[measured] The frozen cards and T5/T6 already pass named functions as values
and use nested closures. Their references parse/check/evaluate in the shared
harness. The absence of surface function-type annotations did not remove
first-class function values.

[argued] Accept the requested anonymous form as an **unmeasured argued
extension**, rather than denying the feature is needed:

```text
fn takes {x: Int} returns doubled: Int
  do return Int.mul(x 2)
endfn
```

[argued] In expression context `fn` has no declaration name; in statement
context a named `fn` declares one. Captures are preceding lexical bindings,
immutable and hygienic; parameters and result binders follow the same scope
rules. An anonymous body is checked just like a named body, and executes only
when called. Use named helpers for self recursion rather than invent an
implicit anonymous self. Captured capabilities/intents retain their provenance.
**Instead:** reuse closure semantics, add expression/parser/type/termination
cases, and update C7/X4: X4 remains unrun, but anonymous syntax is no longer
excluded by default if adopted. No measured superiority claim follows.

## 6. Interface values do not automatically give static dispatch

[argued] The original `console: Console` example treats an interface as a
runtime value type while claiming static generic dispatch (F4). That requires
a choice: implicit generics, erased dynamic interface values, or a concrete
implementor type. The manifest does not by itself explain mixed implementations,
returned handles or higher-order calls. **Instead:** keep F4's explicit nominal
implementors and a compile-time constraint mechanism. A schematic default is:

```text
proc main(C F)
  given {C: Console F: Files}
  takes {args: List(Str) console: C files: F}
  returns code: ExitCode
  do ...
endproc
```

[argued] Here `given` is a proposed type-level section, **not** an executable
`requires` clause. It introduces no real I/O privilege: the values still carry
the grants. Bind C/F and their selected implementations before execution and
pin them in B10's target identity. Reject ambiguity/overlapping selected impls.
Defer generic/blanket-impl machinery not needed at this gate rather than imply
it exists. **Instead:** agree this constraint mechanism or explicitly reopen
the static-dispatch decision; do not call interface erasure "static".

[argued] The interface omits a receiver that its implementation adds, despite
"no hidden receiver." Give the interface an explicit implementor-type binder
(for example `interface Console(Self)`, with Self defined as that receiver
binder), and declare `self: Self` in its operation signatures. Substitution
into an impl then preserves arity. Member contracts belong to the interface;
an impl must match its types/call kind and cannot silently strengthen the
precondition or replace the public postcondition. No general solver proves
arbitrary override refinement at gate 1. **Instead:** inherit the public
contract and retain its checks, with exact signature/receiver matching.

## 7. Associated names and method sugar are different decisions

[argued] `fn Cents.add` is a useful type-associated namespace form. Restrict
such definitions to a type owned by that file/package; imported namespaces
cannot be reopened to replace `Int.add`. Calling `c.add(x)` also needs method
resolution, receiver insertion and a collision policy with Fn-valued fields.
Calling it "no methods" does not remove those judgments. **Instead:** my
first-gate default is `Cents.add(c x)` and `Console.write(console line)`, with
the receiver explicit; defer dot-call sugar. Keep ordinary field projection.
If dot calls are adopted, specify their lowering and ambiguity rejection rather
than silently guessing between a field callback and an associated operation.

## 8. Impl origin and model isolation are orthogonal

[argued] Accept dropping `mode host|model` from ordinary impl syntax. Reject
"a model is an impl holding no capabilities" literally: models may need fake
file/console handles, state and harness-choice capabilities. What must be
absent is **transitively reachable real-effect authority**, including captured
callbacks, delegated intents and selected foreign bindings. **Instead:** keep
an explicit model execution profile in the target/entry manifest and evidence
identity. This preserves F5 without making model/host two source-code kinds.

[argued] An ordinary Vibra impl can delegate to a real underlying capability
(a prefixed/buffered console wrapper), while a trusted foreign implementation
can be an in-memory fake. Source language is not execution mode. Bind foreign
drivers outside Vibra bodies through the typed manifest/ABI. Model targets
must reject real-driver bindings and real handles, separate production request
identities, and enforce any model-only choice operation. Native driver code
remains in the stated trusted base; source annotations alone do not sandbox it.
**Instead:** state both origin and world-binding policy, preserving F4/F5/G7/J1.

[argued] Stateful fake writes/reads need a defined harness-owned model world;
returning success without updating anything is not a useful file/console fake.
`choose` should be an inspectable model-only operation supplied by the harness,
not ambient randomness callable from pure Fn. **Instead:** use controlled
model capabilities and record chosen alternatives/domains; defer full fault
exploration while implementing F7's useful deterministic fakes.

## 9. Await, try and delivery need operational definitions

[argued] Keep **await**, with one precise default: applying a Proc or operation
constructs a **cold Intent(R)**; it does not dispatch. `await intentExpression`
executes it and waits for the outcome, only inside Proc. Allow storing/returning/
composing plans. Pure Fn may build such values but cannot execute them. Effectful
foreign operations cannot masquerade as Fn. Scoped concurrency libraries must
run plans within their bounded parent scope, not create hidden eager tasks.
**Instead:** define this type/lowering contract before saying await marks every
effect. A direct operation call is not an eager effect with await added later.

[argued] Distinguish executing a plan again from retrying one invocation. A
reasonable default is a fresh invocation identity per await; transport retries
of that invocation keep its identity. Record uncertainty if an operation may
have happened. A `once` label is not a proof of exactly-once external effects;
durable receipt/deduplication promises wait for the durable profile. **Instead:**
define identity and permitted retry behavior with late-reply/cancellation tests.

[argued] Accept `delivery Delivery.once()` as ordinary **closed constant
metadata**, with no atoms and no runtime-dependent delivery expression. Give
once/repeatable/unknown precise contracts before choosing classifications.
A consuming readLine is not safely repeatable merely because it is a read:
an unobserved first read may consume one line and a retry consume the next.
**Instead:** use unknown/conservative classification until the actual driver's
receipt/cursor/idempotency contract justifies more.

[argued] Preserve Option try as evaluate-once unwrap-or-propagate to an
Option-returning callable. Result try, if added, unwraps success and propagates
the **same error type** into a Result-returning callable; no automatic error
conversion or propagation into ExitCode. All early returns bind the declared
output and check ensures. Pin the standard Option/Result identities: same-named
or same-shaped user variants cannot hijack a special form. **Instead:** fix
main's explicit matches and add carrier/error-type/capture tests.

## 10. Effect visibility: site listings are useful but not exact consent

[argued] Accept the proposed `show` report: source await site, operation id,
delivery, selected impl/driver and resource scopes for a built target. But an
await may invoke an opaque callback/composite intent, and an interface named
Console could be implemented using a network capability. Listing only lexical
interface calls is insufficient. F2 also includes the reviewed root values and
manifest—not just main's parameter types. **Instead:** report conservative
**transitive may-effects**, include opaque/unknown summaries (F3), and ground
the actual request in the selected driver/target before dispatch.

[argued] The enforceable default is: every real dispatch must fit an approved
upper bound of host operations/resources, checked by the capability/driver
boundary against the protected target manifest. Show plain-language resource
descriptions and precise identities, not interface labels alone. If consent
means approval of each exact runtime request, add a policy decision **before
dispatch**; a static may-effect list or model trace does not provide that.
No wording choice between await/perform guarantees human comprehension or
honest behavior of an untrusted native driver. **Instead:** state the threat
model, the approval bound and what unknown means, with redaction appropriate
to displayed/logged payloads.

[argued] If the owner now requires exact local effect families for every
higher-order call without inspecting its value/body, F3's recorded precision
deferral is reopened. Capability values still enforce confinement, but that
stronger audit policy may need value-aware analysis or effect indices (F1).
**Instead:** do not promise that fn/proc separation and a nicer report alone
meet a stronger guarantee the ledger explicitly deferred.

## 11. Documentation as a string; protected tests as ordinary code

[argued] Accept the latest simplification: one optional `doc` section containing
one triple-quoted Markdown string, **no enddoc and no executable example form**.
The literal ends lexically; following sections/declaration closers follow as
usual. Put doc first after any of the five declaration headers, not "before
do" on types/interfaces that have no do. Markdown code fences are unchecked
prose unless explicitly linked to a checked test; do not secretly parse a
second executable language out of Markdown strings. **Instead:** use ordinary
protected tests and generate linked, labeled examples for inspection if useful.

[argued] Protect the tests' **bodies and oracle helpers/data**, test selection
and runner policy—not only their claims. Otherwise an implementer can replace
an ordinary test body with `return true` while preserving its signature.
Do not freeze the entire transitive graph indiscriminately either: that would
freeze the business implementation under test. The manifest must distinguish
protected oracle code from mutable candidate code, pin both for a run, and
record evidence for that exact combination. **Instead:** carry B3's actual
approval boundary into the new test arrangement.

[argued] A simple convention is manifest-selected, zero-input Fn tests returning
Bool, passed only when true under bounded execution; effect tests are Proc
under isolated supplied fakes. No new declaration kind is needed. Failures,
unrun, unsupported and exhaustion are not passes. This removes the old binding
example clause but preserves approved sampled expectations through protected
tests. E1/X6-4 must be versioned as historical semantics; X6 is not rescored.
**Instead:** keep formal allowed results, protected admission checks and observed
evidence visibly distinct, as the prior shape review requires.

## 12. Triple-quoted string rules must be exact

[argued] Accept triple quotes, Markdown convention and no interpolation. My
minimal proposed lexical contract is:

- The opening `"""` is followed by a newline. A closing delimiter occupies its
  own line with only indentation/trailing whitespace around it. Those delimiter
  lines are not text. A standalone delimiter line cannot be embedded in this
  first form; do not invent an undocumented escape.
- Treat content as raw Unicode scalar text: no escape processing and no
  interpolation. Backslashes, braces, comments and K3 keywords are data.
  Ordinary single-line Str literals keep their separately specified escapes.
- Normalize physical newlines to LF. Remove the exact common whitespace prefix
  of nonblank content lines **and the closing delimiter's indentation**; this
  caps removal so meaningful Markdown indentation is preserved. Relative
  indentation and internal whitespace remain. Blank lines become empty lines;
  join with LF and retain a final LF when content has lines.

[argued] Closing indentation matters: otherwise a doc consisting entirely of
an indented Markdown code block can lose its meaningful indentation through
unbounded dedenting. Define tabs as literal prefix characters, not implicit
tab-stop columns, or choose a different explicit rule before the rewrite.
`fmt` must preserve the resulting Str value and not reflow approved prose.
**Instead:** add golden values for LF/CRLF, empty/blank content, relative code
indentation, backslashes, delimiter-like text and following example prose.
These are proposed rules, not a statement that the sheet already defines them.

## 13. File units/imports still require a module system contract

[argued] Accept no explicit module declaration, files as namespace units, and
one form `import "std/list" as list` at the file top, with a mandatory alias
and ordinary double-quoted constant path. No atoms, bare-path token class,
wildcard/open import, computed import or import-time I/O. Omitting a module
keyword does not remove module identity, resolution, visibility or cycles.
**Instead:** define them explicitly and keep package acquisition separate from
semantic checking over pinned immutable inputs.

[argued] Do not resolve "package root **or** dependency" by filesystem search
order. One concrete policy is logical paths with an explicit first package
segment: `self/...` for this package and a manifest-pinned dependency alias for
others. Choose one exact `.vib` suffix policy; no index/extension guessing,
working-directory dependence, absolute/UNC paths or `..` escape. Logical case
is exact even on Windows. File moves must follow D7's explicit nominal-identity
policy rather than accidentally rebrand types. **Instead:** freeze the loader
mapping and diagnose missing/ambiguous/case-mismatched paths; reject file import
cycles at the first gate unless a complete SCC/initialization rule is chosen.

[argued] Use one namespace tree:

- File alias: `money.someFunction` and `money.Cents`.
- Associated operation: `money.Cents.add(c other)`; definitions live with the
  owned type, not in an importing file that reopens its namespace.
- Prelude types are explicit pinned bindings, so `Int.add` / `List.map` work
  without a special "import the primary type" heuristic.
- Importing `"std/list" as list` exposes `list.List` and thus
  `list.List.map`; **not list.map** unless the file actually exports a separate
  module-level function of that name. Do not make both fallback spellings.

[argued] Aliases are namespace bindings, not runtime values or capabilities.
Scope/shadowing must be deterministic; casing is not the resolver. **Instead:**
keep qualified resolution to stable declaration ids and defer dot-call sugar,
which otherwise adds another ambiguous lookup path.

[argued] Reject blanket public visibility as an unexamined default. Favor
private declarations with explicit exports; exporting a type identity need
not export its constructor/projections. Host-handle constructors remain sealed
regardless of familiar field shapes. The export representation may be a source
modifier or a protected package export table, but choose one before declaring
the syntax sheet complete. **Instead:** provide ordinary public-data and opaque
type cases, private oracle helpers, and public-capability factory tests that
cannot mint undelegated authority. Explicit file namespaces do not satisfy F4
without that constructor boundary.

## Required ledger reconciliation and disposition

[argued] Accepted defaults: owner naming/return spelling, restricted record
`of`, Fn values plus proposed anonymous Fn syntax, await with cold plans,
ordinary Delivery values, no impl mode, plain doc strings, protected ordinary
tests, and quoted import-as. The qualifications above are part of acceptance.
The exact function-type input vector, explicit static bounds/receiver, model
profile, dispatch/consent policy and export contract require agreement before
a coherent full rewrite; they are not established by the old experiments.

[argued] Reconcile B2/B3/B10/B11 (identity/protected tests/imports), C4/C5/C6/C7
(forms/arity/operators/anonymous Fn), D1/D4/D7 (records/invariants/brands), E1/
E4/E8/E9/X6-4 (documentation and changed contract semantics), F1/F2/F3/F4/F5/
F8 (static constraints, root grants, latent authority, model isolation, delivery),
H1/H4 (scopes/cancellation) and J1/J5/L4/L6/L9 (trust, callable lowering and
tooling). In particular **main's types alone**, **interface-value static
dispatch**, and **models having no capabilities at all** contradict the
qualified ledger positions if left unamended. **Instead:** choose these explicit
contracts, correct the illustrative programs, and only then translate the
amended ledger into normative chapters. No new model cohort or production
language implementation is made by this review.

## What would change these defaults

[argued] These are concrete reopening checks, not claims that they have run:

| Disputed choice | Evidence or counterexample that would change the recommendation |
| --- | --- |
| Restricted record `of` and labeled constructors | A needed CLI/library data type that cannot be expressed without an alias/wrapper distinction, or a matched task arm showing a safer single alternative |
| Bounded predicates and first-order generics | A required predicate rejected by the supported fragment, a sound validated proof mechanism, or an inference ambiguity on complete gate-1 programs |
| Uniform Fn input vector | Handwritten zero/unary/binary/one-product callback examples plus an independently parsed future-tuple extension; a smaller single spelling with no arity ambiguity would replace it |
| Anonymous Fn | Matched T5/T6-style tasks showing worse completion/repair results than named helpers; independently of those results, capture/return/indirect-call conformance must pass |
| Explicit bounds and receivers | A fully specified implicit-generic elaboration that handles mixed implementors and returned/captured handles without dynamic dispatch or ambiguous impl selection |
| No dot-call sugar | A deterministic field/callback/associated-operation resolution rule and tasks demonstrating enough benefit to justify the extra judgments |
| Explicit model profile without source `mode` | A real-capability/closure/intent escape through an allegedly isolated target; such a failure requires stronger binding enforcement or a different model boundary |
| Cold intents and conservative delivery | Executable retry/cancellation/late-reply cases establishing a stronger driver contract, or a required scoped-concurrency operation that the cold-plan rules cannot express |
| Conservative host-effect consent | A required local approval policy that broad/unknown summaries cannot satisfy; this reopens F1/F3 rather than merely changing the report's wording |
| Plain docs plus protected ordinary tests | A test-body/helper/selection tampering counterexample, or a matched usability case showing linked ordinary tests cannot carry the approved expectations |
| Raw triple strings | Golden Markdown values that cannot be represented/preserved conveniently by the delimiter/dedent rule; change the lexical rule before freezing it |
| File imports, namespace qualification and private exports | Offline loader/rename/Windows-case cases or public-data/opaque-handle fixtures that expose a resolution or authority leak; explicit alternative export syntax can replace the proposed representation |
