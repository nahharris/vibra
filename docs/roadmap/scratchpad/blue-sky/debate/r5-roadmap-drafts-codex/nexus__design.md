# Nexus design

Status: implementation design against the active K3 specification, not evidence of implementation. This page defines internal responsibilities; observable rules belong to the specification and release profile.

## Pipeline and dependency direction

K3 source becomes lossless syntax, resolved forms and checked IR. The interpreter consumes checked IR directly. vibra-lower emits execution IR; vibra-exec verifies it; vibra-nexus-eval steps it and vibra-codegen emits a Cranelift object. vibra-build links that object with the target static runtime archive and launcher.

| Crate/service | Responsibility and dependency boundary |
| --- | --- |
| vibra-exec | Execution IR, deterministic display and bounded verifier; language/registry facts from vibra-ir. |
| vibra-lower | Immutable checked IR to execution IR; no workspace/filesystem/network/backend acquisition. |
| vibra-nexus | Context store, frames, transfers, dispatch, observation, host/grant boundary and admission; does not depend on execution IR/codegen. |
| vibra-nexus-eval | Reference execution-IR stepper over Nexus. Internal comparison instrument, not another product language. |
| vibra-codegen | Verified execution IR to target object; sole direct owner of Cranelift dependencies. |
| vibra-nexus-bridge | Versioned generated-code/runtime ABI and audited unsafe boundary. |
| vibra-build | Explicit target toolchain discovery, object/archive/link orchestration and provenance. |
| vibra-actors | Later shared kernel/lifecycle/mailbox/simulation behavior driven by interpreter and Nexus executors. |

One source/type/effect/diagnostic/workspace engine serves CLI/MCP/LSP. Backend or UI arrows never point into semantic phases. Package/import/build/initializer dependencies are acyclic. Symbolic nominal/function references are not completed-body prerequisites and retain finite-size/call-recursion checks.

## Checked IR and execution IR

Lower only whole admitted programs; unsupported forms return a named unsupported result before execution/emission. Checked IR retains written runtime operand order, nominal/type/callable identities, explicit coercions, complete latent rows and the contract proof/consequence classification. Identity/effect reports remain pinned even when execution erases proof/row metadata.

Execution IR is a flat function/block table with explicit frame slots, value ownership, branches, direct/indirect calls, resume points, module values and registered host operations. Stable code/type/origin identities are separate from process addresses. Generic instantiation initially uses runtime type descriptors, not speculative monomorphization.

Verifier obligations, updated together with each added instruction:

| Rule | Required check |
| --- | --- |
| V1 | IDs, types/classes, function/block/resume/entry metadata are present and bounded before allocating verifier state. |
| V2 | Every block terminates and branch/switch/resume targets are valid for its function; supplied metadata cannot jump into another function. |
| V3 | Direct signatures and indirect header arity/classes match; operation/registry descriptors are known and typed. |
| V4 | A slot is full at every read and empty at every owned write; merges do not manufacture ownership. Move consumes, copy retains, drop releases; borrowed operands do not consume. |
| V5 | Every path disposes/transfers owned values exactly according to its return/tail/call/host contract; aggregate/capture child ownership is explicit. |
| V6 | Origins/registry requirements/descriptor lengths and external symbols are admitted; no instruction mints a grant from ordinary source data or erases a checked latent-effect boundary. |

The verifier checks execution-IR invariants; it does not re-prove all source typing/contracts. Emission accepts a verified immutable wrapper. Bound frame slots, blocks, operand vectors and verification work explicitly. Borrowed operand lists may repeat a slot; do not equate argument-list length with the number of distinct frame slots, an inherited bridge/emitter gap that needs a consistent rule.

## Frames, checkpoints and quota

All language activations live in context-owned runtime memory, not one native stack frame each. A generated function is resumable by block/resume ID and returns to a dispatcher at a checkpoint. Non-tail Call pushes a frame; TailCall replaces it; Return delivers a result and releases the completed frame.

Sequential checkpoints are Call, TailCall, Return, Host and Global. Branches/jumps remain within the current step. A long primitive is atomic relative to cooperative yielding, then charges proportional work; quota debt yields at the next checkpoint. An exhausted scheduler quota suspends/resumes, never becomes a language fuel value or guard failure. Under equivalent deterministic worlds, quota one and unbounded preserve results/origins/ordered host observations. This is not a real-time latency guarantee or interruption of a blocked host action.

