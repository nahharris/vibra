# Round 2 semantics: capabilities and proposed collapses

[argued] Scope: independent examination of Claude's R5, the five proposed construct collapses, and the merged checking rule in `r2-claude.md`. I read both Claude round files. These are paper examples, not compiler measurements. All code below is pseudocode for the proposed design, not supported Vibra syntax.

## R5: concede the literal challenge, distinguish the weaker claim

[argued] I cannot construct authority arriving through **no signature anywhere on its provenance path** while assuming all authority originates in explicit root parameters, all creation/delegation is traced, closure capture is accounted for, and capability values are unforgeable. Under those assumptions the requested example contradicts the premise: some root/factory has the authority. This is a concession to the literal challenge, not evidence that a particular implementation meets those assumptions.

[argued] The following example challenges a different statement: that the ordinary parameter **types of each local signature** reveal its concrete effect kinds without examining witnesses or provenance. Assume a write capability is required to construct an authorized write intent.

```text
make-writer : (WriteCap) -> ((Text) -> Intent(Unit))

make-writer(cap):
    send(text):
        return cap.write(text)
    return send

run : ((Text) -> Intent(Unit)) -> Proc(Unit)

run(job):
    await job("hello")

main(cap: WriteCap):
    await run(make-writer(cap))
```

[argued] `run` can receive a disk writer, console writer, or network sender with the same result shape. Its local signature identifies an effect-bearing function capability but does not identify which concrete service is reachable. The `WriteCap` is visible in `main` and `make-writer`; I am expressly **not** claiming that the factory hides its parameter from those signatures. Even a directly visible `WriteCap` type need not distinguish directory A from directory B if both are scoped instances of that type; exact entry authority then also requires the supplied roots and limits in a manifest.

[argued] A prebuilt `Intent(Unit)` is another delegated authority value. If `run` accepts it and awaits it, the runtime can soundly require the embedded unforgeable token. Authority does not appear from nowhere; concrete effect metadata is abstracted by the value's coarse type. Closures and authorized intents can themselves be object capabilities. Consequently this example does **not** show that effect rows are necessary for no-ambient-authority security.

[argued] Three properties should be specified separately: (1) authority confinement: no effect outside the authority actually delegated from roots; (2) a local static summary: a caller can determine concrete permitted operations/scopes from a callee's public contract; (3) policy enforcement: a target can refuse forbidden effects, whether by static checking, manifest/provenance analysis, or runtime token checks. A design may deliver (1) and (3) with capability handles and conservative higher-order abstraction while intentionally giving a coarse answer to (2).

[argued] **Verdict and alternative:** accept value capabilities as the CLI-first security mechanism; do not require effect rows merely to win this debate. Replace “parameter capability types are the effect summary” with a precise statement about directly held and transitively delegated authority. Decide whether higher-order functions/intents need operation-set type metadata only when a stated local policy needs it. **What would change my view:** a typed account of closure/intent provenance and target checking that accepts this writer, rejects an unauthorized constructor, and explains how `run`'s coarse contract is presented to a reviewer. If exact local effects are a requirement, test both authority-indexed function/intent types and value-aware manifest analysis on this example.

## Interface/port: reuse syntax, retain the runtime contract

