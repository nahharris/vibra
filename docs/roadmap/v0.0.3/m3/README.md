# v0.0.3 M3: build, distribution, and cutover

**Planned.** [M2](../m2/README.md) qualifies all three platforms. This
milestone ships `vibra build`, makes builds reproducible, proves a clean
install, and removes the WebAssembly backend.

## Scope

The work packages are X3-23 to X3-27 of the
[Nexus plan](../../nexus/plan-v0.0.3.md).

1. **`vibra build`.** `vibra [GLOBAL...] build [TARGET] [--target TRIPLE]`,
   the host triple by default. An unsupported target, a missing link tool,
   and a link failure are structured diagnostics. Lowering and code
   generation acquire nothing: files, tools, and dependencies are found by
   the build driver only.
2. **Inputs and manifest.** The canonical build record of the
   [programs chapter](../../../spec/04-programs-and-packages.md#build-products):
   source and dependency identities; claim, witness, approval, and policy
   identities; registry, driver, and tool versions; target, CPU baseline,
   ABI, link inputs, and archive; and the origin table's identity.
3. **Reproducible builds.** Per target, two clean offline builds in
   different checkout paths produce byte-identical executables and
   manifests, compared before signing. An unknown tool or SDK version is
   rejected.
4. **Clean install.** On a clean Windows machine with no Rust tools: install
   the package, inspect what it requires, build the release program of
   v0.0.2, and run it with its arguments, streams, scoped files, and exit
   code. A linker or SDK the user must install is stated up front.
5. **Cutover.** The Wasm profile, emitter, runner, dependencies, build
   contract, and packaging references are removed together. The interpreter
   and native conformance remain, and `run`, `test`, and `eval` still work
   without a linker. The shipped v0.0.1 evidence is not touched.
6. **Release audit.** Admission, the bridge, driver authority, memory
   limits, error and exit mapping, schemas, release metadata, and the
   support inventory. Both suites and the tooling regressions on all three
   targets.

## Manifest and admission

- **Three identities, kept apart.** A source change moves the portable input
  identity. A target, link, or ABI change moves the artifact identity. A
  change of authority policy is visible by itself, so it can be reviewed
  without reading either of the other two.
- **Admission before the entry.** The descriptor and manifest checks cover
  the ABI, the registry, and the driver, world, and policy requirements. The
  static effect and guard reports stay bound to the exact code, even though
  rows are erased in execution.
- **Nothing live is serialized.** No grant, callable, or plan is written in a
  form that could restore authority.
- **No hash cycle.** Identities are computed in prerequisite order: inputs,
  object, executable, manifest payload, signature. A payload excludes its
  own hash and signature. Signing comes after the reproducibility
  comparison. Changed pinned tooling is a changed identity, never a cache
  hit.

## Exit evidence

- Per platform, the commands and counts, and the complete parity inventory.
- A search of the active crates, manifests, workflows, and specification for
  Wasm dependencies and references, with each remaining historical mention
  listed.
- The clean-machine record: the whole workflow of create, repair, test,
  build, distribute, and run, on Windows.
- No failure was removed, and nothing that was not run is reported as
  passed.

The release is complete only with all three targets.

## Not in this milestone

A public native-library ABI, a foreign function interface, a package
registry, a debugger, and service recovery.
