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
one row for each executable case and the columns `stepper`, `windows`,
`linux`, and `macos`. A cell is `matched` or `pending <package>`. The
`stepper` column is always executable. The `windows` column is executable on
Windows when the runner is given the runtime archive (`--runtime-archive <path>`,
the `harness` archive; the runner neither builds nor searches for it): each case
the stepper completes is linked and launched twice, unbounded and with quota 1,
and held to the case's one expectation by the comparison the stepper is held to.
The executable's result is read from the launcher's structural dump of it by the
entry's result type, with the reader the stepper uses, and must equal the
stepper's result; a dump the launcher cut off leaves the case pending at X3-18.
The `linux` and `macos` columns are checked for form. The conformance runner
prints `native backend: stepper N matched, P pending, F failed`, then
`native backend: windows N matched, P pending, F failed` and the time of the
column, or `native backend: windows not run (<reason>)` without an archive or
off Windows. A cell the stepper or the executable contradicts fails the run, and
a pending cell is a gap that fails the M2 gate.
`vibra-conformance --root conformance/cases --write-native-parity <path>` writes
the table the current executors produce (add `--runtime-archive` for the
`windows` column); the committed file is the source of truth and the test suite
never writes it.

Counts at X3-09b (175 cases):

| Column | matched | pending X3-08 | X3-10 | X3-11 | X3-12 | X3-13 | X3-14 | X3-15 | X3-16 | X3-18 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `stepper` | 21 | 0 | 0 | 89 | 33 | 6 | 2 | 2 | 2 | 20 |
| `windows` | 21 | 0 | 0 | 89 | 33 | 6 | 2 | 2 | 2 | 20 |
| `linux` | 0 | 21 | 0 | 89 | 33 | 6 | 2 | 2 | 2 | 20 |
| `macos` | 0 | 21 | 0 | 89 | 33 | 6 | 2 | 2 | 2 | 20 |

Counts at X3-09c were 174 cases: `stepper` and `windows` 20 matched, X3-10 6 and
X3-12 27 pending in every column. At X3-10a the six cases that waited on X3-10
(the continuation chains and the tail loops that call a function value) turned
out to wait on `match` as well, so they now name X3-12, and the new case of the
division variant is `matched` in `stepper` and `windows`. No cell moved from
`matched` to `pending`.
Counts at X3-04 were 173 cases: `stepper` 19 matched, the other columns 19
pending X3-09.

## M1 evidence (X3-09a)

Recorded 2026-11-12 on the branch `nexus/x3-09a`, Windows
`x86_64-pc-windows-msvc`. The fixture is [`examples/nexus-m1`](../../../../examples/nexus-m1)
(two modules, an indirect tail call across them through a function value, a
module value, one `io.stdout.text`, and a loop whose count is one constant; at
X3-09a the count was held in 17 `bool` arguments, and X3-09b replaced it with an
ordinary `u64` counter) and the corpus case `V1-RUNTIME-nexus-m1-fixture`. The
evidence tests are in `crates/vibra-conformance/tests/it/m1_evidence.rs`. The
milestone exit gate is not passed by this record: it needs the Linux and macOS
runs and the owed rows below.

| Gate | Evidence | Status |
| --- | --- | --- |
| G3.M1.7 Executable corpus recorded by feature and platform | The table above, from `conformance/native-parity.tsv`; the `windows` column is checked by `native_parity_inventory::every_windows_row_matches_its_linked_executable` | Windows recorded; Linux and macOS not run |
| G3.M1.8 Compared with the interpreter, including the division variant and the unobservable-function variant | The fixture case matches in `stepper` and `windows`. The division variant was owed to X3-09b and is recorded below. The unobservable-function variant cannot be written: an entry result must be `void` or `result void e`, and `result.err` is pending X3-11 | Division variant recorded at X3-09b; the unobservable-function variant is owed |
| G3.M1.9 Yielding around the host call does not duplicate its event | `quota_one_has_one_stdout_event_and_yields_at_every_checkpoint`: on the stepper, one `io.stdout-text` event and a yield at every checkpoint (a checkpoint is a step the dispatcher takes); the linked quota-1 run's observation document equals the one derived from the stepper, holds one `io.stdout-text` event, and reports as many yields as the stepper, at least the stepper's step count less the last step, which completes instead of yielding; the unbounded run reports none | Passed on the stepper and the linked executable |
| G3.M1.10 Tail transfers keep activation growth bounded | `peak_frames_and_live_size_do_not_grow_with_the_loop_count`: peak frames and live size on the stepper do not grow with the loop count, and the linked executables, at 10,000 and 100,000 rounds, report equal peak frame counts and peak live sizes within 64 bytes, equal to the stepper's | Passed on the stepper and the linked executable |
| G3.M1.11 Invalid IR, incompatible runtime identity, unknown registry entries fail before they execute | `a_program_mutated_after_verification_is_rejected_by_verify` (four mutations, and on Windows `build_program` answers `BuildError::Verify`), `an_object_with_another_abi_is_refused_by_the_launcher`, `a_descriptor_naming_an_unknown_symbol_is_refused` | Passed |
| G3.M1.12 Fixture results and remaining corpus recorded by feature and platform | The committed `conformance/native-parity.tsv` and the counts above | Recorded |

