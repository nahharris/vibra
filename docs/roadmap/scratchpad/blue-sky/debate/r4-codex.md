# Round 4: Codex's review of the draft ledger

Status: original 97-row and revised 105-row merges confirmed. The owner has
since selected K3; [r4-k3-owner-decision-codex.md](r4-k3-owner-decision-codex.md)
accepts that provisional choice and records evidence-scope/chronology edits
for the 106-row update. Earlier confirmations and reviews remain as history.

[measured] Reviewed `ledger.md` SHA-256
`18ad3df8b10d00adab7fbdfdbaf934ea4542b7487cbe895da0782a1a2064f8f3`.
This snapshot has **86 decision rows**, all unique, rather than 88. Every row
has a disposition below. I checked the experimental counts against the saved
240 writing trials and forty reader sessions; those checks pass. Both the
public-artifact and harness-source freezes still match. No participant reruns,
new experiments or changes to the frozen grader are part of this review.

[argued] I accept the CLI-first architecture, file authority, separate claim
and witness identities, capability values, retained executable guards,
structured concurrency, deferred services/UI/proofs, and provisional K.
**C4 is correctly a default**, E9 is an acceptable direction subject to the
zero-result correction below, and G5 correctly avoids adopting actors as the
ordinary CLI model. G2, H5, I1 and L8 should remain open. I do not accept the
current wording of F3/F8, an incomplete trusted-base inventory, or the omitted
accepted requirements as a closed consensus. **Instead:** merge the specific
replacements and carry unresolved items visibly, without restarting X1.

## Row-by-row disposition

[argued] `Accept` means agreement with this design decision or scope cut, not a
claim that Vibra already implements it. `Keep open` preserves uncertainty.
`Amend M#` points to the concrete replacement below; the existing text is not
my agreed position on that point. The support for this table is the accepted
[round-2 response](r2-codex.md), [round-3 report](r3-codex.md), and
[cross-review](r3-codex-cross-review.md), checked against the ledger snapshot.
Measured result rows retain their measured provenance; accepting a product
choice remains a judgment.

