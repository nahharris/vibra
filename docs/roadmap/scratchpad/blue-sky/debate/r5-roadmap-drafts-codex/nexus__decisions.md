# Nexus current decisions

Status: implementation decisions for the authorized K3 rewrite, not implementation or benchmark evidence. This is one current list; earlier unshipped alternatives are replaced, not retained as another authoritative plan. Specification and release gates outrank it.

| ID | Decision | Reason / boundary | Reopen on |
| --- | --- | --- | --- |
| D1 | Permanent interpreter; native compiler/runtime; Wasm removal at qualified cutover. | Owner product direction. The question is how/readiness, not whether to build native or keep Wasm indefinitely. | Owner product change. |
| D2 | Checked IR -> verified execution IR -> stepper and Cranelift object -> runtime archive/linker. | Isolates lowering from emission and reuses shared semantics. | A demonstrated boundary failure, with specification review. |
| D3 | Runtime-owned frames/resume dispatcher; language calls are not host-stack calls. | Deep non-tail calls use context memory; indirect tail replacement and later park need explicit suspended state. | Measured cost motivates a semantics-preserving optimization. |
| D4 | Five sequential checkpoints: Call, TailCall, Return, Host, Global. | One explicit transfer protocol. Wait/kernel actions extend it later with ABI/version change. | Required asynchronous behavior cannot be expressed. |
| D5 | Quota yields/resumes without source fuel semantics. Long primitives complete then charge. | No fixed reduction-count coupling across executors or real-time claim. | Scheduler/profile requirement needs a reviewed change. |
| D6 | Context arena, 32-bit offsets, precise non-atomic RC, worklist release; initial 4 GiB/context. | Acyclic immutable values and exclusive context ownership; no cycle collector/shared arena/constant-time claim. | Liveness leak, invalid cycle or required capacity. |
| D7 | Symbolic code/type anchors independent of body hashes. | Function/type recursion does not create completed-layout/hash prerequisites; actual import/build/initializer cycles reject. | Required identity migration, through explicit tooling. |
| D8 | Runtime operations behind bridge; layout private. | Keeps generated code from depending on slot/store pointer internals before measurement. | Measured equivalent specialization. |
| D9 | Cranelift baseline pin from existing scaffold, explicit architecture/CPU flags. | Reuse work, do not inherit host CPU features or upgrade dependencies accidentally. | Explicit reviewed target/toolchain update. |
| D10 | Unsafe source is confined to audited bridge. | Useful internal boundary; OS, dependencies, generator/linker and providers still trusted. | Unavoidable reviewed platform adaptation. |
| D11 | No user-data guard trap. Bare obligations discharge statically; else supplies typed consequences. | Requires-else preflight bypass, ensures-else candidate relation and where-else Result are source semantics. | Supported proof fragment insufficient for a needed task. |
| D12 | Intent uses closure-like layout but distinct type; reusable cold plan. | Creation captures only; every await is a fresh invocation and dynamic entry. Delivery state belongs to execution/driver. | Concrete lifetime/semantics counterexample. |
| D13 | Sealed grant envelope with disjoint HostResource/ActorEndpoint targets. | Share representation, not source protocol or dispatch meaning. Host checks at Context::host; actor checks at kernel. ActorEndpoint is reserved until the typed v0.0.4 contract. | A well-typed target-kind escape. |
| D14 | Broker-owned resources, liveness and shared accounting; restriction retains parent. | No minting from data, reset by cloning or amplification by interface erasure. Replaces the former no-runtime-grants approach. | Grant escape or counter-accounting race. |
| D15 | Typed policy data, correlated alternatives/intersection and versioned normalization. | File/HTTP selectors are closed data; HTTP selectors remain reserved until typed network integration. No arbitrary predicate DSL or subset solver. | Needed selector outside current contract or enforcement failure. |
| D16 | Every file invocation walks from held anchor; no intermediate handle cache between invocations. | Preserve selected object lifetime without stale descendant access. Reparse/name/non-truncation/race tests are required. | Platform cannot implement contract: unsupported, never unchecked fallback. |
| D17 | Startup recipes stable topological order, reverse cleanup. | Required binding errors build-time; acquisition may fail at startup; optional specified refusal None. Startup effects are reviewed. | Actual acquisition dependency/failure cannot be expressed. |
| D18 | Completed host events and safe data projections; no authority-restoring encoding. | Audit is not a complete external-action proof. Pre-dispatch refusal is not a completed call. | Need separately specified attempt/refusal/event schema. |
| D19 | Exact runtime ABI/registry/driver/policy/origin admission. | Static ceilings may erase during execution but protected reports and requirements remain. Arbitrary native code is not sandboxed. | A stale/mismatched artifact executes. |
| D20 | Runtime type descriptors first; no monomorphization baseline. | Match shared interpreter semantics and complete parity before optimization. | Measured workload needs specialization. |
| D21 | Actual three-platform execution; pending/not-run is a gap. | Cross-emission cannot prove launch, adapters or target semantics. | Explicit revised supported-platform decision. |
| D22 | Portable source-snapshot identity separate from target artifact identity. | No authoritative store/publication prerequisite; pinned drivers/policy and source maps visible. | Invalid cache reuse or hidden approved-input change. |
| D23 | Versioned internal harness observation/world contracts. | Internal producer/consumer compatibility is real even when not public API. | Schema mismatch or new observation requirement. |
| D24 | One backend-neutral actor kernel; scoped child/join and ordered bounded pure map first APIs. | Owner delegated this default. Receive-loop actors have explicit lifetimes; no mandatory mailbox design for all CLIs. | A real protocol needs a changed source/API contract. |
| D25 | Fixed pinned workers/exclusive contexts; messages copy neutral data. | No migration, stealing or implicit shared immutable heap in first concurrency release. | Measured copy/placement cost fails workload. |
| D26 | Actor endpoints and grants transfer only by typed broker/kernel delegation. | Arena offsets, bytes and persisted tokens are not authority. Mapper code/captures need an explicit separate transfer rule. | A concrete transfer escape or necessary unsupported capture. |
| D27 | Replay at kernel actions/virtual-time decisions, not quota counts. | Interpreter/native need equal observable decisions, not equal instructions; production multicore is not replay event-order equality. | Replay lacks future-relevant state. |
| D28 | Selected service journal profiles later, no universal journaling. | none/ring/durable have explicit crash/reconcile/redaction contracts; @once is not crash exactly-once. | Gate-3 workload needs stronger specified recovery. |

## Uncertainty and evidence

Architecture choices are current defaults, not measured winners. Windows scope adapter feasibility remains v0.0.2 M0 critical-path work. The static checker and revised native ABI/grant world are not established by an old-profile test. Requalify existing machinery; retain shipped v0.0.1 evidence but do not cite it as K3 gate support.

No source/API name left open by a later concurrency/service prerequisite is silently selected in implementation. A missing observable contract blocks that slice until specification/diagnostic/schema review. X-K3c uses the real v0.0.2 M1/M2 engine after freeze; no throwaway semantic evaluator is scheduled here.
