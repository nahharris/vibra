# M2 step 35 — fixes of the review of drivers

The fixes of the findings of the [review of drivers](review-drivers.md), on
the step branch `m2/35-fix-drivers`, following the rulings on that review
and the sentences of spec batch 5 (03 "Filesystem policies", 04 "Startup
order", 05 "Streams", 06 "Standard input", "Host calls" and "Entry
outcome"). Each defect has its test in a commit of its own, seen to fail
there, and its fix in the next commit, where the test passes. A test that
could not compile before its fix is recorded with the compiler's error.

## Findings

| Finding | Class | Test | Fails at | Passes at |
| --- | --- | --- | --- | --- |
| F1, a Windows pipe is read as a regular file (probes 21, 24, 25, 26) | defect, fixed in `vibra-host/src/system/streams.rs` | `system::streams::tests::a_pipe_has_no_known_size`, with the control `a_regular_file_reports_what_remains` (`vibra-host`); `run_command::stdin_pipe_slow_producer`; `run_command::stdin_pipe_scalar_split_across_writes` (`vibra-cli`) | `24c5485f`: an empty pipe reports `Some(0)` where `None` is expected; a slow producer's `abc` is read as `[]`; a split scalar exits 6 | `0c9f625a` |
| F2, a UNC root is acquired and nothing below it opens (probe 2) | defect, fixed in `vibra-fs-scope/src/windows.rs` | `controls::a_root_on_a_remote_volume_is_unsupported` (`vibra-fs-scope`); `attacks_run::windows::atk_run_unc_root` (`vibra-workspace`); the unit test `a_root_is_held_only_on_a_local_volume_that_reports_its_name` | `f6a95e45`: `\\localhost\C$\…\inside` is held; the run is `@command.programFailed` where `@command.operationalFailure` is expected | `8ca4d341` |
| F3, existence is visible below a directory the policy does not permit (probe 19) | ruled a defect, fixed in `vibra-fs-scope/src/windows.rs`, `vibra-host/src/system/fs.rs` and the gate | `fs::atk_fs_existence_below_a_case_alias`; `fs::atk_fs_existence_below_a_short_name`, with the control `a_missing_entry_below_a_permitted_directory_is_not_found`; `broker::an_error_below_an_unpermitted_directory_is_denied` (`vibra-host`); `controls::a_failed_walk_reports_the_directories_it_opened` (`vibra-fs-scope`) | `24bafca0`: below `PUB` and `SECRET~1`, a missing `pub/y.txt` (read, stat), `pub/no/y.txt` (read, stat, write) and `pub/e` (list) are `notFound` where `denied` is expected | `72c3562f` |
| F4, a failed release is logged nowhere | defect, fixed in `Driver::release`, the startup release, the run and the command | `startup::a_failed_release_is_one_log_line` (`vibra-host`); `run::tests::a_failed_release_leaves_the_result_and_one_log_line` (`vibra-workspace`); `run::tests::a_failed_release_is_one_line_on_standard_error` (`vibra-cli`) | `3ea9aa54`: does not build: `Driver::release` returns nothing (E0053), and neither the startup record nor `RunOutcome` has a log (E0609) | `3b005bfb` |
| F5, the bytes the adapter keeps are lost to a later reader (probe 23) | ruled a defect for a regular file, fixed in `vibra-host/src/system/streams.rs` and `system.rs`; an accepted limit for a pipe | `run_command::stdin_file_is_positioned_after_the_bytes_delivered` (`vibra-cli`); `system::stdin_release_gives_back_the_kept_bytes` (`vibra-host`) | `b6a7ff74`: two runs on one handle on `abc€def`; the second starts inside `€` and exits 6 where `€def` is expected | `81fa75cd` |
| F6, an allocation the operating system refuses inside a driver is an error case | ruled a defect, fixed in `vibra-host` (`Completion::MemoryExhausted`, the broker, the random, standard input and file adapters) and in `vibra-interp` (`Machine::allocated_whole`) | `system::random_allocation_refused_is_memory_exhaustion`; `broker::a_driver_out_of_memory_is_memory_exhaustion` (`vibra-host`); `run::tests::a_driver_out_of_memory_ends_the_run_as_memory_exhausted` (`vibra-workspace`); `host::tests::a_host_built_value_is_accounted_whole` (`vibra-interp`) | `54ccf7cf`: a refused buffer answers `unsupported` where memory exhaustion is expected; the run test does not build, `Completion` has no memory-exhaustion form (E0599) | `5a418a81` |
| F7, the environment is copied before the gate | question, answered by 06 "Environment" | none | — | — |
| F8, nothing presents a target's authority before a run | question for the owner | none | — | — |
| F9, the guide's first probe row contradicts the feasibility record | stale guide row | none | — | — |
| F10, the launch check is textual | defect, test gap, fixed in the scan | `run_command::the_launch_scanner_sees_both_spellings_and_the_system_driver`; `run_command::only_caller_of_the_system_launch` (`vibra-cli`) | `e3dd41e8`: does not build: the scan has no way to report an imported variant or `SystemDriver::new` (E0425, E0422) | `8f5c6063` |
| F11, the hard-link and rename limits | accepted limit | none | — | — |

The F1 tests broke the process-exit scan of `vibra-host`, which read the
attribute of their test module as crate code; `b47d32de` moves that
attribute inside the module.

## The rules the fixes follow

- **F1.** A standard input is a regular file only when the operating
  system says its handle is a disk file; a pipe and a console have no
  known size, so a read reserves its `max` and an empty read is the end of
  input.
- **F2.** 04 "Startup order", the failed cell of `@host.fs.directory`: a
  directory on a remote volume, or on one that cannot report the real
  names and link counts of its entries, is a failed acquisition. The root
  is refused when its device reports `FILE_REMOTE_DEVICE`, when the device
  cannot be read, or when the name query of the root fails.
- **F3.** 03 "Filesystem policies", driver rule 2: an error decided below
  a directory is returned only after the real names of the directories
  already opened are checked. A walk that fails reports the real names it
  opened; the driver then asks the gate with them and the rest of the
  request's names, and a refusal is a completed `denied`. The world driver
  is unchanged: its names are exact, so the names it opened are the
  requested names that step 3 already checked.
- **F4.** 06 "Entry outcome": a release that the operating system reports
  as failed does not fail the run. The directory is closed explicitly;
  the binding stays `released`, the result is unchanged, and the failure
  is one line on the command's standard error in both formats.
- **F5.** 06 "Standard input": when the last hold of the standard input is
  released, a regular file is positioned back by the bytes the adapter
  kept, which is just after the bytes delivered.
- **F6.** 06 "Memory": the driver reports memory exhaustion, never an error
  case, and the call appends no event. The broker charges what the adapter
  moved, as it knows it, and the run ends as memory exhausted with every
  leaf released. A host-built value enters the accounted footprint whole.
- **F10.** The scan of the source tree refuses a `use` of a variant of
  `Launch` anywhere, a construction of `Launch::System` outside `vibra run`
  and the two attack suites, and a `SystemDriver::new` outside the launch
  and the system driver's own tests.

## Accepted limits

- **F5, a pipe or a console.** "A pipe or a console cannot take back the
  bytes the adapter kept: a later reader of it does not see them. This is
  an accepted limit of the platform, not a guarantee." (06 "Standard
  input")
- **F11.** A file permitted by name can be a hard link to a file
  elsewhere, and it is read; a link created by another process after the
  link count was read is within the hard-link limit, since the program
  cannot create links. The grant follows the directory object across
  renames.

## Other items of the step

- **Opaque events.** An event with no JSON projection, or a binding left
  `acquired`, keeps its form: `@command.trap`, status `unknown`, the empty
  payload, no added diagnostic, and in the human format one line on
  standard error (05 "Streams").
- **Admission by path.** The entry's row is compared with the ceiling by
  `EntityRef`. A startup step's interface is a host interface, which
  carries only its owner's path and no identity, so its root is compared
  by path; no `EntityRef` is at hand there.
- **The dependency record.** The field `alias` of `@sourceGraph.v1`'s
  dependency record is `name`; no corpus case or schema held it.

## Validation

All exit 0 on Windows. The corpus: 1209 passed, 0 failed, 0 unknown, 0
unavailable. The attack suites of steps 13, 31 and 33 pass, among them
`ATK-run-unc-root`, and the four rows of the dual run agree. One line is
`NOT RUN`:

```text
NOT RUN: directory target, other platforms (this is Windows)
```