| Row | Codex disposition |
| --- | --- |
| A1 | Accept the owner constraint. |
| A2 | Accept the owner priority order. |
| A3 | Accept workload-dependent recording profiles. |
| A4 | Accept population-specific, dated author hypotheses. |
| A5 | Accept short, complete cards and measured size; no fixed token ceiling. |
| A6 | Accept human comprehension as a separate endpoint. |
| B1 | Accept files as source and the store as a cache. |
| B2 | Accept separate hashes; restore executable identity and nominal identity through M8. |
| B3 | Accept the protected approval path and exact checked snapshot. |
| B4 | Accept removal of the store-overlay command family. |
| B5 | Accept five declaration kinds and target data; fix the machine cross-reference under M10. |
| B6 | Accept one ordinary definition and scoped evidence for optional realizations. |
| B7 | Accept removal of duplicate-meaning/node-budget admission bans. |
| B8 | Accept code pinning and versioned durable frames; gate 1 also pins its bundle under M8/M9. |
| B9 | Accept repository policy in place of the five language roles. |
| C1 | Accept K as the provisional package choice, with human readability still open. |
| C2 | Accept the reported counts and explicit limits on inference. |
| C3 | Accept omitting D0/P from the prototype while retaining controls/data. |
| C4 | Accept as default: these parts were not independently varied. X5 tests local annotations, not every `x: int` or call/example spelling. |
| C5 | Accept named operations as default and X2 as deferred. |
| C6 | Accept positional ordinary calls, labeled constructor fields, and deferred X3; keep security labels separate. |
| C7 | Accept named callbacks as default and X4 as deferred; preserve the measured friction. |
| C8 | Accept annotated lets as default and X5 as deferred. |
| C9 | Accept recursion/transforms and restricted termination checking as defaults; this is not measured superiority over loops. |
| C10 | Accept repair as deferred, with its own feedback/denominator freeze; see M7. |
| C11 | Accept notation/table/protocol deferral. |
| C12 | Accept provider counts and the rounded means; no token tiebreaker or cross-tokenizer inference. |
| D1 | Accept nominal wrapped/record/variant types and constructor conditions; restore the identity seed under M8. |
| D2 | Accept dependent-type deferral without ruling out useful indices. |
| D3 | Amend M1: do not choose a memory-management mechanism while I1 is open. |
| D4 | Accept retained guards and sound elimination; align the trigger with dominating checks under M10. |
| D5 | Accept security-label deferral and the explicit loss of information-flow guarantees. |
| D6 | Accept a shared sequence interface where useful, preserving complexity and uncertainty distinctions. |
| E1 | Clarify guard clauses versus executable examples under M2. |
| E2 | Amend M2: no general solver is not the absence of static guarantees, and unrun tests are not tested evidence. |
| E3 | Accept the three-way distinction between guards, propositions and runtime budgets. |
| E4 | Accept coverage/method/trust/dependency/version/domain records; apply the identities in M8. |
| E5 | Accept conditional-proof handling; state the declared trusted base explicitly under M2. |
| E6 | Accept proof-phase/erasure requirements and gate-1 proof/lemma deferral. |
| E7 | Accept execution quotas separately from descriptive cost claims; no wall-time equivalence inferred from abstract fuel. |
| E8 | Accept relational contracts and optional reference realizations. |
| E9 | Amend M3: zero permitted results must not be reported as one determined result. |
| E10 | Accept scoped mutation/example findings with unknowns. |
| F1 | Accept capability values without mandatory effect indices, subject to sound provenance and conservative policy. |
| F2 | Accept explicit roots plus reviewed resource/scoping/driver data. |
| F3 | Amend M4: unknown latent authority is an audit approximation, not a grant of ambient authority or a claim about every pure callback. |
| F4 | Accept shared interface/impl spelling, static dispatch and unforgeable nominal handles. |
| F5 | Accept isolated model mode and inspectable harness-controlled choices. |
| F6 | Accept proc-only gates 1/2; fix the reopening reference under M10. |
| F7 | Accept early standard-capability fakes and a real-driver boundary corpus. |
| F8 | Amend M5: delivery/outcome metadata matters before the durability gate. |
| G1 | Accept recording profiles and explicit recovery/evidence limits. |
| G2 | Keep the hashing mechanism open at gate 3; full future-relevant state remains required under M9. |
| G3 | Accept machine deferral without an unsupported equivalence claim. |
| G4 | Clarify one local effect-order sequencer versus pure worker execution under M6. |
| G5 | Accept structured scopes as ordinary and actor/machine lifecycle as a later question; this rules out neither an internal actor implementation nor a future explicit machine form. |
| H1 | Clarify local task lifetime versus completion of external side effects under M5. |
| H2 | Accept bounded ordered parallel map and fallback, without automatic speedup or memory promises. |
| H3 | Accept explicit laws/domain/order or a fixed tree; include seed placement and empty-input behavior when specified. |
| H4 | Accept cancellation requests and uncertain outcomes. |
| H5 | Keep host/engine/thread performance open; do not infer a backend result. |
| I1 | Keep the mechanism open; safety/quotas/no constant-latency promise are agreed requirements, not open questions. |
| I2 | Accept scoped sampled conformance plus the real-driver corpus. |
| I3 | Accept early fakes and deferred simulation/exploration; restore the complete configuration requirement under M9. |
| I4 | Accept metering and explicit value-size limits; distinguish the experimental harness from a finished runtime. |
| J1 | Amend M6: the faithful frontend, execution and authority-enforcement components belong in the trusted-base inventory. |
| J2 | Accept finite agreement as differential testing. |
| J3 | Accept workload-limited analyses and explicit unsupported results. |
| J4 | Accept bundling the reference runtime before adding a backend. |
| K1 | Accept UI deferral and the later native-stack candidate, with semantic, visual and accessibility testing. |
| L1 | Accept the nine CLI verbs; restore the end-to-end release acceptance under M9. |
| L2 | Accept protected-claim comparison and exact build snapshot. |
| L3 | Amend M6: a tool need not construct every witness that exists. |
| L4 | Accept formatting parsed code and only unambiguous mechanical fixes. |
| L5 | Accept bounded reproducible evaluation/testing. |
| L6 | Accept bounded, versioned retrieval distinguishing checked examples from observations. |
| L7 | Accept generated author materials and deferred MCP/move adapters. |
| L8 | Keep provider-specific constrained-decoding effectiveness open; no decoder dependency or unverified token advantage enters the first gate. |
| X1-1 | Accept the assignment matrix and matched prompts, with the pre-run budget amendment recorded in X1-3. |
| X1-2 | Accept the T10 defect, retained primary failures and labeled post-hoc exclusion. |
| X1-3 | Amend M7 to preserve aggregate-token accounting and missing-count policy. |
| X1-4 | Accept the entry-declaration validation rule; label the prescription argued from an observed defect under M10. |
| X6-1 | Accept all model-reader totals and the description of wrong forbidden-result answers. |
| X6-2 | Accept no established reliable surface effect/equivalence, without denying the descriptive spread. |
| X6-3 | Keep human comprehension open; separate performance data from preference under M7. |
| X-arms | Amend M7: D0/K are fixed for X2–X5; the repair feedback design still needs its own freeze. |