[cited] The current draft interface is statically dispatched and has no interface values; it permits one implementation per interface/type pair. A port includes operations, outcomes, delivery semantics, external signals, and an associated simulation model. Those contracts differ in the draft. [Types: interfaces](../../../spec/03-types.md#interfaces), [Execution: ports](../../../spec/05-execution.md#ports).

[argued] Static interface dispatch is compatible with capability values. A generic procedure receiving `h: T` with `T: ReadFs` can invoke statically resolved operations on an unforgeable nominal `HostHandle` or `ModelHandle`; it does not need a dynamically dispatched interface value. Two scoped instances can share one concrete handle type and differ in protected value data. The original one-implementation-per-interface/type rule prevents using both host and model implementations for the **same** nominal handle type in one context; distinct concrete handle types plus a generic entry, or an explicitly controlled binding mechanism, resolve that. A “target chooses the implementation” rule alone still supplies neither unforgeability nor scope restriction. Structural similarity of an arbitrary record to a capability interface must not mint authority.

[argued] **Verdict and alternative:** one `interface` declaration syntax is reasonable, provided nominal handle construction and delegation enforce unforgeability and scopes. Use static generic dispatch first; runtime interface values are optional if heterogeneous handles are needed. A more explicit `Cap<I, Scope>` type is another choice, not a necessary new construct. Specify which handles may be serialized or restored into a pinned target; bytes naming a handle must not grant authority in another target. Keep delivery/outcome metadata attached to operations even if the word `port` disappears. **What would change my view:** two-directory attenuation tests, counterfeit-handle rejection, model substitution, and restoration tests under the unified interface notation. Keyword reduction alone settles none of these tests.

## Model/impl: an implementation shape is reusable; admissible behavior differs

[cited] The draft model returns a nonempty list of allowed `(next-state, outcome)` answers; simulation selects one and exploration branches over all. Its driver conformance contract is membership in that set. [Execution: models](../../../spec/05-execution.md#models).

[argued] A normal implementation that returns one answer and a model defining all allowed answers do different semantic work. Supplying `choose` can express this distinction with a shared implementation syntax, but `choose` must be controlled by the harness and expose all alternatives to exploration. A model that quietly receives real filesystem/network capabilities would also defeat a promise that tests use no real I/O. A recorded trace provides sampled behavior for recorded requests, not a universal allowed-outcome relation for new requests.

[argued] **Verdict and alternative:** allow models to use `impl` syntax, but retain an explicit model profile: permitted capabilities, state transitions, choice points, outcome coverage, and evidence scope. For the first CLI, a deterministic in-memory filesystem implementation is useful without claiming it models every host failure. **What would change my view:** one shared implementation mechanism that runs deterministic tests, enumerates choices under exploration, rejects real-I/O authority in model mode, and labels trace-only coverage honestly. This is a viable syntactic collapse with distinct mode checks, not proof that every existing model requirement vanishes.

## Machine/recursive proc: expressive encoding does not determine the protocol

[argued] A recursive procedure awaiting a bounded inbox can encode a receive loop. I do not claim an explicit `machine` keyword adds fundamental computability. However, “await one request to completion, then receive again” differs from an event reducer that can process a second message while the first external operation is pending. The recursive-procedure version can recover the reducer behavior by awaiting a choice over inbox/outstanding outcomes and storing explicit state; at that point the same protocol obligations have been encoded rather than removed.

```text
receive-loop(state, inbox):
    event = await choose-next(inbox, state.pending)
    (next-state, intents) = step(state, event)
    emit intents
    await receive-loop(next-state, inbox)
```

[argued] The minimal example to distinguish the protocols is a slow external write followed by a status query: must status reply before the write completes? Also specify mailbox capacity, repeated messages, correlation of outstanding outcomes, child ownership, cancellation, and durable state migration. A function precondition can express a state invariant at recursive entry, but does not itself specify atomic commit or persistence boundaries.

[argued] **Verdict and alternative:** accept `proc` only for the CLI gate and defer the spelling choice for services. Keep a reducer representation in runtime/analysis where needed; expose it as a library combinator if that remains smaller for authors. **What would change my view:** the same status-during-write protocol implemented both ways, with identical event traces, failure/cancellation outcomes and migration behavior. Count the required concepts and proof obligations, not just declarations.

## Lemma/fn: a common declaration can retain a separate proof phase

[cited] The draft lemma body is not executed and `apply` makes its established postconditions available to later reasoning. [Claims: lemmas](../../../spec/04-claims.md#lemmas-and-laws). Lean's validation guidance distinguishes proof validity from statement meaning and lists dependencies/axioms as part of the trust account. [Lean validation](https://lean-lang.org/doc/reference/latest/ValidatingProofs/).

[argued] A `fn ... returns Unit ensures P` can share syntax with a lemma, but an ordinary runtime function returning unit does not establish `P` by virtue of returning. An erased proof body and a runtime computation require different treatment. In particular, testing `P` on a few inputs does not license using `P` as a universal proved premise at an arbitrary call. Running a finite check for a concrete `P(x)` may establish that occurrence, not every quantified instance of `P`.

[argued] **Verdict and alternative:** accept `fn` as a possible unified declaration spelling, with explicit ghost/erasure phase and evidence rules; only checked proofs may introduce universal facts at `proved`. Sampled lemmas remain sampled evidence, and assumptions remain named assumptions inherited by conclusions. Erased computations must not determine runtime values or consume runtime capabilities. **What would change my view:** examples of a recursive inductive proof, a ghost-only helper, and a tested false-on-one-input statement, showing that only the first validly introduces its universal conclusion. If a separate proof mode costs more than keeping `lemma`, keep `lemma`.

## Label/opaque type: scope is a valid cut; equivalence is false

[argued] Opaque representation controls which code can inspect a value; it does not automatically track the information returned after inspection. No FFI or generic serializer is needed for this distinction.

```text
module secrets:
    opaque Secret = Bool
    public bit(value: Secret) -> Bool:
        return its-private-bool-field

proc report(secret: Secret, log: LogCap):
    if secrets.bit(secret):
        await log.write("1")
    else:
        await log.write("0")
```

[argued] A representation-opaque type permits this API and ordinary Boolean use. A flow-label system can keep the returned Boolean secret and propagate the branch label to the log sink unless the bit disclosure is explicitly authorized. The example is an intentional declassification if the API designer wants it; that is precisely the policy choice that opacity alone does not distinguish. Withholding every observer can prevent useful consumers too; allowing selected observers requires reviewing their information-release contracts.

[argued] **Verdict and alternative:** defer labels for a CLI product with no promised information-flow property. State explicitly that opaque types provide abstraction and controlled observation, not general noninterference. Do not retain the original security guarantee while deleting its checking mechanism. **What would change my view:** a named secrecy policy and a type/module discipline that rejects this disclosure while accepting an authorized release and useful nonrevealing operations. If no such policy belongs to the product, cutting labels is a scope decision that needs no equivalence claim.

## The merged “every other obligation keeps its runtime check” rule

[argued] I accept the repair for executable safety predicates: bounds, divisor nonzero and fixed-width overflow can be dynamically checked when no adequate proof justifies erasure. A failed check should name its obligation. This does not extend automatically to every clause in chapter 04, which permits quantified formulas over arbitrary inputs and claims about termination or asymptotic cost.

```text
ensures forall n: Int:
    not halts-within(machine-description, n)
```

[argued] Here `halts-within` can be a total bounded simulation for each individual `n`; checking every `n` by execution has no finite stopping rule when the claim is true. A finite range changes the property. A timeout returns `unknown` or a budget fault, not a universal answer. Likewise a runtime budget can stop this execution but cannot prove that all executions meet an asymptotic bound; a deadline can detect exceeding its deadline but cannot establish mathematical termination of every input.

[argued] **Verdict and alternative:** replace the merged rule with three explicit classes: (1) executable guards, retained dynamically until proved; (2) general propositions, usable through proof, bounded/sample evidence, or a recorded assumption according to policy, without inventing a universal runtime check; (3) operational budgets, enforced by metering independently of logical contracts. An unproved general proposition cannot erase a safety guard merely because a test policy admitted it. A closed no-fault profile must require appropriate universal evidence and resource assumptions. **What would change my view:** a specified executable-contract fragment and a compiler test that rejects or marks `unknown` for this quantifier while compiling finite bounds checks, plus a policy test showing sampled assertions cannot become proved premises.

[argued] Overall position: several collapses are good candidates for shared syntax. They do not justify deleting the security, proof-phase, model-mode and lifecycle judgments that their original constructs carried. Conversely those judgments do not force separate keywords or effect rows when capability provenance and runtime checks already meet the stated policy.