- **The `windows` column.** 20 cases matched, equal to `stepper`; none pending
  at the column's own package and none failed. The column took 29.9 s to run on
  this machine (the 174 cases, two runs of each linked executable, in the debug
  profile). Each matched case agrees with the stepper in the outcome, the trap
  code and origin, the audit trace, and the entry's result, under no quota and
  quota 1; the result is read from the launcher's dump, never lent.
- **The observation document.** Besides the outcome, the result kind, the trap,
  and the audit trace, the `harness` launcher writes `counters:` (yields, peak
  frames, tail transfers, peak live size, live size at the end) and `value:` (the
  result as a dump that names no type, bounded at 64 levels, 65,536 nodes, and
  1 MiB of text and bytes). It does not report what the module values keep: the
  context does not expose it (X3-17).
- **Not run:** Linux, macOS.


## M1 evidence (X3-09b)

Recorded 2026-11-12 on the branch `nexus/x3-10a`, Windows
`x86_64-pc-windows-msvc`, after the unchecked integer operations (X3-10a, N41).
The milestone exit gate is not passed by this record: it needs the Linux and
macOS runs and the owed rows.

- **The fixture.** [`examples/nexus-m1`](../../../../examples/nexus-m1) keeps its
  two modules, its indirect tail call of `finish` (a function value that
  `app.main` defined), its one module value, and its one `io.stdout.text`. The
  counter is an ordinary `u64`: `app.ticks.count` tests it with `equatable.equal`,
  subtracts one with `u64.sub`, and calls itself in tail position. The loop count
  is the one constant `1000u64` in the call in `app.main`, which the evidence
  tests rewrite. `V1-RUNTIME-nexus-m1-fixture` is byte-equal to the example
  (`the_fixture_case_of_the_corpus_is_the_example`).
- **The division variant.** `V1-RUNTIME-nexus-m1-fixture-division-by-zero` is the
  same project with a loop that divides by the counter
  (`u64.rem 1000000u64 remaining`), so after 1,000 tail calls the divisor is
  zero. It writes nothing and ends in `@runtime.division-by-zero`, which has no
  origin. Its case is `matched` in `stepper` and `windows`.
- **G3.M1.8.** `the_linked_fixture_equals_the_interpreter_on_result_trap_origin_and_trace`
  holds the interpreter to the corpus case's own files (the result, the trace,
  and the trap diagnostic with its empty span), and then holds the linked
  executables to the interpreter. The fixture's linked `harness` document says
  `completed`, no trap, origin 0, and the interpreter's one-event trace, and
  equals the stepper's document; its `system` executable exits 0 and writes
  `done`. The variant's `harness` document says `trapped`, the trap
  `@runtime.division-by-zero`, origin 0, and an empty trace, and equals the
  stepper's; its `system` executable exits 5, writes only the trap's atom to
  standard error, and nothing to standard output. The origin of an
  `@runtime.unobservable-function` trap would map to a span; that variant is
  owed to later packages (X3-11 for a `result.err` entry, X3-18 for the
  observation).
- **G3.M1.9 and G3.M1.10, re-derived on the new fixture.** At 1,000 rounds the
  stepper takes 5,012 steps (checkpoints) and, with quota 1, yields 5,011 times;
  the linked quota-1 executable reports the same 5,011 yields. At 10,000 and at
  100,000 rounds both the stepper and the linked executables report 3 peak
  frames and a peak live size of 512 bytes (50,012 and 500,012 steps), so the
  bounds of the tests (equal peak frames, peak live sizes within 64 bytes, and
  the work growing at least fivefold) hold with no change to the tests.
  G3.M1.11 is unchanged: its four mutations still find their tail call, branch,
  and host call in the new fixture.
