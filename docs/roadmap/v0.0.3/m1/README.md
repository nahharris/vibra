# Milestone 1 — Native execution slice

A linked program proves the boundary between the
compiler, Nexus, and the closed host registry on a small fixture.

## Delivery order

1. Review the transition specification. Close execution IR verification,
   runtime frame ownership, resume and quota semantics, source origin
   mapping, the ABI and bridge boundary, and the native parity profile.
   Record toolchain and target feasibility for Windows, Linux, and macOS
   early, including their link and launcher requirements. The draft is the
   [Native execution](../../../spec/06-runtime.md#native-execution) section; it
   states resume semantics without defining a quota, which stays a toolchain
   control.
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