## Required replacements

### M1 — D3 must agree with the open memory mechanism

[argued] Sharing and copy-on-write do not select or establish a safe memory
manager. D3 currently commits to a mechanism that I1 explicitly leaves open.
**Replace its mechanism sentence with:** no affine/linear `own` or `resource`
surface at this gate; immutable values may be shared, while a safe quota-aware
runtime mechanism is chosen at I1. No static uniqueness, must-consume or
must-reply guarantee is claimed. Copy-on-write is an implementation candidate,
not a settled safety argument. **Settling evidence:** retained graphs, deep
destruction and resource-lifetime cases under the chosen runtime. **Instead:**
keep the scope cut and guarantees explicit while leaving I1 genuinely open.

### M2 — E1/E2/E5: guard, example, test evidence and trusted-base precision

[argued] For E1, describe `requires`/`ensures` as executable Boolean guards and
`example` as a concrete call/expected-result assertion checked by `check` or
`test`. Do not imply that each function invocation recursively runs its example
suite. The surface example remains `example: call = value` in K. **Instead:**
state the distinct checking moments and preserve their common executable core.

[argued] For E2, say: no general solver-driven behavioral proof facility in
gate 1; examples/tests produce sampled evidence only after execution, otherwise
the statement is unverified or unsupported as appropriate. Static typing,
exhaustiveness and supported structural termination checks remain separate
static judgments. For E5, unconditional `proved` is relative to the explicitly
declared trusted base and has no unresolved additional assumption debt.
**Settling cases:** a checked type error, an unrun test, a successful finite
test suite and a proof with an unproved premise receive distinct reports.
**Instead:** keep these distinctions in the evidence schema and narrow the
closing list's claim about giving up "universal correctness beyond tests."

### M3 — E9: uniqueness requires existence as well as at-most-one

[argued] E9 is an agreed tooling direction, not a gate-1 feature requirement.
Its finite-domain branch needs one more distinction. `ensures false` over a
Boolean result admits zero outputs; exhausting both values cannot truthfully
answer "determined" or "the only allowed result." In a supported fragment,
distinguish a disallowed call, no permitted result for an otherwise admitted
input, exactly one permitted result, at least two, and unresolved/unsupported.
Uniqueness needs a witness plus complete exclusion of alternatives, or an
equivalent existence-and-uniqueness proof. Exhaustion means the full finite
output domain, not a finite prefix of integers. **Settling cases:** zero, one
and two allowed Boolean results, plus an unbounded unresolved integer case.
**Instead:** report the input, supported fragment, domain/bounds, validated
witnesses and evidence status; keep unknown when those conditions are unmet.

### M4 — F3: conservative auditing must not manufacture authority

[argued] Replace F3 with: an effect-producing callable or delegated intent whose
type hides its resource family receives a conservative unknown/broad *latent
authority summary* for local policy checking. This does not confer additional
capabilities; actual operations remain confined to authority delegated through
its captured values/handles and the reviewed roots. A soundly pure callback
is not automatically assigned network/filesystem effects simply because it is
a function value. Exact resource-family auditing without reading the witness
is the precision being deferred. **Settling case:** the round-2 `make-writer`
example under two attenuated roots, an ordinary pure mapper, and counterfeit
handles. **Instead:** keep value-capability security and explicitly name the
loss of local summary precision; reopen indices/value-aware analysis only for
a concrete policy that conservative checking cannot satisfy.

### M5 — F8/H1: uncertainty exists before durable services

