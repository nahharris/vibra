# Concrete effect ceilings and runtime-scoped grants

[argued] This refines the [required/optional effects review](r4-required-optional-effects-codex.md)
with the requested source form and attacks proposals (a)–(g). Prefer ordinary
records of **sealed operation-specific grants** over a builtin `Cap(operation
set)` at this gate. Restore checked closed effect ceilings as the complement
to those values. Neither records nor nominal brands alone supply a grant
issuer, host confinement or proof that a runtime path is the configured log
directory. Those boundaries must be explicit. This version incorporates owner
rulings 8/9: declarations use `performs`, rows are positional in types, and
interface values are carried over. It supersedes my generic-only workaround.

[cited] The starting contract is current [chapter 03](../../../../spec/03-effects.md),
Function effect rows: omitted is empty, written ceilings are not inferred,
body effects are computed, callable rows are exact, interface implementations
respect the contract, operation owners contribute implicit roots, and rows
have one canonical record representation. The proposals below adapt that
contract to interface operations and cold plans; they are not measured K3.

## 1. Smallest proposed spelling

[argued] Accept the owner's section **`performs {DirWrite NetSend}`**, on Proc declarations,
anonymous Proc declarations, test declarations, and effectful
interface member contracts. The braces contain a closed list of resolved
operation-family names; they are not a runtime record or a value expression.
`performs` remains a may-effect ceiling, not a promise that every listed action
will occur. Inside types the row is positional, with no section word:

```text
Proc({Str} Void {DirWrite})
Intent(Void {DirWrite})
Fn({WriteHandle} Intent(Void {DirWrite}))
```

[argued] The last type is a **pure plan factory**. Calling it performs no host
operation, while awaiting its returned Intent may write. `Proc({Str} Void)`
has the empty latent row and cannot stand for an arbitrary effectful job.
Fn has empty immediate effects by kind; a nonempty immediate `performs` on Fn
is rejected. It may return an explicitly effect-bearing Proc/Intent. **Instead:**
retain one arity-vector spelling and one `performs` section, and carry the
latent row in the returned value's type rather than mark the pure factory as
performing the future effect.

[argued] Illustrative declaration signature, not a complete library program:

```text
proc appendLog
  takes {logs: LogDir entry: Bytes}
  returns outcome: WriteOutcome
  performs {DirWrite}
  do
    return await LogDir.write(logs entry)
endproc
```

[argued] This assumes the protected LogDir module exports the indicated method
and result type. LogDir wraps an interface value, not an exact static path.
The performs section says what execution may attempt, not where that writer is
authorized. **Instead:** publish those two distinct contracts together; do not
infer a second resource name from the effect row.

### Positional rows beside tuple types

[argued] Proc requires an input vector and one result TypeExpr, followed by
one optional row. Intent requires a result TypeExpr followed by one optional
row. Braced products recursively consume one TypeExpr; their position decides
whether braces denote a type or a row:

```text
ProcType   = Proc "(" TypeVector TypeExpr EffectRow? ")"
IntentType = Intent "(" TypeExpr EffectRow? ")"
FnType     = Fn "(" TypeVector TypeExpr ")"
TypeVector = "{" TypeExpr* "}"
EffectRow  = "{" OperationInterfaceRef* "}"
```

[argued] This is a type-grammar sketch, not source code. The final slot is a
different kind of operand; it is not a singleton tuple argument masquerading
as an effect set. Examples:

```text
Proc({Str} {I32 I32} {DirWrite})
Proc({{I32 I32}} {I32 I32} {DirWrite})
Intent({I32 I32} {DirWrite})
Proc({Str} {DirWrite})
```

[argued] The first returns a two-I32 tuple; the second takes one such tuple
and returns one. The third is a plan returning that tuple. The last returns
an interface-valued singleton tuple with an **empty ceiling**; it is not a
write Proc missing its result type. Do not allow result-omission shorthand.
Row entries are resolved root names, not nested TypeExprs or record pairs.
**Instead:** use these fixed productions and golden parse trees. The forms
are syntactically distinguishable, although the final example deserves a
reading example. Canonically omit an empty final row.

### Resolution and checking rules

[argued] Adopt these rules, preserving chapter 03 unless noted:

- Omitted `performs` means the empty written row, never infer-and-publish. Compute the
  body row and reject a body exceeding its written/default ceiling. An unused
  written root is a warning, not proof that the declaration is least privilege.
  For operation members the written row is additive and the owner is implicit;
  Proc/Intent type rows always denote the complete latent row.
