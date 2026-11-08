# Milestone 3 — Reproducible builds and Wasm cutover

The supported toolchain builds and ships native executables with
auditable inputs and reproducible outputs.

## Delivery order

1. Expose native build and explicit platform selection through the shared
   CLI/tooling contract. Emit the executable and canonical `.build.vibon`
   manifest with semantic input identity, artifact hash, target and CPU
   baseline, toolchain and linker provenance, ABI and registry requirements,
   required effects, and source-map identity. Produce structured build
   results and diagnostics.
2. Pin toolchain and system link inputs per target. Run clean offline
   builds from different checkout paths on each platform. Compare bytes
   within each target before signing. Package the toolchain and document
   the supported commands and platform profiles. Smoke-test a clean
   Windows-native install and build on Windows.
3. After all three native targets pass parity and reproducibility, switch active
   backend conformance to interpreter/native. Retire the active Wasm build
   contract, emitter, runner, backend dependencies, and packaging references.
   Retain v0.0.1 release evidence and reusable fixtures as history. Update
   the affected specification chapters and release profiles in the cutover.

## Exit gate

- All three platforms pass the supported executable corpus and repeatable-build
  checks. The host suite, independent Vibra corpus, and v0.0.2 tooling
  regressions pass on the published support matrix.
- Audit artifact admission, the closed host bridge, runtime limits, error
  mapping, and release metadata. Deliberate changes to the linker, target,
  or ABI affect artifact identity and leave the portable input identity
  intact.
- Verify that ordinary `run`, `test`, and interactive evaluation still use
  the interpreter. Review native packaging and release evidence before
  tagging. Track any requirements for the broader full conformance profile
  separately from the pre-alpha support claim.
