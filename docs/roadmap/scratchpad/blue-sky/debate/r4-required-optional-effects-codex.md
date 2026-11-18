# Required grants, optional grants, attenuation and effect ceilings

[argued] Follow-up: [concrete ceiling form and runtime-scoped grants](r4-effect-ceiling-form-codex.md)
incorporates later owner rulings: `performs` on declarations, positional
Proc/Intent rows, and retained interface values. It supplies the syntax,
interface-value carry-over rules and remaining open contracts for this review.

[argued] Accept the seven reported owner rulings. They settle the corresponding
open choices, not the remaining grammar and checker details identified in the
[carry-over review](r4-k3-carryover-review-codex.md). On the new requirement:
Claude's proposal supplies useful capability plumbing, but **does not alone
provide signature-based compile-time rejection of forbidden effect families**.
I recommend reopening F1/F3 for checked closed effect ceilings alongside value
capabilities. These mechanisms answer different questions.

[cited] Current [effects chapter](../../../../spec/03-effects.md), Static
contract and Calls and rows, already distinguishes an erased may-attempt
ceiling from runtime permission, and carries a closed row on an indirect
callable's type. [Round 2 R5](r2-codex.md#2-r5-authority-does-not-appear-from-nowhere)
accepted capability confinement without an index while explicitly preserving
the local callback-auditing problem. The owner's new wording activates that
recorded question; it is not a refutation of unforgeable capabilities.

## 1. What can be established, and when

[argued] Split "mandatory at compile time" into the following judgments:

| Requirement | Earliest justified check | What it establishes |
| --- | --- | --- |
| Entry requires a ReadDir argument rather than Option(ReadDir) | Source type checking | Calling that entry requires a value of the declared type; None is not such a value |
| The selected target binds every required entry argument | Closed-target build validation | The pinned target contains a well-typed provider/grant binding; missing or incompatible required bindings reject the build |
| The provider can acquire the configured required resource now | Startup/acquisition | If acquisition is refused or fails, startup fails explicitly before main; the program is not run with a fabricated dummy grant |
| The folder remains accessible and an operation succeeds later | Each operation | Typed outcome, including denial, disappearance, timeout or uncertainty; no compile-time availability guarantee |
| A local callable may perform only specified effect families | Compositional static effect checking | Its inferred execution effects fit the declared ceiling, including delegated work |
| An actual dispatch stays within approved directory/endpoint rights | Capability/driver enforcement | The concrete request is confined to the actual value grant, not merely its type name |

[argued] Therefore "the program does not start" is insufficient if the advertised
guarantee is a compile-time check of required bindings. **Instead:** reject a
closed target missing a required binding during build, and separately fail
startup when its resource cannot be acquired. Library/source checking can
succeed without a deployment manifest; it must not be reported as successful
validation of a particular deployment's grants.

[argued] Presence of a capability is not a proof that the app is useful or that
every call succeeds. An ordinary fake implementing an interface also does not
establish that a production resource binding exists. **Instead:** distinguish
the interface contract from sealed grant issuance and approved target providers;
pin provider identities and world profile in B10/B11/F2/F5. Real production
bindings and model fakes must not become interchangeable accidentally.

[argued] Refusal at startup and refusal after dispatch are different. An
unavailable mandatory root can prevent startup; an operation may still return
PermissionDenied later. "Unknown whether completed" cannot be collapsed into
"refused, did nothing." **Instead:** retain F8's delivery/outcome distinctions
and test both acquisition failure and failures after main has begun.

## 2. Optional authority: Option is suitable, with explicit policy

[argued] Accept `Option(Cap)` for an optional root. The target must declare how
that optional parameter is populated, including the permitted None case. Grant
acquisition refusal produces None through that policy; source code pattern
matches it and selects a fallback. **Instead:** describe required/optional in
the target-binding schema and validate compatibility with the entry signature,
not just leave a missing manifest field to an implicit default.

[argued] Option eliminates the missing-value path at the type level; it does
not prove that the remaining app behaves usefully. A program can match None
and immediately quit, or perform some different optional effect through a
captured callback. **Instead:** state the required fallback behavior in claims
and protected tests: denied metrics still produces the normal core result,
does not contact the endpoint and does not stall waiting for it. That is a
behavioral acceptance case, not a theorem implied by Option.

[argued] A held optional capability can later fail or be refused too. Keep
typed Result/outcome values from its operations. Some(cap) is neither a
permanent promise of reachability nor evidence that an uncertain send did not
happen. **Instead:** handle absence, known operation failure and unknown outcome
as distinct states; choose metrics' best-effort policy explicitly.

### Optional permission and a forbidden effect family are not identical

