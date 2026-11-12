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


## Bridge archive record (X3-05)

Recorded 2026-11-12 on the branch `nexus/x3-05`, Windows
`x86_64-pc-windows-msvc`, `rustc 1.96.1`. The contract is the crate
documentation of `crates/vibra-nexus-bridge`; it is provisional until the
native-execution chapter of the runtime specification is approved.

- **Archive.** `cargo build --locked --offline -p vibra-nexus-bridge --release`
  writes `target/release/vibra_nexus_bridge.lib`, 18,725,154 bytes. It holds
  Nexus, the host providers, the native code, and the launcher's `main`.
- **Native static libraries.** `cargo rustc -p vibra-nexus-bridge --release
  --crate-type staticlib -- --print native-static-libs` prints
  `kernel32.lib ntdll.lib userenv.lib ws2_32.lib dbghelp.lib
  /defaultlib:msvcrt`. That is the list the [feasibility record](feasibility.md)
  observed for its std-using stand-in archive: no difference.
- **Exported symbols.** `main`, and the routines `nexus_call`,
  `nexus_call_value`, `nexus_tail_call`, `nexus_tail_call_value`, `nexus_ret`,
  `nexus_host`, `nexus_global`, `nexus_trap`, `nexus_scalar`, `nexus_constant`,
  `nexus_copy`, `nexus_move`, `nexus_drop`, `nexus_op`, `nexus_build`,
  `nexus_cell`, `nexus_variant`, `nexus_function`, `nexus_branch`, and
  `nexus_switch`. The one data symbol a program defines is `vibra_program`.
- **`unsafe` blocks.** 22 in `src`: 20 in the default build and 2 more behind
  the `harness` feature. The crate's lint tables deny an undocumented one.
- **Not run.** Linux and macOS archives, and a link of the archive with an
  object from the code generator, which does not exist yet.