- **Not run:** Linux, macOS.
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

## Link driver record (X3-07)

Recorded 2026-11-12 on the branch `nexus/x3-07`, Windows
`x86_64-pc-windows-msvc`, `rustc 1.96.1`. `vibra-build` discovers the MSVC
linker, links the generator's object with the runtime archive, and builds an
executable from a checked program. These are the first tests that **execute
generated machine code**: the linked programs below ran as processes and were
compared with the reference stepper. The typed errors `MissingLinkTool` and
`LinkFailed` are the planned diagnostics `@tool.missing-link-tool` and
`@tool.link-failed` of the specification package X3-S1, which is not merged;
the binding to the diagnostics registry is owed after it.

- **Crate.** `vibra-build` depends on `vibra-codegen`, `vibra-exec`,
  `vibra-ir`, and `vibra-lower`; it adds no registry crate and no `unsafe`, and
  the lock file gained the workspace crate and its edges only. It compiles on
  every platform: on another host, or for another target, it answers
  `BuildError::Unsupported`. Every environment read, process, and file access
  is in its module `system`
  (`ambient_reads::the_build_driver_reads_the_machine_in_one_module`).
  `vibra-cli` does not depend on it.
- **Discovery rule.** `Sources::from_machine` is the one function that reads the
  machine: `ProgramFiles(x86)` (default `C:\Program Files (x86)`),
  `vswhere.exe` at its documented place under it, run with
  `-latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64
  -property installationPath`, and `reg.exe` under `%SystemRoot%\System32`
  (never through `PATH`) for `KitsRoot10` in the 32-bit registry view. Both are
  run with an explicit environment: `vswhere.exe` returns nothing without
  `ProgramData` (observed: empty output with `SystemRoot` alone, the
  installation path with `SystemRoot` and `ProgramData`). `Toolchain::discover`
  is then a function of the `Sources` and the filesystem: the toolset is the
  line of `Microsoft.VCToolsVersion.default.txt` (else the numerically highest
  directory of `VC\Tools\MSVC`); the SDK is the numerically highest `Lib`
  version holding `um\x64\kernel32.lib` and `ucrt\x64\ucrt.lib`, under
  `KitsRoot10` or else `<ProgramFiles(x86)>\Windows Kits\10`. `PATH`,
  `VCINSTALLDIR`, `LIB`, and `INCLUDE` are never read. Found here: Visual Studio
  Build Tools 2022, toolset `14.43.34808`, SDK `10.0.22621.0`. The registry and
  `vswhere.exe` print in the OEM code page, so an installation path with
  characters outside it would be misread; not exercised.
- **Link command**, as a value (`link_command`), run without a shell:

  ```text
  <LINK> /NOLOGO /MACHINE:X64 /SUBSYSTEM:CONSOLE /OUT:<OUT> <OBJECT> <ARCHIVE>
      /LIBPATH:<MSVC-LIB-X64> /LIBPATH:<SDK-UCRT-X64> /LIBPATH:<SDK-UM-X64>
      kernel32.lib ntdll.lib userenv.lib ws2_32.lib dbghelp.lib /defaultlib:msvcrt
  ```

  It is the command of the [feasibility record](feasibility.md) with no
  difference: the same options, libraries, and order. The libraries are those
  `rustc --print native-static-libs` printed for the real archive (see the
  bridge record above). A test asserts the list against this literal
  (`the_link_command_is_the_recorded_literal_list`, and on a real build
  `linked_exe::the_link_command_is_the_recorded_one`), and every failure message
  of a linking test carries the full command. Arguments with spaces and
  non-ASCII characters reach `link.exe` intact as single arguments (the process
  is started from an argument list, which Windows quotes): no response file was
  needed, and none is implemented. A command line over the Windows limit would
  fail at the start of the process, as a `LinkFailed` with no exit code.