[argued] A binary prepared to send metrics when granted may legitimately have
network among its potential effects while receiving None for metrics on this
run. Refusing the grant prevents the real dispatch; it need not make that
binary ill-typed. Conversely, a compile-time rule "this target has no possible
network effects" is stronger than "metrics permission was refused today."
**Instead:** separate the source/target's allowed effect ceiling, the optional
grant request, and the actual grant decision for the run.

[argued] An Option match alone does not remove Net from a conservative static
effect union: the Some branch still contains it. To prove a special target
binding None has no reachable network use, a checker needs a justified
reachability/specialization analysis, or the target must use a no-network
implementation/profile. **Instead:** do not promise that old closed rows or
the Option spelling automatically prove branch-sensitive deployment erasure.
Unknown analysis must be rejected when that stronger no-network policy is
required, rather than silently treated as no effects.

## 3. The callback counterexample survives

[argued] Consider the following **typed sketch, not final K3 syntax**:

```text
makeLocalLogger : LocalLogCap -> Proc({Str} Void)
makeNetworkJob  : NetCap      -> Proc({Str} Void)
runJob          : Proc({Str} Void) -> Intent(Void)
```

[argued] Both returned Proc values have the same ordinary argument/result
type. One captures a local logging grant; the other captures network authority.
`runJob` awaits the supplied plan. Neither forges authority, and both factories
declare the capabilities from which their authority originates. The unindexed
Proc type does not distinguish which job runJob received. A record containing
the job or a prebuilt Intent has the same issue; naming more capability
parameters elsewhere does not restore the lost local distinction.

[argued] Under a local policy "may log locally, may not use network," a checker
using **only that type** must accept both or reject both. A whole-program/value-
aware analysis may distinguish them; a transitive report alone is not such an
enforcement rule. **Instead:** either reject an opaque broad/unknown job at
that policy boundary, or carry a checked latent-effect bound and enforce it.
Accepting an unknown job and relying on its displayed parameter names fails
the stated compile-time policy.

[argued] Rejecting every unknown is a sound conservative design, and F1/F3
already permit it. Thus capabilities are not incapable of participating in a
safe compiler. The cost is that a separately checked generic runner cannot
accept an opaque but approved local-only job from that ordinary Proc type.
**Instead:** make the precision/usability requirement explicit. For the owner's
request, I recommend supporting that case, rather than presenting rejection
of all delegated jobs as equivalent to existing effect-ceiling checking.

[argued] "May log" must also mean something precise. A logging interface can
be implemented by an HTTP logger. It is not evidence of no host network use.
**Instead:** define the policy over checked operation roots and approved driver
groundings, or explicitly call it an abstract interface policy. A no-host-network
policy must reject a network-backed logger regardless of its friendly name.

## 4. Recommended repair: closed bounds plus capabilities

[argued] Reuse the existing **closed effect-ceiling discipline** as the starting
point, adapted to cold plans. No effect-row polymorphism, dependent path types
or proof of future host success is required for this first repair. **Instead:**
separate three kinds of information in checking and tool output:

1. Immediate execution effects: constructing or storing a cold plan performs
   no I/O; pure Fn cannot execute one.
2. Latent execution effects: a Proc/Intent records a checked upper bound on
   effects executed when awaited. Await contributes that bound to its enclosing
   Proc's performed effects. Composition conservatively combines the bounds.
3. Runtime authority: sealed capability values carry actual rights, resources,
   delegation lineage and selected driver/world identity. An effect ceiling
   grants none of these rights by itself.

[argued] A local-only Proc then has a checked local-log bound; a network Proc
has a network bound. The runner's accepted callable contract and own ceiling
must admit the former and reject the latter. Bound declarations are checked
against implementations and captures/delegated invocations, not trusted
annotations; an opaque unbounded component remains unknown and is rejected
under a narrower required policy. **Instead:** preserve callable/Intent bounds
through closures, record fields, return values, interface contracts and foreign
bindings, without pretending pure plan construction is execution.

[argued] This changes the proposed Fn/Proc/Intent contracts and the public
interface checker. Existing rows provide a basis, not a ready implementation:
the old system executes ordinary effectful calls eagerly and does not express
these new cold-plan semantics. **Instead:** agree one syntax and one canonical
machine representation after the semantic rules are settled, and version them.
Do not add two alternative spellings of effects merely to preserve a diagram.

[argued] Choose the initial compatibility rule explicitly. Retaining current
closed-row equality is a coherent minimal starting point; ceiling inclusion
checking on bodies does not by itself introduce callable row subtyping. A
wrapper may be needed to adapt a smaller declared bound, as current Vibra
already specifies. **Instead:** do not silently add subtyping or row variables
while restoring the ceiling guarantee. Reopen them only on demonstrated
higher-order/library friction.

## 5. Attenuation: a weaker value, not merely a weaker-looking interface