[argued] F8 must apply delivery/idempotency/outcome metadata whenever an
effectful operation is introduced, including gate-1 file/console operations.
Only durable deduplication/reconciliation storage is conditional on the durable
profile at gate 3. A CLI can be cancelled after a write was dispatched; its
short lifetime does not make "not observed" equal to "did not happen."
**Instead:** retain metadata and honest uncertain outcomes now, and defer only
the durable machinery, with crash/retry/cancellation cases as the trigger.

[argued] In H1, scope exit joins or terminates the owned *local child task*
under a stated cleanup contract. It does not imply that a dispatched external
operation stopped or that its side effects rolled back. Those results obey H4
and F8. Preserve bounded fan-out and explicit handling of late replies or
uncertain outcomes. **Settling case:** a child is cancelled after dispatch and
a late success arrives after the local waiter has ended. **Instead:** specify
both local lifetime and external outcome instead of claiming one implies the
other.

### M6 — G4/J1/L3: execution and checker boundaries

[argued] G4 should name one local **effect-order sequencer** and require queue,
timer and causal rules before the relevant runtime gate. This does not mean
all pure computation uses one worker or that independent map evaluations must
be serial. **Settling case:** bounded pure parallel work with ordered effect
observations and responsive timers. **Instead:** separate logical effect order
from physical scheduling while keeping distributed time deferred.

[argued] J1 omits components that round 2 explicitly accepted as trusted:
faithful parsing, resolution and elaboration; the reference evaluator/lowering
and primitive semantics; capability confinement and resource enforcement; and
the host/driver boundary. Add obligation translation and certificate checking
only when introduced, plus persistence/recovery trust at its later gate. A
second checker remains deferred. **Settling cases:** wrong same-typed name
resolution, a mistranslated obligation, a forged handle, and a bypassed fuel
counter. **Instead:** publish logical, execution, security and later durability
inventories rather than equating the logical checker with the whole trusted
base. The X1 harness tests do not independently verify a Vibra frontend/runtime.

[argued] L3 promises "a confirmed witness when one exists." That is stronger
than the agreed "when available" and incompatible with honest unknowns for
unsupported searches. Return expected/found when meaningful, a validated
counterexample **when found**, otherwise the appropriate diagnostic or unknown
with reason and scope. A static type error need not have a runtime witness.
**Settling case:** an unsupported quantified obligation or exhausted search
does not invent a counterexample or imply its absence. **Instead:** expose
limits rather than require a complete witness finder.

### M7 — X1-3/X6-3/X-arms: preserve the actual experimental contracts

[argued] X1-3 should retain the agreed rule: use provider-reported aggregate
output including reasoning **once**; at most 4,096 is eligible; a missing,
invalid or conflicting count fails budget eligibility rather than becoming
zero. Keep raw functional success separate, and state the generation-cap
asymmetry. **Instead:** carry the frozen `budget.py` policy into the ledger,
not only the observed fact that six already-failed trials exceeded the limit.

[argued] For X6-3, Hannah's answers, omissions, timing and confidence where
available are exploratory data from one human reader. Her ranking/comments
are preference evidence, distinct from correctness and the owner's product
scope instructions. Different items per surface confound a general surface
comparison. **Instead:** keep the human endpoint open, record those outcomes
separately, and do not let a taste ranking silently replace the registered
writing rule or become a measured reading-performance result.

[argued] X-arms should say X2–X5 retain their registered D0/K matrix. The repair
arm remains deferred pending a separate freeze of its feedback, context,
budget, assignment denominator and handling of defective T10; do not describe
its full protocol or its surface controls as already frozen. **Instead:** keep
all unrun arms unresolved and preserve their explicit reopening triggers.

### M8 — Restore accepted identity requirements

[argued] These were already accepted in round 2; the ledger loses them rather
than settling them differently. Add rows or expand B2/B3/D1:

| Proposed row | Decision and status | Reopening/acceptance case |
| --- | --- | --- |
| B10 | [argued] Agreed: executable/build/evidence identity pins resolved transitive witnesses and target/tool/driver/policy bindings, not the claim hash alone. A witness-only edit may preserve claim approval while invalidating execution evidence. | Change a dependency body without changing its claim, and reject stale execution evidence. |
| B11 | [argued] Agreed: version persistent semantic/schema contracts as they are introduced; manifests/cache identities include interpretation versions. Unknown versions are explicitly unsupported and migration is deliberate. Journal/frame/ABI versions enter with their gates. | Inspect an old artifact after an incompatible normalization, arithmetic, ABI or schema change. |
| D7 | [argued] Agreed: nominal identity includes a stable nominal seed/domain identity, not structural content alone; same-shaped `cents` and `meters` remain distinct. The concrete seed/rename/migration policy must be fixed before identity-sensitive implementation. | Structurally identical domain types cannot be interchanged, and declared identity survives only the intended edits. |