True tail position has no continuation: verified bare ensures leaves no runtime work, but explicit ensures-else, result widening or scope cleanup may leave work. Do not erase a consequence merely to keep recursion constant-space. Deep non-tail recursion consumes bounded context memory and ends in a host memory outcome on exhaustion, not native stack overflow.

Waiting/kernel actions extend the protocol in v0.0.4 with explicit Wait/completion state and an ABI change. A synchronous Call alone does not implement asynchronous external completion.

## Values and reclamation

Choose a context-local byte arena with 32-bit offset references, precise non-atomic reference counts and iterative worklist release. An initial context holds at most 4 GiB including charged frame/storage state. Layout is private to the runtime/representation module; generated code names slots through bridge routines. No raw arena pointer crosses the generated-code bridge.

Values are immutable and built from already existing children. Closures capture older immutable values/type arguments; module functions are symbolic code IDs, not counted self-owning closures. Module values are unread/evaluating/ready, initialized lazily once per run; initializer dependency cycles reject before executable checked IR. New grant/Intent capture graphs must preserve acyclicity. No cycle collector, arbitrary shared heap, automatic close protocol or constant-time release is claimed.

A grant value holds a sealed broker reference with world/generation, target, rights/scope and lineage. Broker resource/accounting lifetime outlives any retained child/capture; stale slot reuse cannot resurrect it. Internal registries/weak references avoid cyclic owning links. Context-local RC is not used as synchronization for shared broker counters.

Observation walks values by checked types in runtime code and produces the canonical value contract, not a generated encoding walker. Callables/Intents/live grants have no authority-restoring persistent encoding. Unsupported erased observation is an explicit encoder/tool/test failure, not a user-data trap.

## Static contracts and explicit consequences

Bare requires, ensures and where are static obligations checked before admission. Unproved/refuted/unsupported required obligations cannot silently become dynamic checks. A bare-where constructor returns T and requires proof at construction; a where-else constructor returns Result(T ErrorType), preserving that type even when a check is erased.

Lower requires-else to a pure preflight branch: false evaluates/returns its declared consequence and bypasses body and ensures; true enters the body with the condition established. Lower ensures-else by saving the body candidate, checking its predicate once, returning the candidate on true or its consequence evaluated with that candidate on false. Do not revalidate the fallback, rebind it as a new candidate or promise the final output satisfies the predicate. Where-else validates representation before T escapes and uses the author's constant error value on false.

Contract expressions are in the certified pure/fault-free fragment. Proof erasure preserves ordinary callee/argument evaluation, observable body behavior, declared types and other continuations. No guard-failed trap exists. Unknown proof/tool exhaustion, memory/host outcomes and toolchain defects are not converted into ordinary rejected-input values. Partial arithmetic uses the checked Option/Result surface; division/rem and unsupported observation do not add user-data traps.

## Cold Proc and Intent

Calling Proc captures evaluated arguments, immutable environment, type descriptors and execution identity without running a body, dynamic guard or host action. Bare requires is established at plan construction. Intent is a distinct checked type using a closure-like layout; it is not interchangeable with a pure Fn.

Each explicit await enters its captured execution entry as a fresh invocation; dynamic requires-else runs per await. Copying/dropping a plan neither executes nor consumes it. Invocation/submission/retry bookkeeping is execution/driver state, not a hidden mutable memoized promise. Plain callable conversion retains executable entry/exit clauses, complete rows and type restrictions.

@once forbids automatic redispatch after possible submission within one logical invocation. It does not suppress a second explicit await and does not supply crash deduplication or exactly-once external effects.

## Host calls and sealed grants

A Host transfer names a registered typed operation and takes the held sealed grant as an operand. Before action, Context::host validates world/provider/generation, operation rights, requested resource/scope and shared limits; it dispatches only through the bound reviewed adapter. Valid out-of-scope/refused requests have typed outcomes; invalid internal tokens are toolchain defects.

Authority data and policy descriptions are distinct: parsing a path, decoding a policy, nominal wrapping or interface erasure cannot mint authority. Restriction retains parent policy as intersection and shared accounting. Child rights/scope/limits cannot grow or reset counters. HostResource and ActorEndpoint share a tagged envelope, not interchangeable protocols: actor authorization is checked at kernel dispatch. ActorEndpoint remains a reserved representation kind until the v0.0.4 typed concurrency contract; it creates no current source constructor, interface or messaging API.