- **Environment of the linker.** `TMP`, set to the work directory, and nothing
  else. Observed by linking one object with each environment: empty, and
  `SystemRoot` alone, both fail with `LNK1104` (the linker cannot create
  `C:\WINDOWS\lnk{...}.tmp`); `TMP` alone, `TEMP` alone, and `SystemRoot` with
  both, link. `SystemRoot` is therefore not needed. `link.exe` also does not
  report a missing library given by a `\\?\` path (the linker drops it and the
  link fails later on unresolved symbols), so the driver checks that the archive
  exists and names it as a `BuildError::Io` before it writes or links anything.
- **The runtime archive is an input** (`BuildOptions::archive`); the tests build
  it with the `CARGO` of the test run into `target/x3-07-archive-system` and
  `target/x3-07-archive-harness`, once per process each. Both are built because
  each exposes what the other cannot: the shipped `system` launcher gives the
  exit code, the real standard output and the standard error; the `harness`
  launcher (cargo feature `harness`) reads a host world from standard input and
  writes the observation document (outcome, result kind, trap code and origin
  ordinal, audit trace). The `system` archive is 18,731,428 bytes.
- **Size and time.** The linked demo executable is 247,808 bytes with the
  `system` archive and 812,032 bytes with the `harness` archive; the other
  linked programs are 203,264 to 282,624 bytes (`system`). One `link.exe` run
  took 76 to 87 ms (a PowerShell stopwatch around the process); a whole build
  (lowering, verification, generation, object write, link) of a small program
  took about 75 to 150 ms in the test profile.
- **What is compared.** For the `system` launcher: the exit code, the standard
  output bytes, and the standard error text (for a trap the code line with the
  origin, `@runtime.division-by-zero at app/main.vib:20..21`; for an `err` the
  line `@command.program-failed`), against the stepper's run (the output
  captured through `SystemProvider::capturing`) and against an expectation
  written by hand. For the `harness` launcher: the whole observation document,
  with the audit trace, under no quota and quota 1, against the document built
  from the stepper's run. Programs of the slice are lowered by
  `build_executable`; the trap, `err`, and demo programs are execution IR built
  by hand and linked by `build_program`, because the lowering emits no `Trap`
  and cannot build a `result.err` yet (`the_slice_cannot_express_an_err_entry_yet`).
  The deep loops compare the unbounded run only and take their standard output
  from the hand-written expectation: the unoptimized stepper needs about nine
  seconds for a 100,000-round loop.
- **Linked programs** (every row: launcher outputs equal the stepper's):

  | Program | Exit | Standard output |
  | --- | --- | --- |
  | The lowering slice's demo (`everything`: module value, closure, module function as a value, three host writes, early `return`) | 0 | `labellateend` |
  | A scalar; a standard-library module value read twice; a `str` module value; a scalar module value | 0 | none |
  | Two `io.stdout-text` calls | 0 | `one\ntwo\n` |
  | An indirect call whose callee comes from a host-writing call | 0 | `first\nsecond\n` |
  | A tail call of a module function; of a closure; indirect, of a function value in a parameter; indirect, of a module function used as a value; mutual tail calls | 0 | none |
  | A closure reading its captures; an early `return` in the middle | 0 | none |
  | 100,000 direct tail calls (17 `bool` parameters) | 0 | none |
  | 100,000 indirect tail calls through a module value | 0 | none |
  | The foundation demo: 1,000 indirect tail transfers, one host write, return (hand-built IR) | 0 | `done\n` |
  | The same, ending in a `DivisionByZero` trap | 5 | `done\n`; standard error `@runtime.division-by-zero at app/main.vib:20..21` |
  | A trap at once | 5 | none; standard error `... at tests/trap.vib:3..9` |
  | An entry returning the `err` variant, `entry_outcome` 2 | 6 | none; standard error `@command.program-failed` |
  | Non-tail recursion 100,000 frames deep (hand-built IR) | 0 | none |

  Neither deep program overflowed the native stack. `entry_outcome` and
  `registry_minor` are computed from the checked program (`entry_outcome`: `0`
  unless the entry's result type is an enum with a variant named `err`, then
  `1` and its index, the rule by which the stepper and the interpreter classify
  a program failure), and the standard `result` gives 2
  (`the_entry_outcome_is_computed_from_the_entry_result_type`).
- **Not observable yet** (so not compared): the entry's result value (the
  launcher says only `void-or-ok` or `err`; the canonical value is X3-18, ledger
  N35); the context's live size and the arena balance (neither launcher
  exposes them; X3-17); the audit trace of the `system` launcher (it has none:
  the audit is compared through the `harness` launcher only); any host world
  other than the default one (files, environment, arguments, standard input,
  clocks); the memory-exhaustion exit (3) and an admission refusal, neither of
  which the programs here provoke; an exit code from the `harness` launcher
  (it is 0 whatever the program did).
- **Not run:** Linux, macOS.