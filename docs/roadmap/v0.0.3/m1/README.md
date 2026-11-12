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

## Code generator record (X3-06)

Recorded 2026-11-12 on the branch `nexus/x3-06`, Windows
`x86_64-pc-windows-msvc`, `rustc 1.96.1`. `vibra-codegen` turns a
`vibra_exec::Verified` program into an object file; it links and runs nothing,
and **no generated code was executed** in this package. Execution evidence is
owed by X3-07.

- **Cranelift.** `cranelift-codegen`, `cranelift-frontend`, `cranelift-module`,
  and `cranelift-object`, each pinned `=0.136.2`, the version of
  `cranelift-codegen` that Wasmtime already locked. Only `vibra-codegen` names
  them (`only_codegen_depends_on_cranelift`), and the lock file holds one
  version of each (`the_lock_file_holds_one_cranelift_and_the_generator_pins_it`).
  No Cranelift feature was enabled: the x86-64 ISA the lookup finds for both
  targets comes from `host-arch`, so a build machine of another architecture
  cannot emit either target until the `x86` feature is enabled (see the open
  points of the [feasibility record](feasibility.md)).
- **Lock file.** Two packages were added, with the checksums the feasibility
  record observed, and the workspace crate `vibra-codegen`; no locked package
  changed version.

  | Package | Version | Checksum |
  | --- | --- | --- |
  | `cranelift-module` | 0.136.2 | `382105ee9ba862bbca7f7b7119ef507125309764a0ee6cbcf46d1e0958e1dafc` |
  | `cranelift-object` | 0.136.2 | `7e1d0b304a1a852087669b13a26f341c1984c15ba0f4f2d9b3182e67e69782c4` |
- **Flags.** `Target::flags` is the one list, set on `settings::builder()`
  with the ISA found by `isa::lookup_by_name(<triple>)` and no host CPU
  detection: `opt_level = none`, `is_pic = true`, `enable_verifier = true`,
  `unwind_info = true`, and the Cranelift 0.136.2 defaults of the feasibility
  record restated (`regalloc_algorithm = backtracking`, `tls_model = none`,
  `libcall_call_conv = isa_default`, `enable_probestack = false`,
  `probestack_strategy = outline`, `probestack_size_log2 = 12`, the two
  spectre-mitigation flags `true`, and the rest `false` or `0`).
- **Object size of the demo program** (`main`, `even`, `odd`; 1000 rounds):

  | Target | Format | Bytes |
  | --- | --- | --- |
  | `x86_64-pc-windows-msvc` | PE/COFF | 4,391 |
  | `x86_64-unknown-linux-gnu` | ELF64 | 4,376 |
- **COFF indirection.** On Windows, Cranelift reaches each imported routine and
  each operand list through a read-only pointer slot (`.refptr.<symbol>`), so
  a call is `call [slot]`. That is one more load than a direct call, and it is
  part of the unoptimized baseline X3-22 measures.
