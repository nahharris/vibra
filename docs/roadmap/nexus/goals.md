# Nexus goals, v0.0.3 and v0.0.4

A review of what the roadmap commits Nexus to. Each row restates one
deliverable or gate clause of the source document and gives it an ID. The work
packages cite these IDs, and [validation.md](validation.md#gate-map) maps each
to the evidence that closes it. Nothing here adds a goal: where this page and
a roadmap disagree, the roadmap wins.

Sources: [v0.0.3](../v0.0.3/README.md) and its [M1](../v0.0.3/m1/README.md),
[M2](../v0.0.3/m2/README.md), and [M3](../v0.0.3/m3/README.md);
[v0.0.4](../v0.0.4/README.md); the
[v0.0.2 M5 handoff](../v0.0.2/m5/README.md); the
[Nexus scratchpad](../scratchpad/execution-targets/nexus.md);
[GitHub #408](https://github.com/nahharris/vibra/issues/408).

## What Nexus is

Nexus is the runtime that a native Vibra executable links. The compiler
proves and describes what can execute; Nexus decides when and where it
executes; the reference interpreter stays the oracle for what it means. For
v0.0.3 Nexus owns activations, value storage, execution checkpoints, traps,
and host dispatch. v0.0.4 adds actors, mailboxes, message authority, and
workers on top of the same boundary.

## v0.0.3 — release-level

| ID | Goal |
| --- | --- |
| G3.R1 | Native executables on `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, and `aarch64-apple-darwin`, each qualified on its own platform with a pinned link environment |
| G3.R2 | Source resolution, checking, queries, and edit planning stay shared. Checked IR lowers to a **verified execution IR**, then CPU objects, linked with a launcher and Nexus |
| G3.R3 | AOT executables and **cooperative** checkpoints only. No GPU, remote execution, native libraries, native debugger, or cross-compilation |
| G3.R4 | The native contract and its transition profile land in the specification **before** the first native implementation slice |
| G3.R5 | The Wasm contract and its differential tests stay in force until all three native targets pass. If one platform slips, Wasm coverage stays and the native slice is preview evidence |
| G3.R6 | Full interpreter parity for every executable case of the supported profile on all three platforms. An unavailable native case is a gap |
| G3.R7 | `run`, `test`, and interactive evaluation stay interpreter-backed. v0.0.2 tooling regressions pass |
| G3.R8 | Carry forward to v0.0.4: runtime-owned frames, opaque context-local handles, and source-linked host events |

## v0.0.3 M1 — native execution slice

| ID | Goal |
| --- | --- |
| G3.M1.1 | Transition specification reviewed: execution-IR verification, runtime frame ownership, resume and quota semantics, source-origin mapping, the ABI and bridge boundary, the native parity profile |
| G3.M1.2 | Toolchain and target feasibility recorded for all three platforms, link and launcher requirements included, Windows first |
| G3.M1.3 | A checked program lowers to explicit blocks, frame slots, direct and indirect calls, tail transfer, resume points, and host calls. Layouts and transfers are verified before emission |
| G3.M1.4 | Target objects emitted with Cranelift as the proposed first AOT emitter, linked with the launcher and the Nexus archive by the selected platform toolchain |
| G3.M1.5 | Nexus pushes a frame for a non-tail call, replaces the activation on a tail transfer, and returns control to a dispatcher at a checkpoint |
| G3.M1.6 | Generated host calls go through the shared registry. Handles and compiler-private layouts stay inside the execution context |
| G3.M1.7 | Actor handoff reviewed: handle representation left private; which checkpoint and source-origin contracts v0.0.4 extends; the proposed split between message authority and performed effects |
| G3.M1.8 | **Demo.** One executable with an indirect cross-module tail call, a forced yield and resume, one audited host operation, and a mapped trap; canonical observations equal the interpreter's on a deterministic test host |
| G3.M1.9 | **Exit.** Yielding around the host call does not duplicate its event |
| G3.M1.10 | **Exit.** Tail transfers keep activation growth bounded |
| G3.M1.11 | **Exit.** Invalid execution IR, an incompatible runtime identity, and an unknown registry entry fail before they can execute |
| G3.M1.12 | **Exit.** Fixture results recorded, and the remaining executable corpus recorded by feature and platform |
| G3.M1.13 | Windows x64 runs first (PE/COFF, pinned MSVC link, direct `.exe` launch); Linux and macOS object, linker, and launcher paths exercised before coverage widens |

## v0.0.3 M2 — native parity

| ID | Goal |
| --- | --- |
| G3.M2.1 | Coverage by feature, each with interpreter/native comparison and rejection cases: scalar and checked arithmetic; structural and nominal aggregates; pattern matching; generic instantiation; closures and indirect/interface dispatch; recursion; module initialization; assertions; the existing host operations |
| G3.M2.2 | All three targets compared on canonical results, traps and origins, test outcomes, CLI entry results, and ordered audit events, with fresh equivalent deterministic worlds |
| G3.M2.3 | Real process launches for platform-adapter smoke cases. Windows: paths with spaces and non-ASCII, arguments, standard streams, filesystem effects, exit status through the launcher |
| G3.M2.4 | Deep non-tail recursion consumes runtime-managed frame memory |
| G3.M2.5 | Forced yields around calls and long primitives preserve ordered effects and exactly-once execution |
| G3.M2.6 | Invalid bindings, unsupported targets, and incompatible artifacts have structured diagnostics |
| G3.M2.7 | **Demo.** An effectful multi-module fixture on all three platforms with forced yields and a mapped trap variant |
| G3.M2.8 | **Exit.** The complete supported executable corpus passes on all three targets. An emission failure on an admitted case is a parity failure |
| G3.M2.9 | **Exit.** Wasm coverage and the v0.0.2 agent workflow still pass |
| G3.M2.10 | **Exit.** Runtime and compiler boundary audits recorded, and an unoptimized performance baseline |

## v0.0.3 M3 — reproducible builds and Wasm cutover

| ID | Goal |
| --- | --- |
| G3.M3.1 | Native build and explicit platform selection through the shared CLI/tooling contract, with structured results and diagnostics |
| G3.M3.2 | Executable plus canonical `.build.vibon` manifest: semantic input identity, artifact hash, target and CPU baseline, toolchain and linker provenance, ABI and registry requirements, required effects, source-map identity |
| G3.M3.3 | Toolchain and system link inputs pinned per target. Clean offline builds from different checkout paths compare byte-equal within each target before signing |
| G3.M3.4 | Toolchain packaged; commands and platform profiles documented; clean Windows-native install and build smoke-tested |
| G3.M3.5 | After parity and reproducibility on all three: conformance switches to interpreter/native; the Wasm build contract, emitter, runner, dependencies, and packaging references retire; v0.0.1 evidence and reusable fixtures stay as history; the specification chapters and profiles change in the cutover |
| G3.M3.6 | **Exit.** Audit of artifact admission, the closed host bridge, runtime limits, error mapping, and release metadata. A deliberate linker, target, or ABI change moves the artifact identity and leaves the portable input identity intact |
| G3.M3.7 | **Exit.** Both suites and the v0.0.2 tooling regressions pass on the published support matrix |

## v0.0.4 — local actors

| ID | Goal |
| --- | --- |
| G4.1 | **Specify first.** Typed mailboxes, scoped send addresses, one-shot replies, ownership and transfer, target effect admission; ordering, full and closed mailboxes, actor exit, timeouts. Charter, language, effects, runtime, tooling, and conformance contracts updated before implementation |
| G4.2 | **Simulate.** One message handler at a time per actor; immutable state carried between turns; immutable snapshots shared; bounded mailboxes; deterministic scheduling with recorded park, wake, delivery, reply, and exit events |
| G4.3 | Source origins surfaced for the send, the handler, and the associated host operation |
| G4.4 | **Native.** The same actor semantics and replay cases on Nexus |
| G4.5 | Two actors run concurrently on separate CPU workers with single ownership of each actor's state |
| G4.6 | Addresses are independent of worker placement. Placement is selected before initialization and exposed through tooling |
| G4.7 | The first capability is a typed, unforgeable send address. Holding it grants its operations; passing it delegates the same authority |
| G4.8 | An actor can issue a narrower address, and the dispatch boundary enforces its operation and resource scope |
| G4.9 | The sender's performed row holds the messaging effect; the handler's row holds its host effects; the target still declares the host effects of every actor body it can start. Tooling shows the sender's direct effect and the causal path through the handler |
| G4.10 | Out of scope, each to be reopened by a concrete protocol: holder binding, revocation, general host-resource capabilities, distribution, live migration, general mutable references, a CPU fork/join API |
| G4.G1 | **Gate.** A document service with a state actor and a persistence actor: save one revision, edit while the write is pending, and the reply cannot mark the newer revision saved |
| G4.G2 | **Gate.** An out-of-scope save, a full mailbox, a duplicate reply, and an actor exit while a caller waits, each exercised |
| G4.G3 | **Gate.** A direct write outside the caller's effect ceiling is rejected, and a target that omits the handler's required effect is rejected |
| G4.G4 | **Gate.** One recorded schedule replays in the interpreter and in native execution on all three platforms, compared on outcomes, message and host events, and source origins |
| G4.G5 | **Gate.** A separate native fixture shows overlapping progress on two CPU workers |
| G4.G6 | **Gate.** The host suite and the independent Vibra corpus pass before tagging |

## Inherited from v0.0.2 M5

v0.0.3 is scheduled after this handoff. Hannah has chosen to start ahead of it
([decisions.md](decisions.md), N1), so each item is either produced early by
a Nexus package or is a named blocker of the package that needs it.

| ID | Handoff item | Needed by | Disposition |
| --- | --- | --- | --- |
| H1 | Backend-neutral build input and its semantic digest | X3-23 (manifest) | Blocker of X3-23 only. M1 and M2 need no digest |
| H2 | Native artifact key inputs, the build-result envelope, and the diagnostics for unsupported targets, missing tools, and incompatible artifacts | X3-21, X3-23 | X3-S1 drafts them with the transition specification; v0.0.2 M5 adopts or revises |
| H3 | Review of the Nexus boundary against checked IR and the interpreter; proposed frame, resume, and host-call contracts with proof cases | X3-S1 | Done here: [design.md](design.md) and the foundation tests are that proposal |
| H4 | Target matrix and object, linker, and adapter prerequisites, Windows first | X3-S2 | X3-S2 |
| H5 | The v0.0.1 execution baseline frozen as reusable differential fixtures | X3-04 | X3-04 reuses the corpus in place; freezing as history is X3-26 |