The filesystem adapter starts every invocation from the held anchor, without intermediate directory handles cached across invocations, uses checked object-relative resolution and never an unchecked canonicalize-prefix-open fallback. Actual-entry spelling/reparse/race/non-truncation requirements and hard-link/object-lifetime consent limits remain mandatory. Unsupported adapter actions are safe refusal, not a useful gate pass. HTTP selectors retain origin/peer/method/target correlation, including component prefix plus exact/any query. That policy family is reserved until a reviewed typed network integration supplies its operation, driver and conformance contract; schema data alone does not advertise a network capability.

A completed host call appends its typed result and safe grant projection after the action/resume transition. Yield before/after must not duplicate it. No completed event is invented for a pre-dispatch refusal or incomplete trap/host outcome; absence does not prove no effect. Any separate attempt/refusal event has its own versioned schema. Audit identities/redaction/correlation are data, never bearer authority.

## ABI, code generation and admission

Baseline generated code calls runtime routines for values/branches/transfers; it does not directly call another generated Vibra function or read arena layout. The bridge alone contains our unsafe code, with documented safety preconditions and no unwind crossing the ABI. This source boundary does not remove Cranelift/linker/OS/providers from the trusted base.

The descriptor carries declared block/frame counts, entry identity/result contract, functions/module initializers, registry-major/minor and exact symbols, driver/policy requirements and source-origin data. Runtime ABI, registry version, persistent schema and release version are independent. Reconcile the existing descriptor before extending it: old entry-err-shape flags and missing portable identity are not the new contract.

Admit compatibility and required drivers/bindings before source entry. A valid descriptor is not a sandbox for arbitrary native code from elsewhere. Review compiler and bridge assumptions independently; verifier/stepper/native agreement alone is not a compiler correctness proof.

## Link, launch and manifests

One target archive contains Nexus, bound host/native routines, bridge and launcher. Cranelift emits the program object; an explicit pinned platform linker joins it. Windows uses explicitly discovered MSVC/SDK, Linux a pinned compiler driver, macOS a pinned Apple SDK/driver. No ambient tool-name lookup selects an unknown linker.

System startup validates target recipes and acquires required/optional grants in stable dependency order, ties in manifest order, before entry. Failed required acquisition reports startup failure and cleans acquired resources in reverse; optional specified refusal maps to None. Recipe/type/version/cycle errors never become optional absence. Reviewed config acquisition and issuers contribute startup effects/consent; cleanup does not promise rollback of external work.

Entry executes the main plan and unwraps its declared ExitCode U8 result. Standalone application status and vibra run's tool exit/status mapping are different; never infer them from arbitrary enum names. Run/test/eval use the interpreter without a native linker.

The harness launcher has a versioned internal observation schema pinned with code/world/runner profiles. It can observe native results/errors/origins/audits under deterministic worlds without exposing a shipping ambient fake-world mode.

Portable input identity is target-independent; artifact identity includes target/CPU/runtime/archive/link/tool versions and flags. Build from closed source snapshots, no published-store dependency. Compare clean offline builds from different checkout paths before signing. No self-referential manifest/signature hash cycle.

## Local concurrency handoff

v0.0.4 specifies the public scoped-child/join and bounded ordered-map library over a shared actor kernel, and receive-loop actors with explicit lifetimes. Kernel state, typed endpoints/replies, mailboxes, timers and event sequence are one implementation driven by interpreter and Nexus.

One context per active unit has exclusive worker ownership. Messages copy into neutral kernel data and into the receiver's store; grants use broker delegation, never context offsets. Internal checked mapper code/capture transfer is distinct from general message serialization. Placement is fixed before initialization; no stealing or migration. Shared broker counters use actual synchronization, not local RC.

Simulation schedules record kernel decisions at observable actions and virtual-time boundaries, not equal reductions across executors. Invalid schedules explicitly fail. Multicore traces are not globally deterministic. Host and actor effect/authority reports preserve direct rows plus causal paths to reachable host actions. No deadlock, must-reply, crash-recovery or exactly-once guarantee follows from this substrate.
