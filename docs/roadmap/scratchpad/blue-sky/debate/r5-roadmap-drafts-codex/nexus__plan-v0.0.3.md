# v0.0.3 native work packages

Status: planned after the approved v0.0.2 contract/slice. Package IDs match the release milestone steps; no old completion status transfers. Read [design](design.md), [decisions](decisions.md), [validation](validation.md) and the active milestone before code.

## Dependencies

| Package | Depends on | Goal | Primary entry points |
| --- | --- | --- | --- |
| N3.1.1 verifier/admission | v0.0.2 M0/M4 contract and slice | G3.1 | vibra-exec verifier/display; bridge descriptor/admission |
| N3.1.2 primitives | N3.1.1 | G3.1/G3.2 | lowerer, Nexus scalar/native operations, codegen |
| N3.1.3 construction/patterns | N3.1.2 | G3.1/G3.3 | aggregate/variant/where lowering, ownership, pattern blocks |
| N3.1.4 callables/generics/interfaces | N3.1.3 | G3.1/G3.3 | descriptors, callable entries/captures, resolve/dispatch |
| N3.1.5 control/guards/modules | N3.1.4 | G3.2/G3.3 | return/tail/resume, guard branches, lazy globals |
| N3.1.6 collections/observation | N3.1.5 | G3.1/G3.4 | native/Iter/default calls, runtime observation |
| N3.1.7 plans/grants/startup/host | N3.1.5, v0.0.2 M2 | G3.4 | Intent capture, Context::host, broker, launcher |
| N3.1.8 M1 join | N3.1.5–N3.1.7 | G3.2/G3.6 | full fixture/harness, boundary audit |
| N3.2.1–N3.2.6 qualification | M1 join | G3.5/G3.6 | per-target launcher/link tests, parity inventory, memory/driver baseline |
| N3.3.1–N3.3.4 build/distribution | M2 exit, v0.0.2 identity inputs | G3.7/G3.8 | build/CLI/schema/manifests, toolchain pinning/install |
| N3.3.5 cutover | all preceding targets and reproducibility qualified | G3.9 | spec/profile/workspace/dependency/packaging removal |
| N3.3.6 audit | cutover | G3.9 | both-suite/release evidence |

Independent low-level target qualification may overlap after its input contracts are fixed. Changes that affect source semantics remain complete vertical slices, not backend-only support claims.

## Per-package implementation contract

For each admitted form extend the instruction/type descriptor, verification rule and rejection cases, Nexus runtime routine, lowerer, stepper, emitter and observations together. Record exact specification anchors, diagnostic/schema deltas and focused host/Vibra cases in the step guide. A form outside the supported slice names itself and rejects the whole program.

N3.1.1 must reconcile current descriptor metadata and inherited repeated-borrowed-operand limits rather than copy old assumptions. N3.1.3 validates all invariant ingress; bare where constructs T only with discharged proof, where-else constructs Result with unchanged type and constant author error. N3.1.4 retains contract-entry behavior in erased callable values and the accepted Self/interface restrictions.

N3.1.5 proves bare obligations before execution, emits only explicit consequence branches, uses the body candidate for ensures-else without revalidation, bypasses ensures after requires-else rejection and keeps true tail transfers free of residual work. Pure lazy initializers are not changed into effectful bootstrap code.

N3.1.7 must exercise coldness, repeated explicit awaits, per-await dynamic guard, correct complete latent row, sealed world/provider/generation and scope/rights/counter checks, acquisition recipe order and safe projections. No source-level host constructor, unchecked path fallback or implicit retry is allowed.

## Validation join

Every case runs against independent expectations on interpreter-v2, verified stepper and actual native target with quota one/unbounded. Reidentify and complete the new-profile parity inventory: old-source entries are not current evidence. wasm-transition-v2 remains only its retained old-ABI coverage; unsupported new capability semantics are visible gaps.

For each feature include the closest rejected source and mutated IR, canonical result/error/origin/audit observation, memory balance/liveness and deep cases as relevant. Every advertised native-v1 case passes on all three targets at M2. Real argv/streams/files/startup/status tests complement fake worlds.

Build manifests derive from closed snapshots, not database publication. Reproducibility precedes signing and compares bytes across clean offline checkout paths. Cutover removes active Wasm contracts/crates/dependencies/profile only after full native readiness while keeping the interpreter and shipped v0.0.1 evidence.

## Completion reporting

Each package records tested base/commit/platform/profile, actual commands/exit statuses, pass/fail/unsupported/not-run counts, parity cells moved, named proof/diagnostic/schema cases and remaining scope. No failing expectation is wholesale regenerated. Refer to the owning [M1](../v0.0.3/m1/README.md), [M2](../v0.0.3/m2/README.md) and [M3](../v0.0.3/m3/README.md) for complete gates.