- Entries are lexical references resolved through imports to nominal operation
  interface identities. No atom entries, computed metadata, structural method
  name matching or resource path strings. Two aliases for one identity are a
  duplicate, not two effects. Wrong-kind references are errors.
- Rows are finite, order-insensitive and closed. Canonical output sorts by
  resolved identity and preserves the existing `EffectRow {roots, tail: None}`
  / `{"roots": [...]}` record shape, with a versioned change in root kinds.
  Persisted entity-reference encodings remain schema-controlled.
- Proc/Intent types compare rows exactly at this gate. Body containment does
  not imply callable subtyping. No effect variables, row polymorphism, implicit
  widening to any effect or unknown-as-empty. Explicit wrappers may adapt a
  smaller declared row to a larger contract as in current Vibra.
- A test's row describes what its isolated world may simulate. It grants no
  production authority and does not change the protected test/world policy.

[argued] The brace row is an explicit contextual grammar addition, as the Fn
arity vector is; it is not justified by claiming every brace form is a product.
Canonically omit an empty row where omission is permitted. **Instead:** give
this section its own parser/schema production and update C1/B11 rather than
invent a new ordinary Set value or a second effect annotation syntax.

[argued] Carry over effectful anonymous functions as anonymous
`proc ... performs {...} do ... endproc` values. Creation is pure, execution
is cold, and their latent rows are checked as for named Proc. Tests execute
in a runner-owned Proc context under fakes while keeping their separate Void
result and no-direct-return-or-try rules. **Instead:** specify these contexts
without allowing await inside ordinary Fn bodies.

## 2. What an effect name denotes

[argued] Accept operation-specific nominal interfaces such as DirRead, DirWrite,
DirList and NetSend as the gate-1 operation vocabulary. An interface exposing
Proc operations owns a nominal operation-family root. Pure Fn-only interfaces
do not thereby become effects. A Proc member contributes its owning interface
root **plus its declared additive effects**, directly translating the current
deffect owner/additive rule. **Instead:** make that language rule explicit;
ordinary interface conformance alone is not a proof of authority or an effect
classification inferred from a friendly name.

[argued] An operation interface with one effectful entry gives the requested
granularity. Grouping read/write/delete behind one Console/Files root loses
that granularity: allowing that root permits its contract's grouped actions.
**Instead:** start with narrow standard operation interfaces. General interfaces
need not all be single-method, but the documented precision follows the chosen
root granularity. Do not advertise an operation-exact ceiling for a broad group.

[argued] For a DirWrite operation whose additive row is omitted, its complete
ceiling is `{DirWrite}`. An implementation forwarding to another DirWrite can
fit that ceiling; an implementation awaiting NetSend cannot. The implementation
inherits/matches the operation contract's additive row; it cannot unilaterally
append NetSend and leave callers believing the old row. **Instead:** reject
that implementation, or deliberately change the public contract/use a different
interface. Preserve exact inherited rows and no silent refinement of contracts.

[argued] Trusted host bindings must be checked against the registry/manifest
identity of their declared operations and assurance boundary. An unchecked
native backend can lie about any interface; no source type system removes it
from the trusted base. A "write" backed by remote transport must not silently
erase the extra declared host operation from policy. **Instead:** ground reports
and binding validation in approved drivers, and state which host operation
families are actually modeled. This is not a proof about every packet an OS or
remote filesystem might emit internally.

## 3. Cold plans: where the row is charged

[argued] Apply these execution/type rules:

| Expression | Immediate effect contribution | Result information |
| --- | --- | --- |
| Name/capture a Proc value | None | Its exact latent row remains in its type |
| Call Proc(args) | Argument/callee evaluation effects only; no operation dispatch | Intent with ResultType and the same latent row |
| Store/return/put an Intent in a record | No dispatch | The latent row is preserved, not erased by storage |
| `await plan` inside Proc | Effects of evaluating plan expression, plus plan's latent row | Plan's outcome type |
| Await inside Fn or an effectful host binding masquerading as Fn | Rejected | No capability exception to purity |

[argued] Composition unions the work it will execute; evaluating arguments
can itself execute an await in an enclosing Proc before a new plan is built.
Invoking a pure factory is not executing the returned plan. **Instead:** check
these stages separately in typed lowering, including early failures, captured
jobs and nested returned plans. Unknown foreign/opaque latent effects cannot
satisfy a narrower closed contract.