[argued] **Instead:** restore these rows with their original scope; do not turn
the hash split into a claim that evidence survives arbitrary witness changes
or that content hashes alone establish nominal identity and reproducibility.

### M9 — Restore accepted core, delivery and later-gate exclusions

[argued] The following requirements have explicit antecedents in the round-2
concessions/response. They may be compact new rows or expansions of the indicated
rows. N3 and L9 are proposed directions/extensions of that discussion and need
Claude's explicit acceptance; they must not be backdated as prior concessions.
None requires implementing deferred services in gate 1.

| Anchor / proposed row | Decision and status | Acceptance or reopening case |
| --- | --- | --- |
| J5 | [argued] Agreed: use a typed higher-order core, or explicitly checked closure conversion accounting for captured bindings, passed/returned functions and indirect calls. Calling the current surface first-order does not remove these obligations. | Lower a captured, passed and returned callable with the correct type and capability provenance. |
| N1 / L1,J4 | [argued] Agreed gate-1 acceptance: create, check, repair, test, package and run a real file-validation/transform CLI on clean Windows; cover arguments, stdin, console/scoped files, typed failures, exit status, pinned dependencies and a distributable reference-runtime bundle. | The end-to-end task fails or needs undeclared host state. A pure syntax pilot does not pass this release gate. |
| N2 | [argued] Agreed method: versioned protected/held-out evaluation, fixed resource/defect criteria and explicit mechanism stop/redesign rules. Existing languages remain baselines, not alternatives to building Vibra. | Before a new mechanism/gate claims improved defect or authoring outcomes. |
| N3 | [argued] Proposed direction, pending acceptance: narrow typed, versioned host/dependency integration with explicit authority/assurance boundaries; first local/offline packaging, no immediate registry or effortless-ecosystem promise. Count conversion/model/upgrade work. | An indispensable real dependency is integrated and upgraded at the library gate. |
| L9 | [argued] Proposed tooling-contract extension, pending acceptance: bounded machine-readable diagnostics/results carry schema/tool versions, exact checked revision/content identity and precise passed/refuted/unknown/unsupported status as applicable. Retrieval alone is not the whole machine contract. | A clean agent repair/run workflow must not rely on unstable prose or an unpinned checked tree. |
| I3 expansion / I5 | [argued] Deferred exploration requirement: state identity covers the full future-relevant configuration, including pending operations, queues, timers, model state, children and code epochs. Report domains, depth, fairness/environment assumptions and exhaustion. | Equal user state with different outstanding outcomes must retain different reachable failures. |
| G6 | [argued] Deferred durable-journal requirement: classify payload/metadata, key ownership, retention, redaction/compaction/deletion, and the replay guarantees deliberately lost by each operation. | A secret-bearing trace is exported/redacted/deleted without an inflated replay/confidentiality claim. |
| G7 | [argued] Deferred replay requirement: simulation/counterfactual execution has isolated credentials/mode provenance and cannot silently reuse production operation identities or dispatch real effects. | An imported trace or what-if branch attempts to route an old operation to a production driver. |

[argued] **Instead:** keep these accepted boundaries visible, with gate-3
items deferred rather than absent. Do not reopen a first-order core or omit
package/version/retention safeguards merely because the syntax pilot did not
exercise them.

### M10 — Editorial/status corrections that matter for reading the ledger

[argued] Replace the header's "provenance tag of the strongest support" with
"provenance of the relevant evidence or reasoning; tags are not a ranking."
An owner instruction is a scope input, not a measurement. Default choices may
be informed by a measured pilot while their adoption remains a judgment.
**Instead:** apply amended rule 2 literally and retain per-claim qualifications.

[argued] Fix B5 and F6's machine reopening reference from G4 to **G3**. D4's
trigger should allow a validated proof **or sound dominating check**, matching
its text. X1-4's future-task rule is **argued from** the measured T10 defect,
not a universally measured requirement. L8 has no verified empirical support;
mark the no-assumed-decoder decision argued and the provider mechanism open.
**Instead:** correct the links/tags without inventing another experiment.

