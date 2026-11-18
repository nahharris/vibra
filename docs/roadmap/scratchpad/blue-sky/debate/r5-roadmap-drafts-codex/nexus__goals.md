# Nexus delivery goals

Status: planned. These goals restate [v0.0.3](../v0.0.3/README.md), [v0.0.4](../v0.0.4/README.md) and their milestone gates; they do not add source semantics. [The specification](../../spec/00-charter.md) wins.

## Native release goals

| ID | Required outcome | Owning milestone |
| --- | --- | --- |
| G3.1 | Complete admitted K3 lowering and verified immutable execution IR, explicit whole-program unsupported results and bounded rejection. | v0.0.3 M1 |
| G3.2 | Runtime frames, correct semantic tail transfer, arena/iterative RC and memory host outcome. | M1, stress M2 |
| G3.3 | Bare proof erasure and exact requires/ensures/where consequence semantics, no guard-failed trap. | M1 |
| G3.4 | Cold typed Intent, fresh await, sealed grants/broker limits/typed refusal, startup recipes, safe audit/observation. | M1 |
| G3.5 | Qualified Windows x64/Linux x86-64/macOS arm64 real process execution across all executable native-v1 cases, no pending cells. | M2 |
| G3.6 | Quota-one/unbounded parity under equivalent worlds, deep frame/drop/resource tests and compiler/runtime/unsafe audits. | M1/M2 |
| G3.7 | Public native build plus canonical portable/artifact provenance and exact compatibility admission. | M3 |
| G3.8 | Clean offline byte-equal builds from different paths per target; clean Windows install/build/distribute/run useful gate-1 program. | M3 |
| G3.9 | Coordinated active Wasm removal after readiness; interpreter remains permanently and tooling regressions pass. | M3 |

## Library and local-concurrency goals

| ID | Required outcome | Owning milestone |
| --- | --- | --- |
| G4.1 | Exact dependency acquisition/locking, real typed integration and upgrade/repair with visible conversion/authority/assurance cost. | v0.0.4 M1 |
| G4.2 | Thin MCP/selected editor adapters over the shared snapshot/query/eval/test/edit engine and equivalent machine facts. | M1 |
| G4.3 | Specified scoped child/join/cleanup and bounded ordered pure map; receive-loop actor lifetimes, no unapproved language forms. | M2 |
| G4.4 | One shared typed kernel: bounded mailboxes, protocol refusal, duplicate reply, exit/timeouts/blocked outcomes and simulation. | M2 |
| G4.5 | Valid schedule replay with equal outcomes/events/origins, invalid/truncated/leftover schedules explicit errors. | M2/M3 |
| G4.6 | Exclusive pinned native contexts, copied neutral messages, checked broker delegation and explicit callable-capture transfer. | M3 |
| G4.7 | Real two-worker overlapping progress and bounded stress/cost evidence, without universal speedup/global-order claims. | M3 |

## Handoff and evidence

v0.0.2 supplies K3 interpreter/closed ceilings/grants/static checker, pinned snapshots/approval and the revised native slice. Its M0 owns Windows scope-adapter feasibility; X-K3c is run only after the real M2 engine exists and its study freeze passes. Native/runtime implementation is not duplicated for that study.

v0.0.5 supplies service/late-outcome and selected recovery/exploration evidence. Native or actor support alone does not close those goals.

Each goal closes only with actual current-profile commands/counts and named independently authored cases in its milestone record. No goal is closed by this plan rewrite, a source crate's existence, unavailable target or old-profile measurement.