[argued] A runner accepting
`Proc({Str} Void {DirWrite})` may accept an opaque job with that checked
row, and must reject one typed with `{NetSend}`. The runner writes its own
`performs {DirWrite}` if it awaits the job. Neither it nor the job needs a
redundant explicit DirWrite capability parameter when the job already closes
over the delegated grant. **Instead:** preserve delegation provenance and the
latent type, rather than insist authority must appear as a direct parameter
at every call site.

## 4. Why capability types and ceilings are not duplicate requests

[argued] A capability parameter supplies a **particular authority value**;
its scope may be learned only at runtime. A ceiling bounds **possible execution
actions**, including hidden/captured delegation. It grants nothing. A procedure
may hold a writer without awaiting it, giving an empty execution row, or await
a job with a write row without receiving a writer directly. **Instead:** keep
both declarations, but put paths/endpoints/rights on grant values, not repeated
as invented constants in every effect row.

[argued] "Exactly the least access" is a policy to request narrow grants,
not an automatic general theorem about program necessity. A checked may-effect
union can overapproximate unreachable work; a programmer can request an unused
capability or an unnecessarily broad directory. **Instead:** check that code
stays within its ceiling and grant, warn on evident unused authority/effects,
and make exact minimization a separately justified analysis. Do not claim the
existing closed row algorithm proves semantic least privilege for arbitrary code.

## 5. Attack on the records-and-brands proposal

[argued] Prefer (a)–(g) with these amendments:

| Proposal | Verdict and required correction |
| --- | --- |
| (a) One small nominal interface per operation | Accept as the standard-library granularity default. Define action semantics: append, overwrite, create, delete and list are not automatically the same permission just because all concern a directory. |
| (b) A capability implements operation interfaces and holds runtime scope | Accept, provided grants are sealed and unforgeable. An ordinary impl provides behavior, not the right to issue host grants. A single concrete handle implementing several operations is not attenuated merely by viewing one interface name. |
| (c) Anonymous record combines leaf grants | Accept explicit product construction and projection. Its exact field types do not acquire record width/subset subtyping. Combining existing tokens mints no new leaf grant. |
| (d) Resource narrowing by library operation | Accept monotone narrowing from held authority. Name whether derivation is pure lexical policy construction or effectful resolution/opening returning Result; enforce actual confinement at dispatch. |
| (e) LogDir wrapper/private constructor | Accept as an abstract application-role certificate, with a protected issuer and actual grant validation. The brand alone neither proves config correctness nor gives generative identity to each runtime directory. |
| (f) Interface names in closed ceilings | Accept under sections 1–3: nominal roots, implicit owner/additive contracts, exact Proc/Intent rows and checked implementations. Not simply a report or a set of nominal names an impl may falsely declare. |
| (g) Option for optional grants | Accept. Required target bindings are build-checked, acquisition can fail at startup, and optional refusal remains distinct from later operation failure or an unknown outcome. |

### Record composition preserves a relation, not a Cartesian product

[argued] `{read: readA write: writeB}` grants read on A and write on B. It does
not grant read/write on both A and B, nor establish that A equals B. **Instead:**
represent leaf grants as operation/resource relationships, preserve each token's
scope, and check shared resource identity at issuance when an API requires
read/write on the same directory. Record structure alone supplies no such proof.

[argued] Destructuring away `write` attenuates a bundle only if its remaining
`read` leaf cannot recover or exercise write authority. A leaf that is really
the same broad read/write handle can still expose its broader impls; a public
parent field leaks it directly. **Instead:** issue sealed read-only views, then
construct the smaller exact record explicitly. Keeping the original bundle at
the caller is allowed and is not revocation. Narrowing and composition must
preserve parent rights, scope, limits and target/world lineage monotonically.

### Interface values are now retained: port their restrictions too

[cited] Owner ruling 9 restores interface values. Current [chapter 02](../../../../spec/02-type-system.md),
Interfaces and Type ascription and widening, supplies these rules:

- Widen a conforming concrete value to a written expected interface type;
  conformance is explicit, not inferred from matching method names. No
  inferred least-upper-bound conversion and no structural/container covariance.
- Widening is pure and occurs at most once at a boundary. No chaining or
  automatic interface-to-interface conversion. Generic arguments stay invariant.
- An interface value erases its concrete payload type. A call through its
  receiver selects the implementation for that payload under the closed-world
  implementation rules. It is not monomorphic generic dispatch alone.
- An interface value implements **no interface, including itself**. It cannot
  satisfy a generic bound requiring that interface; Any is the vacuous bound
  exception, not a way to recover the payload. The interface-receiver call rule
  is distinct from generic-bound satisfaction.
