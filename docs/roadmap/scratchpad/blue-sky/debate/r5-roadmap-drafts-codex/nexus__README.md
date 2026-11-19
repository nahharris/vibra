# Nexus implementation plan

Status: planned delivery design for the rewritten K3 specification. It governs how the native/runtime track is decomposed; [the specification](../../spec/00-charter.md) and [release roadmaps](../README.md) outrank this implementation plan. No rewritten-profile gate is complete.

Nexus is the native executable runtime. The permanent interpreter remains the semantic oracle and run/test/eval/eligible compile-time evaluator. Checked IR lowers to verified execution IR; a reference stepper and Cranelift generator share Nexus; the object links with a target runtime archive and launcher. Wasm leaves at qualified native cutover, not as an optional permanent backend.

| Document | Purpose |
| --- | --- |
| [design.md](design.md) | Pipeline, IR/verifier, frames/store, guards/plans/grants, host/admission, launch and concurrency handoff. |
| [decisions.md](decisions.md) | One current implementation decision list, uncertainties and reopening triggers. |
| [goals.md](goals.md) | Delivery goals and milestone mapping. |
| [plan-v0.0.3.md](plan-v0.0.3.md) | Complete native profile, actual-platform parity, build and cutover packages. |
| [plan-v0.0.4.md](plan-v0.0.4.md) | Library/adapters, scoped/actor kernel simulation and native workers. |
| [validation.md](validation.md) | Standing invariants, commands, evidence interpretation and gate map. |

## Starting machinery

vibra-exec, vibra-nexus and vibra-nexus-eval provide IR/runtime/stepper foundations; vibra-lower, vibra-codegen, vibra-nexus-bridge and vibra-build contain partial old-profile lowering/emission/linking machinery. Some Windows fixtures previously ran. This is source-level starting context, not a pass of the new guards/grants/entry contract, not Linux/macOS qualification and not a shipping CLI build claim. Reconcile design/as-built descriptors in v0.0.2 M4 and requalify through v0.0.3.

## Dependencies

v0.0.2 M0 freezes the source/checker/runtime contracts and Windows scope-adapter feasibility. M1/M2 implement language and authority; X-K3c then runs on that real engine after its own final freeze, not a disposable evaluator. M3 provides portable snapshot/claim/witness/policy identities; M4 joins the revised native slice. Native low-level work may proceed after approved interfaces exist, but a complete vertical slice must join interpreter/lowering/stepper/emitter evidence before support is claimed.

v0.0.3 M1–M3 finishes and qualifies native execution and removes Wasm. v0.0.4 M1 library/adapter work depends on workspace semantics, not native emission; M2 simulation can proceed with its approved contracts; M3 native workers depend on qualified native runtime. v0.0.5 service/recovery plans are separate and add no implicit durability to a host call.

## Work-package handoff

Give an implementer the active milestone, exact source/spec anchors, prerequisite code and tested commit, the package validation rows and [execution.md](../execution.md). Require a focused positive/negative/recovery matrix, diagnostics/schema changes, host tests and independent Vibra cases. “Not run”, “unsupported” and pending are gaps. A design contradiction is corrected in the specification before code, not papered over by a snapshot or ignored case.

No authoritative database/store overlay, old-source compatibility parser, stage/publish workflow or second adapter checker is introduced by this track. Rewritten future plans replace their earlier versions in place; shipped v0.0.1 evidence is retained.
