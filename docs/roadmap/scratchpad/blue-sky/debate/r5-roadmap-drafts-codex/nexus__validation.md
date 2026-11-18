# Nexus validation and gate map

Status: required evidence plan, no rewritten-profile pass claim. Follow [execution.md](../execution.md), the active specification and owning milestone. A plan/test name is not evidence that the test exists or passed.

## Standing rules

1. Independently author expectations from the specification. Interpreter-v2 is the semantic oracle, not a license to replace an independently discovered specification defect with its output.
2. Lower/verify/step/emit whole admitted programs. A form outside the slice names itself and rejects; never execute a partial program.
3. Pending, unsupported, not-run and exhausted checks are gaps, not passes. No case is dropped/downgraded to make a release green.
4. Every sequential native comparison runs with checkpoint quota one and unbounded under fresh equivalent deterministic worlds. Compare value/error/status, origins and completed audit events; not elapsed time or host instruction counts.
5. Each IR extension brings instruction, verifier ownership rule/rejection cases, runtime, lowerer, stepper, emitter and observations together.
6. The bridge is the only unsafe source boundary we own; each block's safety preconditions are documented and audited. Dependency/OS/compiler/linker trust remains explicit.
7. Nothing is advertised as implemented before its complete profile/CLI/schema gate. An unavailable command stays unavailable; old-profile scaffolding does not establish K3 support.
8. Bare proof obligations reject if unknown/refuted; no hidden runtime fallback. Explicit consequences preserve type and candidate/preflight semantics. No new user-data trap is accepted as a positive case.
9. Grant checks include rights, actual resource/scope, world/provider/generation and shared limits. Fake-world success is not actual OS confinement evidence.
10. Contradictory design/spec/validation stops that slice until the contract is corrected; never invent codes/semantics in a test and treat the result as authority.

## Commands and interpretation

Run from the repository root using the pinned toolchain. These are future required validation entry points, not commands executed by the rewrite. Check every native exit status: PowerShell does not automatically stop on nonzero native exit.

```powershell
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-targets --all-features
cargo doc --locked --offline --workspace --no-deps --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
git diff --check
```

A failed command or dependency-unavailable condition is reported with its exact exit/status, not followed by a blanket green summary. Focused crate suites include vibra-exec, vibra-lower through conformance, vibra-nexus, vibra-nexus-eval, vibra-codegen, vibra-nexus-bridge and vibra-build. Adapters/kernel packages add their own focused suites. Both host-language tests and independent Vibra corpus cases are required; one cannot substitute for the other.

Use architecture/ambient-read/diagnostic/evidence/schema checks with their actual test entry points when implemented. Do not copy stale baseline counts from the preceding language. Every evidence record states base/tested commit, profile, target, exact command and pass/fail/not-run counts.

## Profiles and comparison

[Conformance profiles](../../spec/07-diagnostics-and-conformance.md#conformance-profiles) define interpreter-v2, native-v1 and their static/tooling obligations. During transition wasm-transition-v2 keeps the retained old ABI's stated coverage under the K3 front end. New semantics absent from that ABI are unsupported gaps, not a new full Wasm-v2 grant runtime.

Native-v1 support requires all admitted executable cases on actual Windows/Linux/macOS, not merely the stepper. Maintain the parity inventory against current case IDs and owning packages. A matched cell contradicted by execution fails; a pending cell blocks full-profile release. Only reviewed current observations move cells.

The native harness observation/world/registry schemas are versioned and pinned. Canonical persistent data has no encoding that restores live grant/callable/Intent authority. Entry status observes the declared ExitCode contract, not an enum-shape guess.

## Required boundary campaigns

| Boundary | Independent positive and negative evidence |
| --- | --- |
| Verifier | Bounded malformed metadata/ID/slot/ownership/arity/resume mutations, every error family, deterministic display and no panic/unbounded allocation. |
| Source/lowering | Every kept form, written operand order, exact patterns/types, bare obligation rejection, explicit consequence branches, type/interface/callback restrictions. |
| Runtime memory | Deep frame/drop/copy, tail bounds, stable retained size, new capture/grant graph liveness and memory host outcome. |
| Host/grants | Legitimate allowed actions, typed refusal/unknown, stale/world/counterfeit token, scope/rights/intersection/shared limits, startup failure/cleanup, no yield redispatch. |
| Filesystem | Actual platform handle/reparse/name/non-truncation/creation/race tests, fresh walk per invocation, root replacement, hard-link/object lifetime consent limits. Windows feasibility is v0.0.2 M0. |
| Native link/admission | Actual target launch, explicit pinned tool discovery, ABI/registry/driver/policy mismatch before entry, Unicode paths/argv/streams/status. |
| Build | Closed inputs, claim/witness/approval/policy identity changes, per-target offline byte equality before signing and no self-hash cycle. |
| Concurrency | Kernel/source transition cases, valid/invalid replay, bounded scopes/queues, transfer/delegation, exclusive owner stress and actual overlap. |
| Service/recovery | Late revision/correlation mutant, crashes at declared boundaries, unknown-outcome reconciliation and stated finite exploration domains. |

## Gate map

| Goal | Evidence owner |
| --- | --- |
| G3.1/G3.2/G3.3/G3.4 | v0.0.3 M1 steps N3.1.1–N3.1.8 and current-profile slice corpus. |
| G3.5/G3.6 | v0.0.3 M2 actual-platform parity, memory/driver campaigns and boundary audits. |
| G3.7/G3.8/G3.9 | v0.0.3 M3 build/manifest/reproducibility/install/cutover/release audit. |
| G4.1/G4.2 | v0.0.4 M1 real integration/upgrade and adapter facts. |
| G4.3/G4.4/G4.5 | v0.0.4 M2 specification, shared simulation/source cases and replay. |
| G4.6/G4.7 | v0.0.4 M3 native transfer/workers, stress and real hardware overlap. |
| Gate 3 | v0.0.5 M1–M3 service, selected recovery and bounded exploration. |

## Reporting and exclusions

No baseline or qualification result is asserted by this rewrite. Prior native fixtures are reusable only after their expectations are reviewed for the new source/runtime profile. Shipped v0.0.1 evidence remains immutable history. Every release report separates observed, argued and unrun claims.

No unsafe audit is a sandbox proof for foreign native executables; no simulation is a proof of every schedule; no @once/yield test is crash exactly once; no timing baseline establishes a universal speedup or cost bound.