- Self in supported result positions is re-exposed as the same applied
  interface value type; this includes the iterator's remaining-Self result.
  No concrete payload type becomes visible by returning it.
- A member taking Self in another parameter cannot be called through an erased
  interface receiver: two hidden concrete types cannot be assumed equal.
- A result callable accepting Self as an input also makes the call invalid,
  since substituting the interface would admit unrelated concrete payloads.
  Preserve this restriction for returned effectful callables too.
- Destination dispatch still requires a written expected type that determines
  the receiver implementation. An erased interface is not an arbitrary factory
  for one chosen hidden implementor, and ambiguity is not resolved by order.
- No runtime downcast/type test or implicit forwarding of applicability through
  a wrapper. Existing non-overlap, member completeness and default rules remain.

[argued] Thus `logs: DirWrite`, `{read: DirRead write: DirWrite}` and
`type LogDir of DirWrite` are valid under the new scope ruling. A new typed-let
annotation can be an explicit widening boundary; add it deliberately to the
old chapter's boundary list, since old untyped let does not widen. **Instead:**
retain both erased-interface and generic-bound forms and amend F4's earlier
generic-only description. Do not infer generic conformance from an erased value.

[argued] Erasure deliberately prevents recovering a WriteDir impl from a
DirRead interface view. I qualify my earlier concrete-type leakage objection
accordingly: a real erased view can restrict the accessible API. It still does
not by itself prove that the hidden read implementation cannot return a broad
grant, perform extra operations, or misapply the underlying token. **Instead:**
keep the sealed-grant requirement, checked operation contracts and no-downcast
rule; opaque API restriction and actual runtime grant attenuation are distinct.

[argued] The now-preferred wrapper skeleton is:

```text
type LogDir of DirWrite
  visibility @public
  // Constructor and unwrapping pattern are private exports.
  // Exported nested proc write has performs {DirWrite}.
endtype
```

[argued] Comments state required export policy, not implemented syntax. LogDir
does not automatically implement DirWrite: its defining module unwraps to the
interface value and dispatches through it in an exported nested operation.
For a Proc result containing Self, perform the existing supported result
substitution inside the cold Intent result; do not drop the latent row or
bypass the forbidden returned-callable cases. **Instead:** test this lowering
alongside the existing pure interface calls. Generic LogDir(W) is an optional
static alternative, no longer a requirement.

## 6. The runtime-configured log directory acceptance case

[argued] A public `LogDir` parameter plus `performs {DirWrite}` can honestly
say: **this procedure can use the application's certified log-directory writer
and perform directory-write operations; its concrete directory is supplied at
runtime**. It does not statically state an exact path discovered later, and
all LogDir values share the same nominal brand. Two
different runtime directory values do not acquire different static types.
**Instead:** define OUR LOG DIR as an issuer policy tied to a configuration
snapshot/resource identity, not a fictitious compile-time path literal.

[argued] A module capable of privately constructing LogDir can wrap the wrong
writer too. Visibility controls **who may certify**; it does not prove **what
they certified**. Current general wrapper visibility is not yet a complete
syntax for a public identity with a separately private constructor/pattern.
**Instead:** specify that abstract export boundary and protect the issuer's
body/policy/approved dependencies. Add executable scope checks and evidence
at the issuer, rather than claiming nominal typing proves its implementation.

[argued] The concrete bootstrap contract should be:

1. A protected startup provider reads app config using an already authorized
   config-read grant. This work has its own checked effects and typed failures.
2. It parses and validates the log-directory request. A Str path is data,
   never a grant and never enough to invoke an ambient privileged constructor.
3. It derives a write-only directory grant from held authority, with runtime
   confinement checks, or asks an explicitly authorized grant broker governed
   by approval policy. A broker is itself authority, not a loophole to mint it.
4. It records the configuration snapshot, resource/driver identity and scope
   decision, then certifies the resulting handle as LogDir.
5. It invokes the application worker requiring LogDir. If the required grant
   cannot be acquired, startup fails; an optional metrics acquisition instead
   follows the declared None fallback policy.

[argued] A manifest can pin that provider and build-check its types, required
inputs and policy. It cannot prove future runtime config content or OS approval
in advance. The main worker's signature is precise about the logical role and
operation; a runtime record/report supplies the actual folder. **Instead:**
retain that conditional guarantee rather than advertise static proof that a
future path exists, is writable or equals the intended directory.

