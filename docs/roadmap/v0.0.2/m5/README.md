# Milestone 5 — Native foundations and release

v0.0.3 can introduce Nexus and a native backend without changing
the codebase, query, or edit model and without losing the interpreter oracle.

## Handoff

- Project a backend-neutral build input from one admitted checked revision.
  Record canonical source and resolved graph identities, stdlib and registry
  provenance, the selected project target, its effects, and source origins.
  Define and test its semantic digest. Keep it separate from workspace
  revisions, checked object hashes, and native artifact cache keys.
- Specify the native artifact key inputs for v0.0.3, including the platform
  triple, CPU baseline, compiler and linker identities, normalized flags,
  runtime ABI, and selected executable target. Specify the build-result
  envelope and diagnostics for unsupported targets, missing tools, and
  incompatible artifacts before adding producers that can claim those results.
- Review the Nexus execution boundary against the checked IR and interpreter.
  Carry forward source origins, checked calls, closed effect rows, registry
  requirements, canonical outcomes, and audit events. Record the proposed
  frame, resume, and host-call contracts and their proof cases for v0.0.3.
  Keep the execution IR, code generator, and production Nexus implementation
  in v0.0.3.
- Confirm the native target matrix and identify the object, linker, and
  platform adapter prerequisites for Windows x64, Linux x86-64, and macOS
  arm64. Check Windows first to surface PE/COFF and SDK constraints early.
- Freeze the v0.0.1 execution baseline and make its fixtures reusable for
  interpreter/native differential testing. Include indirect cross-module tail
  transfer, non-tail memory exhaustion, module initialization, mapped traps,
  and ordered host events. Keep interpreter/Wasm differential coverage active
  during the transition.

## Gates

- The same admitted project inputs produce the same semantic digest from
  different checkout locations. Source, graph, trusted stdlib, or semantic
  registry changes invalidate the relevant identity. Platform and linker
  changes affect the native key independently.
- The v0.0.3 roadmap links the reviewed contract decisions, fixture inventory,
  and native cutover criteria. The release audit reviews the supported tooling
  inventory and the integrated agent workflow in the v0.0.2 release gate.