[argued] Accept ReadDir/WriteDir interfaces as useful operation groups and
accept deriving a read-only handle from a held read/write directory handle.
Separate interfaces alone do not enforce attenuation: if `readOnly(dir)`
returns the same concrete type with a WriteDir impl, the recipient may still
call its write operation. If a record view exposes the parent as a public
field, the recipient can recover it. **Instead:** return a distinct sealed
read-only view with no available write implementation, parent projection,
downcast, reconstruction or broad-parent-returning method.

[argued] A sound generic body constrained only by ReadDir can prevent direct
use of WriteDir operations on its abstract parameter, but that constraint
does not transform a concrete handle into a universally read-only object.
**Instead:** distinguish a locally restricted generic interface view from
the actual attenuated grant handed to potentially different recipients.
Check both the nominal API and the underlying request authorization.

[argued] The protected child grant must satisfy **child authority is a subset
of parent authority** across rights, resource scope and other delegated limits;
never widen endpoints, path scope or target/world binding. Driver operations
must honor that child grant, not silently dispatch with an exposed broad parent.
Knowing an equivalent record shape or implementing ReadDir cannot mint it.
**Instead:** enforce derivation at sealed library/driver boundaries, retaining
F4's unforgeability and provenance checks.

[argued] Read-only derivation may be pure when it just restricts a sealed token;
`Dir.sub(dir path)` needs more specification. A lexical prefix restriction can
be constructed without host access only if each eventual dispatch enforces
the real confinement policy. If derivation resolves/opens the directory now,
it is an effectful operation that can fail. **Instead:** choose and document
one meaning, returning a typed failure where resolution is performed. String
concatenation or a check-then-use canonical-path test alone is not the security
argument: Windows junctions, parent traversal and races must not escape the
scoped root at dispatch.

[argued] Attenuation grants less authority to the recipient; it does not revoke
the original parent's write permission or consume every alias. D3 currently
has no affine/linear guarantee. Two recipients can also consume a shared quota
unless accounting says otherwise. **Instead:** retain the parent deliberately,
state limit-sharing semantics, and treat revocation/unique transfer as separate
protocols rather than imply them from readOnly.

## 6. Ledger disposition and settling cases

[argued] I propose the following amendments; Claude remains the ledger editor:

- **F1:** retain value capabilities for authority, reopen the unconditional
  "no effect index/ceiling" choice to provide the now-required static local
  effect policy. Checked closed ceilings grant no runtime rights.
- **F3:** replace merely informational broad/unknown reporting by a checked
  latent bound where a local policy requires it. Unknown remains valid data,
  but cannot satisfy a narrower required policy. Exact directory/endpoint
  grants remain value-level and are not inferred from the family bound.
- **F2 / B10 / B11:** closed target builds reject missing/incompatible required
  bindings, explicitly configure optional ones, and pin providers/resources/
  scopes/policy/world versions. Startup acquisition failure remains distinct.
- **F4:** specify sealed monotone derivation and read-only views, including
  attempts to recover a broad parent or fabricate a matching handle.
- **F6:** retain Proc-only execution and await, explicitly preserving cold-plan
  creation and checked latent effects in Proc/Intent types.
- **F8 / N1:** keep typed acquisition and operation outcomes, optional fallback
  behavior and CLI startup/exit policies, including late/unknown completion.

[argued] Settle the design with these acceptance cases before calling the
owner's requirement met:

| Case | Expected finding |
| --- | --- |
| Required read grant absent from a closed target | Build rejects, not merely a later null/missing-value failure |
| Required grant bound but OS denies acquisition | Explicit startup failure; successful compilation never claimed future availability |
| Metrics grant refused | Main receives None; protected behavioral test confirms normal core work and no metrics dispatch |
| Optional Some grant later fails or has unknown completion | Typed outcome handled by the declared best-effort policy |
| Local-only opaque callback versus network callback | Both may delegate valid grants; only the allowed checked latent bound is accepted by the local-only runner |
| Same jobs hidden in a record or prebuilt Intent | Bounds/provenance survive the wrapper; no field/closure laundering |
| HTTP-backed logger under no-host-network policy | Rejected despite its Log interface name |
| Read/write parent passed through readOnly | Recipient can read, write is statically unavailable, and forged/raw requests cannot bypass child rights |
| Read-only subdirectory with junction/traversal/race attempt | Driver refuses escape; unresolved acquisition failures are typed |
| Original parent retained after derivation | Parent may still write; no revocation/linearity claim |

[argued] These are proposed conformance and threat-boundary cases, not tests
already run against a capability implementation. A counterproposal that passes
them using sound whole-program/value-aware checking and conservative unknown
rejection could preserve the no-surface-row design. What cannot preserve the
stronger guarantee is adding only the signature plumbing and a report while
accepting opaque Proc values as if their effect families were known. **Instead:**
reopen F1/F3 now, keep the required/optional/attenuation proposal with the stated
conditions, and settle the exact static policy before rewriting the spec.
