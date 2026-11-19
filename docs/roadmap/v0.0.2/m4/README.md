# v0.0.2 M4: first native slice

**Planned.** One slice of the new checked language runs as a linked native
executable and agrees with the interpreter. This is the handoff to
[v0.0.3](../../v0.0.3/README.md), not the native release.

## Scope

- Reconcile the existing lowering, execution IR, verifier, stepper, code
  generation, bridge, runtime archive, and Windows linking with the
  contracts frozen in M0, following the [Nexus plan](../../nexus/README.md).
- Lower one vertical slice that contains: a guard with a consequence, a
  discharged bare guard that leaves no code, a plan built once and awaited
  twice, one scoped file operation through a bound grant, startup
  acquisition, and an `ExitCode` result.
- Produce the portable semantic inputs of that slice directly from a closed
  snapshot, in the form the build manifest of v0.0.3 will record.
- Record, per feature and platform, what the native path does not yet
  support.

Work that depends only on M0's contracts may start before M3 ends. This
milestone's gate is claimed after M3's.

## Exit evidence

- The interpreter, the reference stepper, and the linked Windows executable
  agree on result, exit code, and audit trace for the slice, with the
  scheduler quota at one step and unbounded.
- No host operation is submitted twice.
- The gap inventory.

## Not in this milestone

`vibra build`, Linux and macOS, and the full language on native code.