[argued] The configured path must be within held authority. If config names
an unrelated directory, derivation fails unless a separate reviewed grant can
authorize it. Junctions and races cannot turn a scoped log handle into access
to the whole filesystem. **Instead:** enforce handle-relative confinement at
the trusted driver boundary, avoid exposing the broader parent, and reject
cross-target/world restoration that would rebind the certificate silently.

[argued] If the actual requirement is to distinguish two independently chosen
directories of the same role **in static types**, reusable nominal LogDir is
not enough. That is the reopening case for generative brands, scoped tokens
or resource indices. **Instead:** defer those mechanisms until that stronger
case is required; do not pretend a private wrapper already provides them.

## 7. Why not builtin Cap(operation-set) now

[argued] The builtin might reduce record boilerplate and express operation-set
subtyping directly. It still needs nominal operation identities, sealed grants,
runtime scopes, resource relationships, host enforcement, checked delegation,
and a rule for latent callable effects. It does not solve config provenance or
same-directory identity by being shorter. **Instead:** start with products of
sealed leaves and explicit wrappers; reopen builtin sets only when actual APIs
show unacceptable product/generic adapter costs or need checked subset coercion.

[argued] The records choice still has costs: more leaf handle types, explicit
projection/reconstruction, generic signatures and wrapper methods, and no
automatic guarantee that bundled handles share a resource. Count these honestly;
do not claim no new mechanism while adding grant issuance and latent typing.
**Instead:** compare complete acceptance-case programs if this boilerplate
becomes material, preserving the same confinement and static-audit guarantees.

## 8. Disposition and validation

[argued] Amend F1/F3 to checked closed ceilings plus value authority, F4 to the
sealed grant/operation distinction and chosen dispatch model, F2/B10/B11 to
startup-provider/config/target identity, F6 to cold latent rows, and C1 to the
effect-section grammar. Keep F5 model isolation, F8 uncertainty, D3's absence
of revocation/linearity, and D7 nominal identity limits explicit. The LogDir
certificate is not a general proof of arbitrary behavioral predicates.

[argued] Settling checks are: a pure factory returning a write plan; a write
plan rejected under a network-only callable type; a network plan rejected by
a write-only runner; an interface implementation laundering NetSend behind
DirWrite; duplicate imported root aliases; empty omitted rows; a record whose
read/write leaves target different folders; public-wrapper constructor forgery;
wrong-config and outside-root LogDir certification; optional metrics None;
and required acquisition failure. Add conformance cases for closures, records,
prebuilt intents and provider versions. These checks are proposed, not claimed
run against an implementation that does not yet exist.

## 9. What remains open on effects

[argued] Owner rulings settle positional rows, `performs`, retained interface
values, required/optional grants and the direction of attenuation. These
remaining contracts still need agreement and conformance before implementation:

1. **Operation roots and interface contracts.** Accept or amend the proposed
   implicit-owner/additive translation, precise standard operation granularity,
   and root identity for generic operation interfaces. My default is one
   nominal declaration-level family per operation interface, independent of
   runtime scope; do not infer root identity from the spelling of a type value.
2. **Abstract exports and issuance.** Spell public type/private constructor and
   pattern access, sealed native grant creation, and the protected issuer path.
   State exactly what a read-only leaf can return or delegate; a private parent
   pointer alone is not a complete authority policy.
3. **Resource-scope operations.** Define confinement and failure contracts for
   files, directories, selected endpoint sets and subscopes, plus shared-limit
   accounting. Combining scopes is a relation-preserving bundle operation, not
   an implicit Cartesian-product or privilege-union coercion.
4. **Startup and consent.** Specify required binding recipes, acquisition
   failure, optional refusal, config provenance, and the boundary for runtime
   requests that exceed preapproved grants. Decide which host/driver operations
   the audit policy actually classifies; compile-time rows do not replace it.
5. **Typed lowering and schema details.** Port interface-value result rules
   through cold Intent, preserve callback/record bounds, define complete
   callable types when labeled/variadic parameters return, and version the
   row/root/target encodings. Anonymous Proc is the effectful lambda carry-over.

[argued] Exact per-runtime-resource static identity, automatic proof of least
access, row polymorphism, subset-capability subtyping and revocation are **not**
implied by these defaults. Reopen them on a named workload, not as silent
requirements on the first gate. Nothing else in the owner's acceptance case
requires builtin Cap sets or dependent path types: its exact configured scope
is enforced and evidenced at the protected runtime grant boundary. **Instead:**
record these limits alongside the accepted static operation ceiling and nominal
role certificate, without calling the remaining boundary work mere naming.