[argued] Add a global convention: `None`/`Never` in a row's reopening field
means no scheduled feature trigger, not immunity to a relevant counterexample,
an inconsistency with another row, or an explicit owner scope change. Accepted
logical distinctions need not be routinely re-litigated, but implementations
and claimed policies remain testable. **Instead:** make this convention once,
then keep the individual triggers concise.

## Guarantees and remaining dissent

[argued] Keep the closing list, but amend item 2 to include must-reply; item 3
to mean no general universal *behavioral-contract proof facility* at gate 1,
preserving type/exhaustiveness/supported termination judgments; item 5 to mean
exact resource-family auditing for opaque effect-producing callables/intents;
and item 6 to defer actor-specific lifecycle guarantees without erasing scoped
task guarantees. Add the explicit loss of global "accepted pure code cannot
fault" at this gate: retained primitive/refinement/contract guards can fault,
and fuel/quotas can stop execution. **Instead:** distinguish these scope cuts
from retained capability confinement, safe runtime behavior and static type
judgments, all of which remain design obligations rather than results proved
by the X1 harness.

[argued] Pending these merges, my dissent is limited to the identified wording
and omissions; it is not an alternative architecture proposal. The provisional
syntax outcome and deferred-arm decision stand. Human claim readability stays
open until Hannah's actual performance/comments are recorded with their
limits. **Instead of calling the entire ledger final now:** merge the accepted
row amendments, preserve any remaining disagreement by row id, and close the
human-dependent rows only on that evidence.

## Confirmation of the merged ledger

[measured] I read the revised `ledger.md`, SHA-256
`22e2019e1221fb4386de60582a86bfe3282b9d6bb7c57ea6a3a1ae501b563542`.
It contains 97 unique, well-formed decision rows: the original 86 plus B10,
B11, D7, G6, G7, I5, J5, L9, N1, N2 and N3. I checked the rewritten rows,
the new rows, the support/reopening conventions, and the eight-item list of
scope cuts against M1–M10. The experiment figures and their limitations remain
as reviewed; no additional experiment is implied by this merge.

| Amendment | Merge confirmation |
| --- | --- |
| M1 | [argued] D3 leaves the memory mechanism to I1 and preserves the explicit ownership/protocol exclusions. Accepted. |
| M2 | [argued] E1/E2/E5 separate runtime guards, examples, evidence after execution, static judgments and the declared trusted base. Accepted. |
| M3 | [argued] E9 distinguishes disallowed input, zero/one/two-or-more results, and unknown, with full-domain/existence requirements and no gate-1 decision-procedure promise. Accepted. |
| M4 | [argued] F3 makes the authority summary conservative without granting capabilities or assigning effects to every pure callback. Accepted. |
| M5 | [argued] F8 retains metadata from gate 1, while H1 separates local task cleanup from external effect completion. Accepted. |
| M6 | [argued] G4, J1 and L3 preserve effect order versus scheduling, the wider trusted base, and counterexamples when found rather than whenever one exists. Accepted. |
| M7 | [argued] X1-3, X6-3 and X-arms retain exact token accounting, raw success, separate human correctness/preference, the D0/K ablation matrix and a separately frozen repair protocol. Accepted. |
| M8 | [argued] B10/B11/D7 restore executable dependency identity, interpretation versions and nominal seeds. D7's concrete identity policy remains open. Accepted. |
| M9 | [argued] J5/N1/N2 and the later-gate I5/G6/G7 requirements are restored. N3 and L9 explicitly record proposal and acceptance in round 4, without backdating them. Accepted. |
| M10 | [argued] Support is provenance rather than rank; owner instructions are scope inputs; G3 references, D4/X1-4/L8 and the global reopening convention are corrected. Accepted. |

[argued] **No row-specific Codex dissent remains in this merged snapshot.**
This confirms the design ledger, not an implemented language, proof of its
runtime, or a normative specification rewrite. K stays the provisional writing
package choice; the deferred arms remain unrun. X6-3 is still open for Hannah's
human review. G2, H5, I1, L8 and D7's concrete identity policy also remain open
as recorded, rather than being silently settled by this confirmation.
