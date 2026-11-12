# Milestone 1 — Native execution slice

A linked program proves the boundary between the
compiler, Nexus, and the closed host registry on a small fixture.

Toolchain and target feasibility record: [feasibility.md](feasibility.md).

## Delivery order

1. Review the transition specification. Close execution IR verification,
   runtime frame ownership, resume and quota semantics, source origin
   mapping, the ABI and bridge boundary, and the native parity profile.
   Record toolchain and target feasibility for Windows, Linux, and macOS
   early, including their link and launcher requirements.
2. Lower a checked program into explicit blocks, frame slots, direct and
   indirect calls, tail transfer, resume points, and host calls. Verify
   layouts and transfers before emission. Emit target objects with Cranelift
   as the proposed first AOT emitter and link the launcher and Nexus runtime
   archive with the selected platform toolchain.
   [Cranelift object emission API](https://docs.rs/cranelift-object/latest/cranelift_object/struct.ObjectModule.html)
3. In Nexus, push runtime frames for non-tail calls, replace the current
   activation on a tail transfer, and return control to a dispatcher at a
   checkpoint. Route generated host calls through the shared registry.
   Keep handles and compiler-private layouts inside the execution context.
   Review this boundary for the [actor handoff](../../v0.0.4/README.md).
   Leave handle representation private and record which checkpoint and
   source-origin contracts v0.0.4 can extend. Record the proposed split
   between [message authority and performed effects](../../v0.0.4/README.md#capabilities-and-effects)
   in the handoff.
4. Run the Windows x64 fixture first. Prove PE/COFF object emission, the
   pinned MSVC link path, and direct `.exe` launch from Windows. Exercise
   the Linux x86-64 and macOS arm64 object, linker, and launcher paths
   before widening coverage.

## Gates

- **Demo.** Launch an executable with an indirect cross-module tail call, a
  forced yield and resume, one audited host operation, and a mapped trap.
  Compare canonical observations with the interpreter on a deterministic
  test host.
- **Exit.** Yielding around the host call does not duplicate its event.
  Tail transfers keep activation growth bounded. Invalid execution IR,
  incompatible runtime identity, and unknown registry entries fail before
  they can execute. Record the fixture results and the remaining executable
  corpus by feature and platform.

This slice proves the native path. General native support starts at the
parity and release gates.

## Native parity inventory

The inventory is [`conformance/native-parity.tsv`](../../../../conformance/native-parity.tsv):
one row for each of the 173 executable cases and the columns `stepper`,
`windows`, `linux`, and `macos`. A cell is `matched` or `pending <package>`.
Only the `stepper` column is executable today; the platform columns are
checked for form. The conformance runner prints
`native backend: stepper N matched, P pending, F failed`; a cell the stepper
contradicts fails the run, and a pending cell is a gap that fails the M2 gate.
`vibra-conformance --root conformance/cases --write-native-parity <path>` writes
the table the current executors produce; the committed file is the source of
truth and the test suite never writes it.

Counts at X3-04 (173 cases):

| Column | matched | pending X3-09 | X3-10 | X3-11 | X3-12 | X3-13 | X3-14 | X3-15 | X3-16 | X3-18 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `stepper` | 19 | 0 | 6 | 89 | 27 | 6 | 2 | 2 | 2 | 20 |
| `windows` | 0 | 19 | 6 | 89 | 27 | 6 | 2 | 2 | 2 | 20 |
| `linux` | 0 | 19 | 6 | 89 | 27 | 6 | 2 | 2 | 2 | 20 |
| `macos` | 0 | 19 | 6 | 89 | 27 | 6 | 2 | 2 | 2 | 20 |