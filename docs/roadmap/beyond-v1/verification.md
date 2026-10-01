# Verification

Status: design direction; not normative
Lines: V0 in [1.x](README.md#1x--foundations), V1 in
[Vibra 2](README.md#vibra-2--concurrent-services), V2 in
[Vibra 3](README.md#vibra-3--distributed-and-proven), V3 on the
[horizon](README.md#horizon)
Architecture: [Assurance layers and priorities](assurance.md)

## Why Vibra should verify programs

V1 makes an agent's changes checkable for types and effects. Contracts let the
agent state a property of those changes and ask for evidence. Evidence may be
a tested execution, a proof under stated assumptions, a refuting witness, or
an unresolved obligation. Failure to prove a claim does not make it false.

Immutable values avoid user-visible mutable-heap reasoning. Nominal data and
exhaustive branches give finite case splits. Static effects identify code
with no host operations. These are useful foundations, but purity alone does
not imply termination or freedom from traps, and mandatory tail calls do not
prove that recursion terminates. Recursive contracts still need inductive
invariants over parameters and well-founded measures.

## Design choice: auto-active verification first

The primary direction is contracts in ordinary pure Vibra expressions, with
explicit lemmas and measures where automation needs help. An SMT-backed
checker operates after ordinary type/effect checking. It does not participate
in type inference or change which unannotated programs are well typed.
General dependent types and arbitrary structural refinements stay deferred.

The motivating pattern is contract-based verification with explicit termination
measures, as documented by [Dafny](https://dafny.org/dafny/DafnyRef/DafnyRef).
This is design inspiration, not a promise to adopt its language or theories.
Interactive proof systems can handle obligations beyond the automatic subset;
they do not guarantee either automatic proof or a counterexample when stuck.

The verification stages below are independent of delivery of the network and
process features that share their release lines. Illustrative labels such as
`requires:`, `ensures:`, and `decreases:` are candidates only. Their grammar,
identities, status atoms, diagnostics, and machine schemas need a specification
proposal before implementation.

## Stages

V0 through V3 name verification stages, not source-language versions.

### V0 — executable contracts and properties

Line: 1.x. No solver. This stage supplies execution evidence, not proofs.

Contracts attach to functions, nested methods, and interface contract members.
A precondition is a pure boolean expression over parameters; a postcondition
also binds the result. Interface clauses apply to implementations. In tests,
every executed contracted call checks entry and exit clauses, including calls
inside libraries. A violation identifies the clause, source location, call
site, and representable argument/result data through structured test output.
Non-data values are reported by typed descriptions, never fabricated VIBON.

Contract evaluation has a specified deterministic step budget. Divergence,
traps, and exhausted evaluation report distinct unsuccessful test outcomes;
none counts as a passed check. Purity forbids host reads, but does not replace
this evaluation rule. The proposal also defines clause evaluation order and
prevents recursive re-entry into instrumentation from checking itself forever.

V0 clauses are erased in ordinary run/build modes and may not justify removal
of runtime checks or compiler optimizations. Adding one can change test
outcomes but does not constrain an existing production caller. A later
verification-required mode is an explicit opt-in, preserving that behavior.

Property tests add generated binders to a test. Nominal types provide explicit
generators and shrinkers, with standard generators for builtin data. Replay
records the source/dependency fingerprint, generator version, seed, limits,
and minimized input. A seed alone is insufficient after generator or program
changes. Shrinking reports the best witness found under its strategy and
budget, not a globally minimal input.

An explicitly selected property domain may filter generated inputs using its
precondition. Discards are counted and capped; an unsatisfiable or exhausted
domain is reported as insufficient coverage. A precondition violation at a
nested call is always a failure, never a discarded sample. Shrinking preserves
the selected domain and checks that the witness still fails.

Demo and exit gate: a parser or collection property exposes a seeded bug,
shrinks and replays it in both backends, distinguishes an empty input domain
from success, and reports a nested precondition violation without hiding it.

### V1 — static contracts and termination

Line: Vibra 2. Adds a pinned solver and a versioned semantic model.

**First supported slice.** Prove pure functions over booleans, fixed-width
integers, nominal products/sums, exhaustive branches, and finite structural
recursion. Explicitly reject unsupported proof obligations. Extend to
sequences, maps, higher-order calls, and effectful contracts only through
separate semantic-model and conformance gates; these are not prerequisites
for the first useful verified library.

**Totality and defined evaluation.** A claimed verified function terminates
and satisfies its postcondition for inputs satisfying its precondition,
without an unmodeled trap. These are separate obligations. Contract predicates,
measures, and proof helpers must themselves be defined and terminating over
the domains where they are evaluated. Otherwise a divergent predicate could
be treated as a logical fact without justification.

Structural descent is checked over each recursive group. Other recursion needs
a decreasing measure in a specified well-founded order, including every edge
of mutual recursion. Unsigned subtraction cannot stand in for mathematical
descent without checking its actual overflow behavior. Ordinary service loops
remain legal; their steps can be verified without claiming the whole service
terminates. A host operation needs an explicit return/progress assumption
before a proof of its caller can claim totality. Fuel exhaustion is containment,
not a termination proof.

**Obligation generation.** Callers establish preconditions and use callee
postconditions only with visible proof or assumption dependencies. Recursive
summaries require induction justified by the recursive group's termination
proof; mutually assuming summaries is not proof. Abstract interpretation may
discharge simple range obligations before SMT, but uses the same evidence and
unknown-result rules.

The logical model follows the active runtime semantics. Mathematical integers
need range constraints and the exact success/error branches of fixed-width
operations. Sequence models preserve Unicode scalar indexing and byte
indexing separately. Maps model missing keys. Floats require the specified
NaN, zero, and rounding behavior or remain unsupported; treating them as reals
is not sound. No proof silently assumes a checked `result` operation succeeds.

For an interface implementation, the member's allowed input domain is not
narrowed, and its promised result relation is preserved. Higher-order calls
need contracts for the callable argument or remain outside the supported
slice. Effectful summaries eventually quantify over permitted host responses;
registry postconditions are named assumptions unless independently checked.

**Nominal invariant types.** An opt-in new nominal type may carry a pure,
total invariant. It is a limited refinement, not general dependent typing.
All introduction paths need either a static proof or a checked constructor
returning a typed failure. This includes decoding, host responses, conversions,
and generic construction. An ordinary unchecked newtype constructor cannot
remain a bypass, even when verification is disabled. Unwrapping preserves the
fact because values are immutable. Retroactively constraining an existing
public type changes its contract and requires a compatibility decision.

**Executable and logical specifications.** V0 expressions retain their runtime
test interpretation. Unbounded quantifiers and other specification-only
functions belong to an explicit erased proof mode. Tests report such clauses
as not executable; they cannot silently skip them and report a contract pass.
Executable bodies cannot call proof-only code or depend on an erased value.
Logic erasure cannot change executable branch selection or output.

**Policy and outcomes.** Proof status and test coverage are separate fields.
The following names describe required distinctions, not final schema atoms:

| Proof outcome | Meaning | Admitted by strict required verification? |
| --- | --- | --- |
| Not requested | No proof attempt for this claim | No |
| Verified | All required obligations discharged under recorded base assumptions | Yes, if evidence matches the build and policy |
| Refuted | A checked witness violates the claim in the modeled semantics | No |
| Unknown | Solver cannot decide, or a candidate model cannot be validated | No |
| Resource exhausted | Deterministic proof budget ran out | No |
| Unsupported | Claim uses a feature outside the verified subset | No |
| Assumed | An explicit reason replaces proof; dependents retain that assumption | No |
| Tool error | Solver, checker, or evidence processing failed | No |

A candidate solver countermodel is labelled as such until validated. Where an
input is executable, replay it against the semantic oracle; logical witnesses
need their own checking rule. No counterexample is fabricated for unknown,
unsupported, or exhausted outcomes.

Strict required verification selects explicit declarations/claims and closes
over their proof dependencies. Merely omitting contracts from a callee does
not make its behavior proved: its body needs supported analysis, a verified
summary, or a visible assumption. Public input preconditions remain stated
conditions; a claim about a whole executable also needs checked entry inputs.
A failure blocks that target's executable artifact. Ordinary builds keep their
specified behavior and report their actual coverage.

The strict policy rejects user assumptions throughout the proof closure. A
future policy allowing named assumptions is separate and records conditional
claims. Both policies disclose unavoidable toolchain and environment trust;
"strict" does not mean a verified compiler or infallible hardware.

**Reproducibility.** Pin the semantic model, obligation generator, solver
binary/configuration, libraries, and deterministic resource limits. Use a
stable single-threaded solver configuration initially. Cross-platform clean
runs must reproduce outcomes before that configuration is supported; pinning
a solver version alone is not a determinism guarantee. Infrastructure timeouts
are unsuccessful interruptions and never proof results.

Cache keys cover exact obligations, all imported summaries and assumptions,
source/dependency fingerprints, model/checker/solver versions, options, and
limits. Cached data is not independent evidence from an untrusted producer.
An invalidated entry is recomputed, and clean builds reproduce results.

**Agent-facing surface.** Queries expose the property, source span, facts in
scope, proof dependencies, outcome/reason, assumptions, and any witness. Repair
plans use ordinary revision checks. Changing a precondition, postcondition,
invariant, or assumption changes the specification; it is not a safe autofix.

Demo and exit gate: prove a finite parser or collection transformation and
its termination, refute a deliberate mutation, reject a non-decreasing
recursive group, and fail a required build for unknown, exhausted, unsupported,
and transitively assumed claims. Repeated clean runs agree, stale cache
entries are rejected, and executable witnesses replay in both backends.

### V2 — lemmas and interface laws

Line: Vibra 3. Depends on V1, not on distribution.

Lemmas are pure, erased, terminating proof declarations. Recursive lemma calls
support induction only with a checked measure. The specification must define
lemma identities, scope of revealed facts, and the erasure boundary before
introducing declaration forms. A lemma contributes no host effect or runtime
value.

Interface laws build on the laws documented and exercised in M3. Proof
obligations are opt-in for a verified implementation or required consumer;
ordinary v1 implementations are not retroactively rejected for lacking a
proof. A required consumer cannot silently accept a merely tested law. Any
added law or changed meaning gets a compatibility review.

Associativity, ordering, equality, and hashing laws support collection proofs.
They do not authorize changing observable evaluation order without separate
reasoning about traps, divergence, and budgets. A law used by parallel
reduction must apply to the actual operation and type, including arithmetic
edge cases.

**Dependency evidence.** A versioned `.vibon` manifest binds each claim to its
canonical declaration identity, source/dependency hashes, contract and IR/model
fingerprints, verifier configuration, outcome, and complete assumption closure.
Build metadata binds those claims to the artifact and compilation configuration;
locks record immutable evidence references under an explicit format version.
CLI/MCP queries project the same information as JSON.

A producer's manifest is provenance, not proof-carrying code. A consumer
recomputes obligations with its supported pinned checker, or later validates
an independently checkable certificate. Signing a success flag does not make
the claim true. Changed dependencies, assumptions, or semantics invalidate
old evidence. No untrusted package code runs to inspect or recheck it.

Demo and exit gate: prove an ordering or collection law, consume that claim
from a pinned dependency, reject tampered and stale evidence, expose transitive
assumptions, and show an ordinary implementation still works outside required
verification. No package-wide "verified" label hides partial coverage.

### V3 — horizon

- **External proof checking.** Export hard obligations to a proof assistant
  through a versioned model. A Lean bridge would rely on kernel checking,
  described in the [Lean reference](https://lean-lang.org/doc/reference/latest/Introduction/).
  The translation into that model remains trusted until separately validated;
  allowed axioms, proof identity, and source binding are checked on import.
- **Protocol verification.** Session types and bounded temporal model checking
  are candidates beyond nominal state APIs and simulation. Report state-space
  bounds, fairness, delivery assumptions, and the model-to-code relationship.
  See [protocol and recovery evidence](concurrency.md#protocol-and-recovery-evidence).
- **Translation validation.** Check selected IR transformations or lowering
  instances before claiming compilation preserves proved properties. This is
  distinct from verifying the entire compiler; see
  [execution targets](execution-targets.md#translation-validation-horizon).
- **Proof-carrying artifacts.** Admit a package through a small independent
  checker only when a certificate can be bound to its policy and executable
  semantics. V2 manifests alone do not meet this gate.

## Trusted computing base

Every stage reports the parts it trusts: the frontend and type/effect checker,
semantic model and obligation generator, solver or independent proof checker,
imported axioms and assumptions, host contracts, compiler, and runtime. An
external theorem about a translated model does not automatically prove the
translation faithful. A source-level proof does not prove the Wasm engine,
operating system, network, or hardware correct.

The proof producer can be complex or untrusted only to the extent that its
output is independently checked. Until solver certificates are checked,
solver correctness remains part of the trusted base. Compiler/runtime
correctness remains an assumption until a precisely scoped validation or
proof closes that part of the boundary. Evidence is always relative to the
property actually stated; tests and review still challenge the specification.
