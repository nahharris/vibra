# Assurance architecture

Status: design direction; not normative
Applies after: the v1 release gate; no implementation is authorized here

## Product direction

Vibra should let an agent propose code, contracts, and proof hints freely,
then accept claims through explicit, reproducible checks. The generator is
outside the trusted computing base. Acceptance still depends on a human's
choice of requirements: a proof of the wrong contract is not correctness.

The unit of assurance is a named claim about an exact program under stated
assumptions. There is no global "fail-proof" badge. A tested example, a proof
for all inputs in a supported subset, a runtime guard, and a restart policy
answer different questions and remain distinct in queries and build evidence.

This is a cross-track plan for the ideas discussed in the verification
conversation. It refines the existing release lines rather than scheduling
all of formal methods as language features.

## Layers and boundaries

| Layer | Question answered | Direction and owner | Limit of the claim |
| --- | --- | --- | --- |
| Nominal types and static effects | Does this program have valid shapes and stay within its declared operations? | Finish v1 first | An effect ceiling is not path-scoped authority, termination, or functional correctness |
| Contracts and refinements | Does an implementation satisfy its stated input/output relation? | [Verification V0 and V1](verification.md#stages); nominal invariant types before general refinements | Tests cover executions; proofs cover a specified semantics and assumptions |
| Resources and authority | Who may use this handle, and for how long? | [Foundations](foundations.md#resources-and-handles); process ownership first, scoped capabilities separately | Runtime ownership checks do not statically prevent duplication or use after close |
| Typestate and protocols | Is an operation legal in this state? | Nominal state APIs first; affine and session-type research only when those are insufficient | A copyable wrapper alone cannot enforce exclusive state transitions |
| Totality | Does this verified computation finish? | Structural recursion and explicit measures in V1 | Tail calls bound stack growth; fuel bounds work; neither proves successful termination |
| Concurrency and recovery | What happens when components fail or messages arrive differently? | [Simulation, supervision, and recovery](concurrency.md#determinism-and-simulation-testing) | Replay and bounded exploration do not prove arbitrary safety or liveness |
| Translation and execution | Does the emitted program preserve the proved source behavior? | [Translation validation](execution-targets.md#translation-validation-horizon) before a whole verified compiler | Differential tests are evidence, not a semantic-preservation proof |
| Portable evidence | Can a consumer check the producer's claim? | V2 evidence manifests; V3 independently checked certificates | A manifest or solver-success flag is not proof-carrying code |

The initial architecture is a verified pure core inside an explicitly effectful
shell. Parsing, domain decisions, and state transitions are the first proof
targets. Host I/O, scheduling, failure detection, and persistence keep named
assumptions and runtime checks. Hoare-style pre/postconditions fit immutable
state passed through recursive functions; separation logic becomes relevant
only if a later model exposes resources whose ownership needs such reasoning.
It is not a prerequisite for proving ordinary immutable values.

## Delivery priorities

Release-line numbers are planning targets, not a dependency from proofs to
networking. V1 verification can ship without a process runtime; V2 lemmas can
ship without clusters. They share a line because of product priority. The
actual verification dependency is V0 -> V1 -> V2, with V3 research unscheduled.
Every slice below begins with a specification proposal and its own demo and
exit evidence before implementation starts.

| Priority | Slice | Dependency | Evidence required to promote it |
| --- | --- | --- | --- |
| First, 1.x / V0 | Executable contracts, property generation, shrinking, replay | Released v1 and a bounded test evaluator | A seeded parser bug produces a replayable witness in both backends; invalid nested calls cannot be filtered away |
| Next, Vibra 2 / V1 | Pure contract verification and totality for a deliberately small subset | V0, specified IR semantics and obligation generation | A parser or collection transformation proves its stated contract; a mutation is refuted; unsupported and exhausted obligations block a required build |
| Alongside V1 | Process-owned handles, runtime guards, budgets, supervision | Effect-row and host-ABI changes | Closed or transferred handles fail predictably; process failure releases resources and a replay reproduces recovery |
| Then, Vibra 3 / V2 | Lemmas, opt-in law obligations, dependency evidence | V1 and stable claim identities | A consumer rechecks a dependency claim, rejects stale evidence, and sees all assumptions |
| Research | Scoped authority, affine handles, protocol models, translation validation, external proof checking | A measured gap in the shipped slices | A bounded prototype demonstrates a stronger named guarantee at acceptable cost |

The first proof subset should cover booleans, fixed-width integer operations,
nominal products and sums, exhaustive branches, and structural recursion over
finite values. Collection theories and effectful contracts expand only after
their semantic models and boundary cases are specified. Excluded operations
produce an unsupported obligation, never an axiom silently invented by the
verifier. Syntax, status atoms, target policy fields, and certificate formats
are decided by the owning specification change, not by this plan.

## Research disposition

- **General dependent types:** defer. Start with contracts and nominal
  invariant types, keeping ordinary type checking independent of proof search.
  Reopen only for useful relationships that this subset cannot express at a
  reasonable proof cost.
- **General linearity or borrowing:** defer. Prototype affine one-shot handles
  before changing the value model. Record closure capture, container storage,
  transfer, and cleanup rules before claiming exclusive consumption.
- **Abstract interpretation:** a candidate for automatically discharging range
  and presence obligations. Its abstraction must over-approximate every
  modeled execution. Imprecision yields unknown, never success.
- **Symbolic execution and CEGAR:** optional ways to find witnesses or refine
  an abstraction behind the same obligation interface. Path bounds and
  unvalidated abstract counterexamples remain visible; they are not proofs of
  unrestricted executions.
- **Model checking and temporal logic:** start with an explicit bounded
  protocol model. State safety and liveness separately, including scheduling
  fairness, delivery, and failure assumptions, and justify the relation between
  the model and executable code. Session types remain a separate candidate;
  typed messages alone do not establish protocol order or progress.
- **Interactive proofs and correct-by-construction transformations:** admit
  external proof search only through a checked model and evidence boundary.
  A Lean bridge is one candidate, not a commitment to replace Vibra's type
  system. Validate each proposed transformation before trusting its output.
- **Category theory:** use explicit algebraic laws where composition needs
  them; it is explanatory machinery, not an additional assurance tier.
- **Consensus and durable transactions:** library/runtime work justified by
  service requirements. Specify failure models and storage guarantees before
  claiming fault tolerance; supervision alone supplies neither.

## Common acceptance and evidence rules

A verification-required target has a stronger, explicit admission policy.
Only that policy blocks its executable artifact on unresolved obligations;
ordinary programs retain their specified execution behavior. A required build
cannot silently fall back to tests, runtime checks, a different solver, or an
assumption when proof fails. An assumption allowance, if introduced, is a
separately named policy with a visible allowlist and conditional claims.

Every claim records its property, coverage, source and dependency fingerprints,
semantic-model revision, tool/checker configuration, resource limits, and
assumptions. A proof of a function's return relation is distinct from absence
of traps, totality, host authorization, protocol liveness, and compiler
correctness. Queries identify which of these were requested and established.
Evidence belongs in versioned `.vibon` data; CLI/MCP projections use JSON.

The agent repair loop is query -> propose source or proof changes -> check ->
inspect evidence -> revision-checked apply. An agent may suggest a stronger
precondition, a weaker postcondition, or an added assumption, but these change
the requirement or trust boundary. They are never labelled safe fixes or
silently used to turn a failed claim green.

Promotion reviews measure proof coverage, false alarms, unresolved outcomes,
annotation burden, check cost, reproducibility, and agent repair success on a
fixed corpus. The comparison is against v1 checks plus V0 tests, not against an
unconstrained generator alone. Each report includes rejected and inconclusive
cases, the declared trust boundary, and interpreter/Wasm parity where executed.

No new v1 gate is added by this plan. The existing interface-law, semantic-IR,
audit, versioning, and compatibility obligations remain the preparation work.
