# M2 step 34 — review of startup, the run, the command and the drivers

Reviewed revision `6569aa5a` (`M2 step 33: dual run and attacks`) on
`m2/34-review-drivers`, cut from `v0.0.2/m2-ops`. The review follows
[the step guide](../steps/34-review-drivers.md). Its goal is to read or write
a file the policy does not permit, end a run without its release, or make the
command's output lie.

**There is one blocker: F1.** On Windows a process's standard input that is
a pipe is taken for a regular file whose size is the number of bytes the
pipe holds at that moment. A read made while the writer is still producing
completes with the empty text, which 06 reserves for the end of input. The
run then reports `@command.ok` and records an end of input that did not
happen; the same mechanism turns valid UTF-8 into `invalidText`. No probe
read or wrote a file outside its policy, and every probed ending released
its bindings.

## Platform and validation

Windows 11 Home, build 26300, NTFS on `C:`, Developer Mode on, 8.3 names
generated, per-directory case sensitivity available without elevation, no
elevation. Linux and macOS were not run (excluded by the guide).

The guide's validation ran on the unchanged tree:

```powershell
cargo test --locked --offline -p vibra-fs-scope -p vibra-host -p vibra-workspace -p vibra-cli --all-features -- --nocapture
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
```

Both exit 0. The first reports, per crate: `vibra-cli` 7, 0 and 104
passed; `vibra-fs-scope` 4 and 55 passed with 1 ignored; `vibra-host` 5 and
323 passed; `vibra-workspace` 13 and 134 passed. The second ends with
`conformance total: 1209 passed, 0 failed, 0 unknown, 0 unavailable`.

Probe sources are in the [appendix](#probe-sources). Each Rust probe was
copied into a crate's `tests/it`, registered in its `main.rs`, run with
`cargo test --locked --offline -p <crate> --test it probe34 -- --nocapture`,
and removed; each PowerShell probe drove the debug `vibra.exe` built from
the same tree.

## Probes

32 probes: 20 hold, 7 fail, 5 are `NOT RUN`.

| # | Probe and source | Expected | Observed | Verdict |
| --- | --- | --- | --- | --- |
| 1 | Root through a link, `probe34.rs` `root_*` | A junction or directory link as the `@fixedPath` is refused; a link among the root's ancestors follows the path rules (feasibility record) | Junction root: `@startup @source @failed Some(("@grant.acquisitionFailed", "the directory cannot be held: deniedReparse"))`. Directory-symlink root: the same. Root below a junction parent: `result @command.ok code Some(0) stdout "ok:inside"` | holds (F9 on the guide's wording) |
| 2 | Root on a UNC volume, `probe34.rs` `root_unc`, `probe34_scope.rs` | `@grant.acquisitionFailed` (04: "a directory on which the driver cannot enforce the leaf's policy") | `root-unc: @startup @source @acquired None` then `@host fs.readText … variant: @other`. Adapter: `open_root … Ok("held")`, `open_read a.txt: Err(Io(… "NTSTATUS 0xc000000d"))`, `root normalized-name with 65540 bytes: 0xc000000d`, with 65536 bytes `0x00000000` | fails, F2 |
| 3 | Root on a `subst` drive, `probe34.rs` `root_subst` | Path rules; a local volume | `result @command.ok code Some(0) stdout "ok:inside"` | holds |
| 4 | Root on a mapped network drive (`net use`) | `@grant.acquisitionFailed` | Not run: mapping a drive changes the machine's state outside the worktree. Read instead: a mapped drive reaches the same redirector as probe 2, so `open_root` takes the same path (`windows.rs:172-199`) | NOT RUN |
| 5 | Mount point as a component, `probe34.rs` `mount_point_component` | `denied` | A junction to `C:\` and one to `\\?\Volume{49784326-…}\` inside the root: `stdout "denied\|denied"`; neither read reached a file | holds |
| 6 | Mount point registered with the mount manager (`mountvol`) | `denied` | Not run: `mountvol` needs elevation. Read instead: such a mount is a reparse point with the mount-point tag, refused by `open_checked` (`windows.rs:443-445`) like probe 5 | NOT RUN |
| 7 | Alternate data streams, `probe34.rs` `alternate_streams` | `RelativePath` refuses every spelling | `a.txt:secret`, `a.txt:secret:$DATA`, `a.txt::$DATA`, `sub:secret`, `sub:secret/b.txt`, `sub/b.txt:secret` give `noPath`; a fullwidth colon (U+FF1A) is an ordinary name, `notFound`; `a.txt` reads: `stdout "noPath\|noPath\|noPath\|noPath\|noPath\|noPath\|notFound\|ok:main"` | holds |
| 8 | Case-sensitive directory, `probe34.rs` `case_sensitive_*` | Each name opens its own file; an exact policy is not satisfied by another case | Any-name policy: `stdout "ok:lower\|ok:upper\|notFound"` (`a.txt`, `A.TXT`, `a.TXT`). Exact `["cs" "a.txt"]`: `stdout "ok:lower\|denied\|denied"`, the denials before the driver (one host event) | holds |
| 9 | Normalization, `probe34.rs` `normalization_*` | Scalars compared, no folding | Any-name: `stdout "ok:composed\|ok:decomposed\|notFound\|ok:d"`. Exact NFC policy: `stdout "ok:composed\|denied\|denied\|denied"` | holds |
| 10 | Long path, `probe34.rs` `long_path` | Opens or a typed error, never another file | A 376-character path reads; a missing one is `notFound`; a 300-character name is `invalidPath`: `stdout "ok:deep\|notFound\|invalidPath"` | holds |
| 11 | File opened elsewhere without sharing, `probe34.rs` `exclusive_*` | A typed `FsError`, not a trap | Read: `variant: @other`; replace: `variant: @other`; afterwards the file still reads `Ok("held")` | holds |
| 12 | Write interrupted by a full volume | A completed `other`, the bytes charged | Not run: a small volume (VHD) or a quota needs elevation. Read instead: a failure of `replace_contents` or of the write after `create_new` answers `WRITE_UNCERTAIN` (`fs.rs:284`, `fs.rs:305`); the broker charges the reservation for an uncertain measure (`broker.rs:731-744`) | NOT RUN |
| 13 | Create race, `probe34.rs` `create_race` | `denied`; the other content kept | 300 names, a racer creating in reverse: `marks 300 tally {"denied->racer": 197, "ok->program": 103} broken []` | holds |
| 14 | Startup with a failing release | The binding `released`, the result unchanged, the failure in the tool's log | Not run: no handle of a held resource can be made to fail its close from outside the process, and the driver has no fault hook for it. Read instead: `SystemDriver::release` (`system.rs:406-408`) drops the held value; a close error is discarded and logged nowhere | NOT RUN, F4 |
| 15 | Exit paths of the run | Each releases | Baseline: `test run::tests::atk_release_on_trap ... ok`, `test run::tests::atk_release_on_exhaustion ... ok`, `test startup::release_after_a_failed_startup ... ok`, `test startup::atk_trap_during_startup ... ok`, `test startup::release_on_drop ... ok`. Read: `Prepared::run` releases once after `enter`; every early return of `run_target` is before `start` or holds `Started`, whose `Drop` releases; the release profile unwinds a panic, so `Drop` runs | holds |
| 16 | JSON on every ending, `probe34_cli.rs` | One schema-valid line per ending | 15 rows, each `lines 1 valid`: `ok` exit 0; option-like arguments exit 0; `programFailed` exit 6 `applicationCode 7`; Git dependency exit 4 `@tool.unavailable`; selector not an atom, unknown target, library only, library selected exit 2; no project exit 1 `@project.notFound`; unreadable project exit 1; required leaf refused exit 3 `@grant.requiredUnavailable`; acquisition failed exit 3; `V2-RUNTIME-run-exhausted` exit 3 `reached ["memory"]` `applicationCode null`; static error exit 1; hello exit 0 | holds |
| 17 | Arguments that look like options, `probe34_cli.rs` | The program receives every token after `--` | `vibra --format json run -- --format json --workspace x -- -h`: `stdout String("--format\|json\|--workspace\|x\|--\|-h\|")` | holds |
| 18 | The fake world's blind spots | Written down as the dual run's domain | See [Dual-run domain](#dual-run-domain) | holds |
| 19 | Existence under an alias, `probe34.rs` `alias_case_prefix`, `alias_short_prefix` | A name whose real directory the policy does not permit tells nothing | Policy prefix `["pub"]`, real directory `PUB`: `stdout "denied\|notFound\|denied"` for an existing file, a missing one, and a case alias. Short name `SECRET~1`: `stdout "denied\|notFound"` | fails, F3 |
| 20 | Text windows on a pipe, no bound, `stdin-probe.ps1` | The input delivered exactly, whatever window splits a scalar | 32 runs (max 4 to 7, 0 to 7 leading bytes): every run delivered the input byte for byte and ended at the empty read. For three leading bytes, max 4: `aaa \| 𝄞 \| € \| éz`; max 7: `aaa𝄞 \| €éz` | holds |
| 21 | Text windows on a pipe, bound equal to the input, `stdin-probe.ps1`, `pipe-timing.ps1` | 06: for a pipe, `limitExceeded` once nothing remains of the bound | 32 runs ended at the empty read with code 0. Trace: `read 4u64 -> ok "𝄞"`, `ok "€"`, `ok "éz"`, `ok ""` with `inputBytes: 10` | fails, F1 |
| 22 | Bound one byte short, `stdin-probe.ps1` | The longest whole-scalar prefix within the bound, then a failure | `[aaa][𝄞][€é]` (12 of 13 bytes), then a non-zero code | holds |
| 23 | Two runs sharing one standard input, `stdin-probe.ps1` | 06: a read "consumes exactly their bytes" | Regular file `abc€rest`: the first run (one read, max 4) printed `[abc]`; the second run printed nothing and failed, since the shared position is past the first byte of `€` | fails, F5 |
| 24 | Slow pipe, `pipe-timing.ps1` | Every byte written before the pipe closes is read | `ab`, a 1.5 s pause, `cd`: `slow-pipe json: exit 0 appCode 0 stdout [ab] late=written reads=2`; human format: `stdout [ab]` and the late write failed ("the pipe is being closed") | fails, F1 |
| 25 | Scalar split across two pipe writes, `pipe-timing.ps1` | Valid UTF-8 decodes | `x` and `0xC3`, a pause, `0xA9` and `y`: 3 of 3 trials `exit 6 appCode 34`; the read: `variant: @invalidText` | fails, F1 |
| 26 | What std reports of a pipe, `probe34.rs` `probe34_pipe_metadata` | — (root cause of F1) | `empty: metadata Ok("ok") is_file Some(true) len Some(0) position Ok(0)`; `after 5 bytes written: … len Some(5)`; `after 2 bytes read: … len Some(3)` | fails, F1 |
| 27 | Local dependencies, `deps-probe.ps1` D1–D14b | Authority stays beneath the workspace root; no link is a way out | Junction or directory link to outside: `@module.pathEscape`; `../outside`, absolute path: `@module.pathEscape`; junction inside, short name, case alias: `@command.ok`; `lib/jout/../money` resolves lexically to `lib/money`; the root itself: `@project.dependencyCycle`; a linked `project.vibon`: `@project.ioError`; a source root that is a junction: `@project.invalidTargetRoot`; a module file linked outside: `@module.pathEscape`; hard-linked marker and module: accepted; undeclared unit, the root target, `@tests` from a dependency: `@module.unknownPath` | holds |
| 28 | Global unit names, `deps-probe.ps1` U1–U6 | One path, one unit | Two `money` libraries: `@project.duplicateName`; one directory through a case alias, a junction and a short name: one unit, `@command.ok`; a key equal to the root target: `@project.duplicateName`; `tests` and `std` in a dependency's file: `@project.reservedUnitName`; an unselected library named like the root target: accepted | holds |
| 29 | Startup order across runs | One order, by spelling | Two runs of the same tree: `same startup: True`, `same trace: True`; `@out` before `@src` although the file lists `@src` first | holds |
| 30 | Test hooks in shipped builds | `testing` only in vibra-host's own dev graph | `cargo tree --locked --offline -p vibra-cli -e normal,build -i vibra-host -f '{p} {f}'`: `vibra-host v0.0.1 (…)` with no feature; the same for `vibra-conformance` | holds |
| 31 | Who constructs `Launch::System` | Only `vibra run` outside the attack and dual-run tests | `test run_command::only_caller_of_the_system_launch ... ok`; the one non-test construction is `crates/vibra-cli/src/run.rs` | holds, F10 |
| 32 | Console standard input | Decoded whole scalars | Not run: this session has no interactive console. Read instead: `TerminalInput` reads through std's console reader (`streams.rs:199-205`) and reports no size, so a console takes the unknown-size path F1 should also give a pipe | NOT RUN |

## Specification coverage

Every MUST of the four sections, and their other normative sentences, with
the case or test that shows it.

| Rule | Evidence |
| --- | --- |
| 03 Target admission: an entry row outside `effects` is rejected | `V2-EFFECT-entry-outside-ceiling`, `V2-EFFECT-outside-ceiling-*` (13 cases), `V2-EFFECT-entry-ceiling-equal` |
| 03: a startup step's row outside `effects` is rejected | `V2-EFFECT-startup-row-outside-ceiling`, host `atk_admit_startup_read_outside_ceiling`, `admission_order` |
| 03: every grant leaf has a recipe | `V2-GRANT-binding-missing`, `-nested-missing`, `-optional-missing`, `-plain-missing`, `-complete` |
| 03: a provider that cannot issue the type | `V2-GRANT-binding-provider-wrong-type`, `-provider-unknown` |
| 03: a cyclic `after` | `V2-GRANT-binding-cycle`, `-after-unknown`, `-after-repeated` |
| 03: a read without naming its leaf | `V2-GRANT-binding-after-lacks-source`, `-source-missing`, `-variable-not-in-names` |
| 03: presentable in plain terms from the project file alone | Gap: no command shows it before a run; the startup record shows the projections after one. F8 |
| 03 Filesystem rule 1: walk from the held directory, no link, no mount point | fs-scope `atk_fs_junction`, `atk_fs_file_symlink`, `atk_fs_swapped_junction`; workspace `atk_run_symlink_denied`, `atk_run_junction_denied`, `atk_run_swapped_component`; probes 1, 5 |
| Rule 2: the real name; an alias MUST NOT satisfy a test the real name fails | fs-scope `atk_fs_case_alias`, `atk_fs_short_name`; host `atk_fs_alias_name`, `gate_real_name_fails`, `atk_reserved_names_in_any_case_through_the_broker`; workspace `atk_run_alias_denied`; probes 8, 9. Probe 19 shows the order of the check leaks existence: F3 |
| Rule 3: the access on the object opened and checked | fs-scope `atk_fs_root_renamed`, `atk_fs_create_parent_swapped`; workspace `atk_run_root_replaced` |
| Rule 4: no truncation or creation before authorization; exclusive creation | fs-scope `atk_fs_replace_no_truncate`, `atk_fs_create_over_alias`; host `create_exclusive`, `create_over_an_alias`; workspace `atk_run_denied_leaves_file`, `atk_run_create_denied_leaves_directory`; probe 13 |
| Rule 5: no intermediate handle between operations | Read: `walk` (`windows.rs:272-288`) keeps one handle at a time and returns it to one operation; `ScopedParent` lives for one creation. Workspace `atk_run_swapped_component` |
| Rule 6: `unsupported` before acting where the platform cannot guarantee | fs-scope `open_root_is_unsupported` (other platforms); host `gate_real_name_fails` with the `names_unsupported` hook. A UNC root is not refused: F2 |
| 03: the two limits MUST appear in what a user is shown | Stated below in the chapter's words; no approval surface exists in this release: F8 |
| 03 Hard links: a replace MUST be `denied` when the link count is above one | host `atk_fs_hard_link_replace`, `gate_hard_link`, `atk_gate_replace_without_link_count`; workspace `atk_run_hard_link_replace_denied`; a read is allowed: `atk_run_hard_link_read_allowed`, `read_of_a_hard_link` |
| 04 Startup order: smallest spelling among ready leaves, readiness recomputed | host `the_order_is_by_spelling`, `the_order_follows_after`, `the_order_recomputes_readiness_after_each_leaf`, `order_by_spelling`, `v2_grant_startup_order_permuted`; probe 29 |
| 04: refused optional is `Option.none()`; refused required is `@grant.requiredUnavailable` | host `refused_optional`, `refused_required`; `V2-RUNTIME-run-optional-absent`, `V2-RUNTIME-run-required-refused`; probe 16 |
| 04: a failure stops startup for both kinds and never becomes an absence | host `v2_grant_startup_failed_optional`, `atk_startup_failure_not_none`, `a_refusal_of_a_never_refusing_issuer_is_a_failure`, `arguments_that_are_not_unicode_are_a_failure` |
| 04: the directory row (refused and failed) | host `each_directory_refusal`, `acquisition_refusals`, `acquisition_failure`, `a_root_that_is_a_junction_fails`, `a_variable_that_is_not_unicode_is_a_refusal`. "Cannot enforce the leaf's policy" is not met for a UNC root: F2 |
| 04: the stream row ("the process has no such stream") | Read: `process_stdin` and `process_output` (`streams.rs:155-190`) answer `Missing`, which `acquire` refuses. No test closes a standard stream |
| 04: a driver the platform does not support is `@tool.unavailable` at admission | CLI `directory_target_other_platforms` (prints `NOT RUN` on Windows) |
| 04: the startup records and events use the three outcomes | host `an_acquired_leaf_is_the_startup_event_of_the_example`, `a_refused_optional_leaf_has_no_grant_and_no_failure`, `a_failed_leaf_records_its_failure` |
| 06 Entry outcome: release on every ending, in reverse order | workspace `atk_release_on_trap`, `atk_release_on_exhaustion`; host `v2_grant_release_order`, `release_after_a_failed_startup`, `release_on_drop`, `atk_trap_during_startup`; probe 15 |
| 06: release runs no code and appends no `@host` event | host `release_appends_nothing` |
| 06: release MUST NOT be presented as rollback | Read: the startup record says `released` and nothing else; no message calls it a rollback |
| 06: release first makes the grant and its narrowings unusable | host `release_invalidates_first`, `atk_released_grant`, `atk_released_derived_grant` |
| 06: a failed release goes to the tool's log | Gap: F4 |
| 06: the entry MUST NOT have a bare `requires` | `V2-PROJECT-entry-bare-requires`, `V2-RUNTIME-run-entry-requires-else` |
| 06: a non-zero code is `@command.programFailed`, exit 6, no trap | CLI `non_zero_code`, workspace `v2_runtime_run_nonzero`, `v2_runtime_run_code_255`; probe 16 |
| 06: exhaustion, startup failure and tool failure MUST NOT appear as an application code | CLI `exhaustion` (`applicationCode` null); probe 16 (`applicationCode null` on every non-return ending) |
| 06: the four exit statuses of a built executable on Windows | Not in this release: no executable is built until v0.0.3 |

## The two limits a user is shown

In the words of 03 "Filesystem policies":

- **Authority follows the opened directory, not its path.** If the directory
  is renamed, the grant still refers to it; if another directory takes its
  old path, the grant does not refer to that one.
- **Hard links.** A permitted file name can be a hard link to a file that
  also has a name elsewhere. Reading such a file is allowed: its content is
  an entry of the scoped directory, and a policy does not establish where
  data came from. Replacing it MUST be refused with `denied` when the
  platform reports a link count above one, because the change would be
  visible outside the scope. A read-only grant still cannot write.

The first is shown by `atk_fs_root_renamed` and `atk_run_root_replaced`;
the second by `atk_run_hard_link_read_allowed` and
`atk_run_hard_link_replace_denied`.

## The `unsafe` blocks of `windows.rs`

**`NtQueryDirectoryFile` in `ScopedListing::entries` (`windows.rs:358`).**
The handle is the listing's own, opened by `walk_to` with the `LIST` profile:
`FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE`, all three share
modes, `FILE_OPEN`, and the `FILE_OPEN_REPARSE_POINT |
FILE_SYNCHRONOUS_IO_NONALERT` options every open adds. Synchronous I/O lets
the call pass no event, APC routine or context and return only when the
buffer is written. The buffer is a `Vec<u64>` of exactly
`LIST_BUFFER_BYTES`, eight-byte aligned, more than `FILE_NAMES_INFORMATION`
needs; `FileNamesInformation` returns names only, so nothing about an entry
is opened or followed. `restart` is true on the first call. The bytes read
are capped at `Information` and parsed with checked offsets
(`read_names`), so a malformed record is an error, not an out-of-bounds
read. The handle is owned by `self` and closed when `entries` returns.
Listing grants no other access: the entries' names are all it yields.

**`NtCreateFile` in `nt_open` (`windows.rs:503`).** This is the one open
below the root. `RootDirectory` is the borrowed parent handle and
`ObjectName` a single component, so the name is resolved relative to the
held directory object, never as a path from a drive. `OBJ_DONT_REPARSE`
makes the object manager fail with `STATUS_REPARSE_POINT_ENCOUNTERED`
rather than follow a reparse point met during the name's resolution, and
`FILE_OPEN_REPARSE_POINT` opens a reparse point named as the last component
as itself, which `open_checked` then refuses from its attributes. The
dispositions are `FILE_OPEN` (never `FILE_OVERWRITE` or `FILE_SUPERSEDE`, so
no open truncates) and, for `CREATE`, `FILE_CREATE`, which fails on any
existing entry under any alias. The access masks are the least each
operation needs: `TRAVERSE` and `STAT` read no data, `READ` adds
`FILE_READ_DATA`, `REPLACE` and `CREATE` add `FILE_WRITE_DATA` and no
`DELETE`. The write profiles share only `FILE_SHARE_READ`, so no other
writer or deleter can hold the file while it is written. The name buffer
`wide` outlives the call and its byte length is checked to fit a `u16`
(a longer component is `DeniedName`). A zero length reopens the parent
itself, which `walk_to` uses for the root.

**`OwnedHandle::from_raw_handle` (`windows.rs:523`).** It runs only after
`NtCreateFile` returned success, so `handle` is a new handle nothing else
owns; the `OwnedHandle` closes it exactly once. Every handle below the root
has this one owner: `walk` replaces the previous directory's handle, which
closes it, so at most one intermediate handle exists and none survives the
operation (rule 5).

**`NtQueryInformationFile` with `FileAttributeTagInformation` in
`attributes` (`windows.rs:536`).** The buffer is a
`FILE_ATTRIBUTE_TAG_INFO` of exactly `TAG_INFO_SIZE` bytes, the structure
the class writes. The query reads the attributes of the object the handle
refers to, which, because of `FILE_OPEN_REPARSE_POINT`, is the reparse point
itself when there is one; `FILE_ATTRIBUTE_REPARSE_POINT` is therefore seen
for junctions, symbolic links and mount points alike, and the open is
refused. The kind comes from the same answer, so the check and the object
cannot differ.

**`NtQueryInformationFile` with `FileStandardInformation` in `standard`
(`windows.rs:565`).** The buffer is a `FILE_STANDARD_INFORMATION` of exactly
its size. `EndOfFile` gives the size the gate reserves (a negative value is
an error) and `NumberOfLinks` the link count the gate compares for a
replace. Both are read from the opened handle, the object that is then read
or written (rule 3).

**`NtQueryInformationFile` with `FileNormalizedNameInformation` in
`real_name` (`windows.rs:595`).** The buffer is `NAME_BUFFER_WORDS` `u32`
words, four-byte aligned like `FILE_NAME_INFORMATION`, `NAME_BUFFER_BYTES` =
65,540 bytes. The normalized name is the long name in the case stored on
disk, so the last component is the real name rule 2 asks for, and an alias
(short name or other case) never reaches the gate as the requested
spelling. An information class the volume does not implement is
`Unsupported`. The byte length is taken from the answer and the copy is
bounded by the buffer. Over SMB this class fails with
`STATUS_INVALID_PARAMETER` for a 65,540-byte buffer and succeeds for 65,536
bytes or less (probe 2); `open_root` ignores that failure for the root, and
every open below it then fails: F2.

## Paths from an operating-system error to a `Completion`

Every path was read from the adapter to `SystemDriver::call`. The broker
accepts `NotSubmitted` only for `denied`, `limitExceeded` and `unsupported`
(`known_not_submitted`), and a completed case for every other.

| Source | Answer | Has anything happened? |
| --- | --- | --- |
| fs: open, walk or name check fails (`map_error`, `fs.rs:326`) | completed `denied`, `notFound`, `invalidPath`, `invalidText`, `permissionDenied`, `other`, or `NotSubmitted unsupported` for `Unsupported` | Nothing written: the opens never truncate, and `Unsupported` comes only from the name query, before any data access |
| fs: read fails after the open (`fs.rs:179`) | completed, `READ_UNCERTAIN` | Bytes may have been read: charged as uncertain |
| fs: `reserved.input < size` (`fs.rs:164`) | `NotSubmitted unsupported` | Nothing read; unreachable, the gate reserves the size |
| fs: replace or write after create fails | completed, `WRITE_UNCERTAIN` | Bytes may have been written: charged as uncertain |
| stdin: window cannot be allocated (`streams.rs:442`) | `NotSubmitted unsupported` | Bytes taken are kept in the adapter, so the program's input is unchanged (F5 for other processes) |
| stdin: source fails (`streams.rs:441`) | completed `other`, progress = bytes taken | Charged |
| stdin: whole scalars below one (`streams.rs:473`) | `NotSubmitted limitExceeded` | Window kept; unreachable for a text read |
| stdout, stderr: write fails | completed; written bytes known on Unix, uncertain on Windows and on a console | Charged as reported |
| env: short reservation | `NotSubmitted unsupported` | Nothing read; unreachable |
| random: allocation fails (`random.rs:30`) | `NotSubmitted unsupported` | Generator not called |
| random: generator fails | completed `unavailable`, input uncertain | Charged |
| clock: value does not fit | completed `unavailable` | Nothing external |

No path turns "may have happened" into "not submitted". The allocation
paths are F6.

## Dual-run domain

The dual run compares the fake world with the system drivers only where
both can express the case. It cannot compare, so passing it is not evidence
of confinement for:

- reparse points of every kind (junctions, symbolic links, mount points),
  8.3 short names and case aliases of a real directory (the attack rows
  cover these, not the comparison);
- hard links and link counts, sharing violations, operating-system
  permission errors, a full volume, and a failing close;
- concurrency: a racing creator, a swapped component, a renamed root;
- network and redirected volumes (probe 2);
- standard input and environment: `materialize` refuses a world with
  `stdin` or `env` ([dual-run.md](dual-run.md), "Domain"), so the pipe, the
  console and timing (F1, F5) are outside it;
- clock and random results, the trace's driver identities, and the message
  of a startup failure, which `compare` leaves out.

## Findings

Ordered by severity.

**F1 — defect, blocker. A Windows pipe is read as a regular file.**
`crates/vibra-host/src/system/streams.rs:162` sets `regular` from
`file.metadata().is_ok_and(|metadata| metadata.is_file())`; on Windows std
reports an anonymous pipe as a file whose `len` is the bytes it holds now
and whose position is 0 (probe 26). `FileInput::remaining`
(`streams.rs:215-222`) then gives the gate a known size, so the broker takes
the known-size branch (`broker.rs:667`). A read made while the pipe is
momentarily empty reserves zero and completes with the empty text, which 06
defines as the end of input ("a completed text read is empty only at the end
of input"): the program stops early, the rest of the input is never read,
and the run reports `@command.ok` with an audit trace that records an end of
input that did not happen (probe 24). A read made while the pipe holds part
of a scalar takes that part as the whole input and answers `invalidText` on
valid UTF-8 (probe 25). At a bound, the unknown-size rule's `limitExceeded`
never comes (probe 21); the bound itself still holds, since the counters
refuse any reservation beyond it. Any `producer | vibra run` whose producer
is slower than the program's first read meets this. The CLI test
`stdin_passes_through`, which runs `V2-RUNTIME-run-stdin` on a pipe, passes
only while its input reaches the pipe before the child's first read.
Smallest fix: on Windows, decide
`regular` by `GetFileType(handle) == FILE_TYPE_DISK` as well as
`is_file()`. Tests: a host test that `FileInput` over `std::io::pipe()`
reports `remaining() == None`; a CLI test that writes the first byte of a
two-byte scalar, waits, writes the rest, and expects the whole text and
exit 0.

**F2 — defect. A UNC root is acquired and then nothing below it opens.**
`crates/vibra-fs-scope/src/windows.rs:194` fails `open_root` only for
`ScopeError::Unsupported`; the root's name query over SMB fails with
`STATUS_INVALID_PARAMETER` because the buffer is 65,540 bytes
(`windows.rs:144-145`), and that failure is ignored. The run acquires the
leaf and every read answers `other` (probe 2), where 04 asks for
`@grant.acquisitionFailed`. It fails closed: only the root itself can be
inspected or opened for listing, and a file created directly in it; nothing
outside the policy is reachable. The buffer must not be fixed alone: an SMB
server resolves its own junctions, which the client does not see as reparse
points (the protocol's behaviour, not probed here), so a working UNC root
would weaken rule 1. Smallest fix: refuse a remote volume in
`open_root` (`FileFsDeviceInformation` with `FILE_REMOTE_DEVICE`, or
`FileRemoteProtocolInformation` answering) as `Unsupported`, and treat any
failure of the root's name query as `Unsupported`. Test: an fs-scope test
that `open_root(\\localhost\C$\…)` is `Unsupported`, and a workspace row
that a UNC `@fixedPath` gives `@grant.acquisitionFailed`.

**F3 — question. Whether an entry exists is visible under a directory the
policy does not permit.** With the policy prefix `["pub"]` and the real
directory `PUB`, an existing `pub/x.txt` is `denied` and a missing
`pub/y.txt` is `notFound`; the short name `SECRET~1` behaves the same
(probe 19). The adapter walks every component before the gate sees the real
names, so a missing leaf ends the walk with `NotFound` (`fs.rs:150-153`,
`windows.rs:292-308`). No content is read. 03 forbids an alias satisfying a
name test; it does not say whether the answer may differ by existence. If
it should not: before answering `notFound`, ask the gate about the real
names of the directories already opened and answer `denied` when they are
not permitted; test with the two rows of probe 19.

**F4 — defect. A failed release is logged nowhere.** 06: "the failure goes
to the tool's log". `SystemDriver::release` (`system.rs:406-408`) removes
the held value, and dropping a `File` or `OwnedHandle` discards the close
error; the `Driver::release` signature has no way to report one, and the
run has no tool log to receive it (probe 14). Smallest fix: close
explicitly in `release`, return the error to the broker, and write it to
the command's diagnostics log without changing the binding status. Test: a
host test with a scripted driver whose release fails, asserting `released`
and one log line.

**F5 — question. Bytes the adapter keeps are lost to a later reader of the
same standard input.** A text window that ends inside a scalar keeps the
partial bytes inside the adapter (`streams.rs:476-477`), and the peek of
`ends_here` keeps one more (`streams.rs:392-409`). The process-level
position has already moved past them, so a second process sharing the
handle starts mid-scalar (probe 23). 06 says the read "consumes exactly
their bytes". If that means the shared position: on release, seek a regular
file back by the kept bytes; a pipe cannot give them back, which would then
be an accepted limit to state.

**F6 — question. An operating-system allocation failure inside a driver is
an error case.** 06: "Memory exhaustion stays a host event and never becomes
an error case." The budget check is before the call, but when the system
then refuses the allocation, random answers `unsupported`
(`random.rs:30`), stdin `unsupported` (`streams.rs:442`), and a file read
`other` through `read_to_end`'s `OutOfMemory` (`vibra-fs-scope/src/lib.rs:179`).
The question is whether that is memory exhaustion.

**F7 — question. The environment is copied before the gate.** 06: "A
driver MUST NOT read, allocate, or send an unbounded payload first and check
afterwards." `lookup` (`env.rs:24-27`) collects `std::env::vars_os()`, and
the value exists before the reservation. The operating system bounds the
environment block, so it is not unbounded; the question is whether the
order is acceptable.

**F8 — question. Nothing presents a target's authority before a run.**
03 asks that what a target may do be "presentable in plain terms from the
project file alone" and that the two limits "appear in what a user is shown
when approving a target". No command of this release shows either; the
startup record shows the grant projections after a run. The limits are
stated above in the chapter's words.

**F9 — question. The guide's first probe row contradicts the feasibility
record.** The row expects a root that is itself a link to be "the opened
directory", recorded as "acquired through path rules". The
[feasibility record](../../m0/file-scope-feasibility.md) and `open_root`
refuse a root that is itself a reparse point and apply the path rules only
to its ancestors (probe 1). The guide row is stale.

**F10 — defect, test gap. The launch check is textual.** `system_launchers`
(`crates/vibra-cli/tests/it/run_command.rs:560-588`) matches the text
`Launch::System`; `use … Launch::{System, …}` followed by `System { … }`
constructs one without the text, and `SystemDriver::new` is not checked
here. Today the only non-test caller of either is `vibra run` (probe 31).
Smallest fix: forbid a `use` of `Launch` variants in the same scan and add
`SystemDriver::new` to it. Test: the scanner's unit test with both
spellings.

**F11 — accepted limit. The two limits above.** A file permitted by name
can be a hard link to a file elsewhere and is read; a link created by
another process after the link count was read is within the hard-link
limit, since the program cannot create links. The grant follows the
directory object across renames.

## Probe sources

### `probe34.rs`

The run-level filesystem probes (1–3, 5, 7–11, 13, 19) and the pipe
metadata probe (26), compiled in `crates/vibra-workspace/tests/it`.

```rust
//! Gate-34 scratch probes. Not part of the repository: copied into
//! `crates/vibra-workspace/tests/it/`, run, and removed.
#![cfg(windows)]
#![allow(clippy::all, clippy::pedantic, unused)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use vibra_host::trace::Event;
use vibra_syntax::canonical_string;
use vibra_workspace::MemoryBudget;
use vibra_workspace::run::{Launch, RunOutcome, run_target};

use crate::support::{TempDir, capture, link_directory};

const STDOUT: &str =
    "{provider: @host.io.stdout resource: {kind: @processStream} after: []}";

fn project(effects: &str, grants: &str) -> String {
    format!(
        "{{format: @project.v2 package: {{name: \"probe\" version: \"0.1.0\"}} \
         targets: [{{name: @app kind: @bin root: \"src/app\" entry: @app.main.main \
         effects: [{effects}] grants: [{grants}]}}] dependencies: [:]}}"
    )
}

fn path_text(path: &Path) -> String {
    canonical_string(&path.display().to_string())
}

/// A directory recipe of `leaf` at the literal path `path` with `expression`.
fn dir_recipe(leaf: &str, path: &str, expression: &str) -> String {
    format!(
        "{leaf}: {{provider: @host.fs.directory resource: {{kind: @fixedPath path: {path}}} \
         policy: {{format: @scopePolicy.v1 family: @filesystem expression: {expression}}} \
         after: []}}"
    )
}

fn file_rule(root: &str, prefix: &str, actions: &str, names: &str) -> String {
    format!(
        "{{kind: @fileRule root: {root} prefix: [{prefix}] depth: @tree actions: [{actions}] \
         names: {names}}}"
    )
}

const ANY: &str = "{kind: @anyName}";

fn list(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| canonical_string(name))
        .collect::<Vec<_>>()
        .join(" ")
}

const ONE_READ: &str = "proc one
  takes {source: DirReadText names: List(Str)}
  returns mark: Str
  performs {DirReadText}
  do
    let Result.ok(value: path) be RelativePath(names) else
      return \"noPath\"
    endlet
    match await DirReadText.read(source path)
      case Result.ok(value: text)
        return Str.concat(\"ok:\" text)
      case Result.err(error: FsError.denied())
        return \"denied\"
      case Result.err(error: FsError.notFound())
        return \"notFound\"
      case Result.err(error: FsError.other())
        return \"other\"
      case Result.err(error: FsError.permissionDenied())
        return \"permissionDenied\"
      case Result.err(error: FsError.invalidPath())
        return \"invalidPath\"
      case Result.err(error: FsError.notAFile())
        return \"notAFile\"
      case Result.err(error: FsError.notADirectory())
        return \"notADirectory\"
      case Result.err(error: FsError.unsupported())
        return \"unsupported\"
      case Result.err(error: FsError.limitExceeded())
        return \"limitExceeded\"
      case _
        return \"error\"
    endmatch
endproc
";

const ONE_WRITE: &str = "proc one
  takes {dest: DirWriteText names: List(Str) text: Str}
  returns mark: Str
  performs {DirWriteText}
  do
    let Result.ok(value: path) be RelativePath(names) else
      return \"noPath\"
    endlet
    match await DirWriteText.write(dest path text)
      case Result.ok(value: _)
        return \"ok\"
      case Result.err(error: FsError.denied())
        return \"denied\"
      case Result.err(error: FsError.notFound())
        return \"notFound\"
      case Result.err(error: FsError.other())
        return \"other\"
      case Result.err(error: FsError.permissionDenied())
        return \"permissionDenied\"
      case Result.err(error: FsError.invalidPath())
        return \"invalidPath\"
      case _
        return \"error\"
    endmatch
endproc
";

/// A program that runs one read (or write) per entry of `paths` and writes the
/// marks, joined by `|`, to `out`.
fn program(paths: &[Vec<&str>], write: Option<&str>) -> String {
    let (iface, one, call) = match write {
        None => ("DirReadText", ONE_READ, "source"),
        Some(_) => ("DirWriteText", ONE_WRITE, "dest"),
    };
    let mut body = String::new();
    for (index, names) in paths.iter().enumerate() {
        let tail = match write {
            None => String::new(),
            Some(text) => format!(" {}", canonical_string(text)),
        };
        body.push_str(&format!(
            "    let m{index} be await one({call} [{}]{tail}) endlet\n",
            list(names)
        ));
    }
    body.push_str("    let s0 be m0 endlet\n");
    for index in 1..paths.len() {
        body.push_str(&format!(
            "    let s{index} be Str.concat(s{} Str.concat(\"|\" m{index})) endlet\n",
            index - 1
        ));
    }
    let last = paths.len() - 1;
    format!(
        "use @std.fs.{iface} enduse

use @std.fs.FsError enduse

use @std.fs.RelativePath enduse

use @std.io.StdoutText enduse

use @std.process.ExitCode enduse

{one}
proc main
  takes {{out: StdoutText {call}: {iface}}}
  returns code: ExitCode
  performs {{{iface} StdoutText}}
  do
{body}    let _ be await StdoutText.write(out s{last}) endlet
    return ExitCode(0u8)
endproc
"
    )
}

fn read_project(root: &str, prefix: &str, names: &str) -> String {
    project(
        "@std.fs.DirReadText @std.io.StdoutText",
        &format!(
            "@out: {STDOUT} {}",
            dir_recipe(
                "@source",
                root,
                &file_rule("@source", prefix, "@read", names)
            )
        ),
    )
}

fn write_project(root: &str, actions: &str) -> String {
    project(
        "@std.fs.DirWriteText @std.io.StdoutText",
        &format!(
            "@out: {STDOUT} {}",
            dir_recipe("@dest", root, &file_rule("@dest", "", actions, ANY))
        ),
    )
}

/// Writes the tree and runs it under the system launch, capturing.
fn run(tree: &Path, document: &str, main: &str) -> RunOutcome {
    fs::create_dir_all(tree.join("src/app")).expect("tree");
    fs::write(tree.join("project.vibon"), document).expect("project");
    fs::write(tree.join("src/app/main.vib"), main).expect("main");
    let snapshot = capture(tree).unwrap_or_else(|d| panic!("capture: {d:?}"));
    run_target(
        &snapshot,
        None,
        Launch::System {
            arguments: Vec::new(),
            capture: true,
        },
        MemoryBudget::DEFAULT,
    )
}

fn report(probe: &str, outcome: &RunOutcome) {
    println!(
        "PROBE {probe}: result {} code {:?} stdout {:?}",
        outcome.result.as_atom(),
        outcome.application_code,
        String::from_utf8_lossy(&outcome.stdout)
    );
    if let Some(failure) = &outcome.startup.failure {
        println!(
            "PROBE {probe}:   startup failure {} {}",
            failure.binding,
            failure.code.atom()
        );
    }
    for event in &outcome.trace.events {
        match event {
            Event::Host(host) => println!(
                "PROBE {probe}:   @host {} [{}] -> {}",
                host.op.symbol(),
                host.args.join(" "),
                host.result
            ),
            Event::Startup(startup) => println!(
                "PROBE {probe}:   @startup {} {} {:?}",
                startup.binding,
                startup.status.atom(),
                startup
                    .failure
                    .as_ref()
                    .map(|failure| (failure.code.atom(), failure.message.clone()))
            ),
            Event::Opaque(_) => println!("PROBE {probe}:   opaque"),
        }
    }
    for binding in &outcome.startup.bindings {
        println!(
            "PROBE {probe}:   binding {} {}",
            binding.name,
            binding.status.spelling()
        );
    }
    for diagnostic in &outcome.diagnostics {
        println!(
            "PROBE {probe}:   diagnostic {:?} {}",
            diagnostic.code(),
            diagnostic.message()
        );
    }
}

fn cmd(line: &str) -> std::process::Output {
    use std::os::windows::process::CommandExt as _;
    std::process::Command::new("cmd")
        .raw_arg(format!("/C {line}"))
        .output()
        .expect("cmd")
}

fn plain(path: &Path) -> String {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").map_or(text.clone(), str::to_owned)
}

/// A world directory `in` holding `files` (relative path, contents).
fn world(dir: &TempDir, files: &[(&str, &str)]) -> PathBuf {
    let root = fs::canonicalize(dir.path()).expect("canonical").join("in");
    fs::create_dir_all(&root).expect("in");
    for (path, contents) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(path, contents).expect("file");
    }
    root
}

fn reader() -> String {
    program(&[vec!["a.txt"]], None)
}

// --- M1: the root through a link -------------------------------------------

#[test]
fn probe_root_through_links() {
    let dir = TempDir::new("p34-rootlink");
    let root = world(&dir, &[("a.txt", "inside")]);
    let junction = dir.join("jroot");
    link_directory(&root, &junction);
    let outcome = run(
        &dir.join("t1"),
        &read_project(&path_text(&junction), "", ANY),
        &reader(),
    );
    report("root-is-junction", &outcome);

    let symlink = dir.join("sroot");
    match std::os::windows::fs::symlink_dir(&root, &symlink) {
        Ok(()) => {
            let outcome = run(
                &dir.join("t2"),
                &read_project(&path_text(&symlink), "", ANY),
                &reader(),
            );
            report("root-is-dir-symlink", &outcome);
        }
        Err(error) => println!("PROBE root-is-dir-symlink: NOT RUN {error}"),
    }

    // A junction as a parent component of the root's path.
    let parent = dir.join("jparent");
    link_directory(&fs::canonicalize(dir.path()).unwrap(), &parent);
    let outcome = run(
        &dir.join("t3"),
        &read_project(&path_text(&parent.join("in")), "", ANY),
        &reader(),
    );
    report("root-under-junction", &outcome);
    // Renaming the junction's target path after startup cannot be observed
    // here; the root is the opened directory.
}

// --- M2: the root on a mapped or UNC volume --------------------------------

#[test]
fn probe_root_unc_and_subst() {
    let dir = TempDir::new("p34-unc");
    let root = world(&dir, &[("a.txt", "inside")]);
    let text = plain(&root);
    let (drive, rest) = text.split_at(2);
    let unc = format!(r"\\localhost\{}$\{}", &drive[..1], rest.trim_start_matches('\\'));
    let reachable = fs::metadata(&unc).map(|m| m.is_dir());
    println!("PROBE root-unc: {unc} metadata {reachable:?}");
    let outcome = run(
        &dir.join("t1"),
        &read_project(&canonical_string(&unc), "", ANY),
        &reader(),
    );
    report("root-unc", &outcome);

    let subst = cmd(&format!("subst Q: \"{text}\""));
    println!(
        "PROBE root-subst: subst {:?} {}",
        subst.status.code(),
        String::from_utf8_lossy(&subst.stdout).trim()
    );
    if subst.status.success() {
        let outcome = run(
            &dir.join("t2"),
            &read_project(&canonical_string("Q:\\"), "", ANY),
            &reader(),
        );
        report("root-subst", &outcome);
        let _ = cmd("subst Q: /D");
    }
}

// --- M3: a mount point inside the root -------------------------------------

#[test]
fn probe_mount_point_inside_root() {
    let dir = TempDir::new("p34-mount");
    let root = world(&dir, &[("a.txt", "inside")]);
    // A junction to the volume's root: the same reparse tag a volume mount
    // point carries (IO_REPARSE_TAG_MOUNT_POINT).
    link_directory(Path::new("C:\\"), &root.join("vol"));
    let volume = cmd("mountvol C:\\ /L");
    let guid = String::from_utf8_lossy(&volume.stdout).trim().to_owned();
    println!("PROBE mount: mountvol C:\\ /L -> {:?} {guid}", volume.status.code());
    let made = if guid.starts_with(r"\\?\Volume{") {
        let out = cmd(&format!("mklink /J \"{}\" {guid}", plain(&root.join("guid"))));
        println!(
            "PROBE mount: mklink /J guid -> {:?} {} {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        out.status.success()
    } else {
        false
    };
    let mut paths = vec![vec!["vol", "Windows", "win.ini"]];
    if made {
        paths.push(vec!["guid", "Windows", "win.ini"]);
    }
    let outcome = run(
        &dir.join("t"),
        &read_project(&path_text(&root), "", ANY),
        &program(&paths, None),
    );
    report("mount-point-component", &outcome);
}

// --- M4: alternate data streams --------------------------------------------

#[test]
fn probe_alternate_streams() {
    let dir = TempDir::new("p34-ads");
    let root = world(&dir, &[("a.txt", "main"), ("sub/b.txt", "b")]);
    fs::write(root.join("a.txt:secret"), "STREAM-SECRET").expect("a stream");
    fs::write(root.join("sub:secret"), "DIR-STREAM").expect("a dir stream");
    println!(
        "PROBE ads: stream written: {:?}",
        fs::read_to_string(root.join("a.txt:secret"))
    );
    let paths = vec![
        vec!["a.txt:secret"],
        vec!["a.txt:secret:$DATA"],
        vec!["a.txt::$DATA"],
        vec!["sub:secret"],
        vec!["sub:secret", "b.txt"],
        vec!["sub", "b.txt:secret"],
        vec!["a.txt\u{ff1a}secret"],
        vec!["a.txt"],
    ];
    let outcome = run(
        &dir.join("t"),
        &read_project(&path_text(&root), "", ANY),
        &program(&paths, None),
    );
    report("alternate-streams", &outcome);
}

// --- M5: a case-sensitive directory ----------------------------------------

#[test]
fn probe_case_sensitive_directory() {
    let dir = TempDir::new("p34-case");
    let root = world(&dir, &[]);
    let cs = root.join("cs");
    fs::create_dir(&cs).expect("cs");
    let out = std::process::Command::new("fsutil.exe")
        .args(["file", "setCaseSensitiveInfo"])
        .arg(plain(&cs))
        .arg("enable")
        .output()
        .expect("fsutil");
    println!(
        "PROBE case: fsutil {:?} {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).trim()
    );
    fs::write(cs.join("a.txt"), "lower").expect("a.txt");
    fs::write(cs.join("A.TXT"), "upper").expect("A.TXT");
    let paths = vec![vec!["cs", "a.txt"], vec!["cs", "A.TXT"], vec!["cs", "a.TXT"]];
    let outcome = run(
        &dir.join("t1"),
        &read_project(&path_text(&root), "", ANY),
        &program(&paths, None),
    );
    report("case-sensitive-any", &outcome);
    let outcome = run(
        &dir.join("t2"),
        &read_project(
            &path_text(&root),
            "",
            "{kind: @exact names: [\"cs\" \"a.txt\"]}",
        ),
        &program(&paths, None),
    );
    report("case-sensitive-exact-lower", &outcome);
}

// --- M6: names that differ by normalization ---------------------------------

#[test]
fn probe_normalization() {
    let nfc = "caf\u{e9}.txt";
    let nfd = "cafe\u{301}.txt";
    let dir = TempDir::new("p34-norm");
    let root = world(&dir, &[(nfc, "composed"), (nfd, "decomposed"), ("only\u{301}.txt", "d")]);
    let paths = vec![vec![nfc], vec![nfd], vec!["onl\u{fd}.txt"], vec!["only\u{301}.txt"]];
    let outcome = run(
        &dir.join("t1"),
        &read_project(&path_text(&root), "", ANY),
        &program(&paths, None),
    );
    report("normalization-any", &outcome);
    let outcome = run(
        &dir.join("t2"),
        &read_project(
            &path_text(&root),
            "",
            &format!("{{kind: @exact names: [{}]}}", canonical_string(nfc)),
        ),
        &program(&paths, None),
    );
    report("normalization-exact-nfc", &outcome);
}

// --- M7: a long path --------------------------------------------------------

#[test]
fn probe_long_path() {
    let dir = TempDir::new("p34-long");
    let name = "d".repeat(60);
    let names = vec![name.as_str(); 5];
    let mut relative = PathBuf::new();
    for name in &names {
        relative.push(name);
    }
    relative.push("deep.txt");
    let root = world(&dir, &[]);
    let full = root.join(&relative);
    fs::create_dir_all(full.parent().unwrap()).expect("deep dirs");
    fs::write(&full, "deep").expect("deep file");
    println!("PROBE long: full path length {}", plain(&full).chars().count());
    let mut path = names.clone();
    path.push("deep.txt");
    let mut missing = names.clone();
    missing.push("none.txt");
    let long_name = "n".repeat(300);
    let outcome = run(
        &dir.join("t"),
        &read_project(&path_text(&root), "", ANY),
        &program(&[path, missing, vec![long_name.as_str()]], None),
    );
    report("long-path", &outcome);
}

// --- M8: a file opened elsewhere without sharing ----------------------------

#[test]
fn probe_exclusive_open() {
    use std::os::windows::fs::OpenOptionsExt as _;
    let dir = TempDir::new("p34-excl");
    let root = world(&dir, &[("a.txt", "held")]);
    let held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(root.join("a.txt"))
        .expect("an exclusive handle");
    let outcome = run(
        &dir.join("t1"),
        &read_project(&path_text(&root), "", ANY),
        &reader(),
    );
    report("exclusive-read", &outcome);
    let outcome = run(
        &dir.join("t2"),
        &write_project(&path_text(&root), "@replace @create"),
        &program(&[vec!["a.txt"]], Some("new")),
    );
    report("exclusive-replace", &outcome);
    drop(held);
    println!(
        "PROBE exclusive: after {:?}",
        fs::read_to_string(root.join("a.txt"))
    );
}

// --- M10: the create race -----------------------------------------------------

#[test]
fn probe_create_race() {
    let dir = TempDir::new("p34-race");
    let root = world(&dir, &[]);
    let count = 300;
    let names = (0..count).map(|i| format!("n{i}.txt")).collect::<Vec<_>>();
    let paths = names.iter().map(|n| vec![n.as_str()]).collect::<Vec<_>>();
    let main = program(&paths, Some("program"));
    let document = write_project(&path_text(&root), "@create");
    let stop = AtomicBool::new(false);
    let racer_root = root.clone();
    let outcome = std::thread::scope(|scope| {
        let racer = scope.spawn(|| {
            let mut won = 0;
            // Start when the program's first create landed, then create from
            // the other end, so the two meet inside the program's sequence.
            while !racer_root.join("n0.txt").exists() {
                if stop.load(Ordering::Relaxed) {
                    return won;
                }
                std::thread::yield_now();
            }
            for name in names.iter().rev() {
                let path = racer_root.join(name);
                if let Ok(mut file) =
                    fs::OpenOptions::new().write(true).create_new(true).open(&path)
                {
                    use std::io::Write as _;
                    let _ = file.write_all(b"racer");
                    won += 1;
                }
            }
            won
        });
        let outcome = run(&dir.join("t"), &document, &main);
        stop.store(true, Ordering::Relaxed);
        let won = racer.join().unwrap();
        println!("PROBE create-race: racer created {won}");
        outcome
    });
    let marks = String::from_utf8_lossy(&outcome.stdout).into_owned();
    let marks = marks.split('|').collect::<Vec<_>>();
    let mut tally = std::collections::BTreeMap::new();
    let mut broken = Vec::new();
    for (name, mark) in names.iter().zip(&marks) {
        let content = fs::read_to_string(root.join(name)).unwrap_or_default();
        *tally.entry(format!("{mark}->{content}")).or_insert(0) += 1;
        let fine = matches!((*mark, content.as_str()), ("ok", "program") | ("denied", "racer"));
        if !fine {
            broken.push(format!("{name}: {mark} {content:?}"));
        }
    }
    println!(
        "PROBE create-race: result {} marks {} tally {tally:?} broken {broken:?}",
        outcome.result.as_atom(),
        marks.len()
    );
}

// --- B5: existence under an alias -------------------------------------------

#[test]
fn probe_existence_under_alias() {
    let dir = TempDir::new("p34-alias");
    let root = world(
        &dir,
        &[
            ("PUB/x.txt", "secret-x"),
            ("secretdirectory/x.txt", "secret-long"),
            ("pub2/x.txt", "ok"),
        ],
    );
    // Case alias of the prefix: the policy permits `pub`, the directory is `PUB`.
    let paths = vec![vec!["pub", "x.txt"], vec!["pub", "y.txt"], vec!["pub", "x.TXT"]];
    let outcome = run(
        &dir.join("t1"),
        &read_project(&path_text(&root), "\"pub\"", ANY),
        &program(&paths, None),
    );
    report("alias-case-prefix", &outcome);
    // Short name of the prefix.
    let short = cmd(&format!(
        "for %I in (\"{}\") do @echo %~snxI",
        plain(&root.join("secretdirectory"))
    ));
    let short = String::from_utf8_lossy(&short.stdout).trim().to_owned();
    println!("PROBE alias-short: short name {short:?}");
    if !short.is_empty() && short != "secretdirectory" {
        let paths = vec![
            vec![short.as_str(), "x.txt"],
            vec![short.as_str(), "y.txt"],
        ];
        let outcome = run(
            &dir.join("t2"),
            &read_project(&path_text(&root), &canonical_string(&short), ANY),
            &program(&paths, None),
        );
        report("alias-short-prefix", &outcome);
    }
}
/// Gate-34 scratch: what std reports of an anonymous pipe on Windows, the
/// way `FileInput` asks it.
#[test]
fn probe34_pipe_metadata() {
    use std::io::{Seek as _, Write as _};
    let (reader, mut writer) = std::io::pipe().expect("a pipe");
    let mut file = fs::File::from(std::os::windows::io::OwnedHandle::from(reader));
    let show = |label: &str, file: &mut fs::File| {
        let metadata = file.metadata();
        println!(
            "PROBE pipe {label}: metadata {:?} is_file {:?} len {:?} position {:?}",
            metadata.as_ref().map(|_| "ok").map_err(|e| e.to_string()),
            metadata.as_ref().map(|m| m.is_file()).ok(),
            metadata.as_ref().map(|m| m.len()).ok(),
            file.stream_position().map_err(|e| e.to_string()),
        );
    };
    show("empty", &mut file);
    writer.write_all(b"abcde").unwrap();
    show("after 5 bytes written", &mut file);
    let mut two = [0u8; 2];
    std::io::Read::read_exact(&mut file, &mut two).unwrap();
    show("after 2 bytes read", &mut file);
}
```

### `probe34_scope.rs`

The adapter on a UNC root (probe 2), compiled in
`crates/vibra-fs-scope/tests/it`.

```rust
//! Gate-34 scratch probe of the adapter on a UNC root. Not part of the
//! repository.
#![allow(clippy::all, clippy::pedantic, unused)]

use std::fs;

use vibra_fs_scope::ScopedDir;

use crate::Sandbox;

#[test]
fn probe34_unc_root() {
    let sandbox = Sandbox::new("p34-unc");
    let inside = fs::canonicalize(sandbox.inside()).unwrap();
    fs::write(inside.join("a.txt"), "inside").unwrap();
    fs::create_dir(inside.join("sub")).unwrap();
    let text = inside.display().to_string();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned();
    let (drive, rest) = text.split_at(2);
    let unc = format!(r"\\localhost\{}$\{}", &drive[..1], rest.trim_start_matches('\\'));
    let root = ScopedDir::open_root(std::path::Path::new(&unc));
    println!("PROBE unc open_root {unc}: {:?}", root.as_ref().map(|_| "held"));
    let Ok(root) = root else { return };
    println!("PROBE unc open_read a.txt: {:?}", root.open_read(&["a.txt"]).map(|f| f.real_names().to_vec()));
    println!("PROBE unc stat a.txt: {:?}", root.stat(&["a.txt"]));
    println!("PROBE unc stat root: {:?}", root.stat(&[]));
    println!("PROBE unc list root: {:?}", root.list(&[]).map(|l| l.real_names().to_vec()));
    println!("PROBE unc list sub: {:?}", root.list(&["sub"]).map(|l| l.real_names().to_vec()));
    println!("PROBE unc open_parent sub: {:?}", root.open_parent(&["sub"]).map(|p| p.real_names().to_vec()));
    println!("PROBE unc open_parent root then create: {:?}", root.open_parent(&[]).and_then(|p| p.create_new("n.txt")).map(|f| f.real_names().to_vec()));
    println!("PROBE unc n.txt exists after: {}", inside.join("n.txt").exists());
    println!("PROBE unc create existing a.txt: {:?}", root.open_parent(&[]).and_then(|p| p.create_new("a.txt")).map(|f| f.real_names().to_vec()));
    diagnose(&unc);
}

fn diagnose(unc: &str) {
    use std::os::windows::fs::OpenOptionsExt as _;
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows_sys::Wdk::Storage::FileSystem::{
        FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
        FileNormalizedNameInformation, FileAttributeTagInformation, NtCreateFile,
        NtQueryInformationFile,
    };
    use windows_sys::Win32::Foundation::{HANDLE, OBJ_DONT_REPARSE, UNICODE_STRING};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
        FILE_READ_DATA, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, SYNCHRONIZE,
    };
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;
    let root = fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES | SYNCHRONIZE | 0x20 | 0x1)
        .share_mode(7)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(unc)
        .unwrap();
    let query_sized = |handle: HANDLE, class, bytes: u32| {
        let mut buffer = vec![0u32; (bytes as usize + 3) / 4];
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: scratch probe; valid handle, buffer and status block.
        unsafe {
            NtQueryInformationFile(handle, &mut status, buffer.as_mut_ptr().cast(), bytes, class)
        }
    };
    let query = |handle: HANDLE, class| query_sized(handle, class, 16384);
    println!(
        "PROBE unc diag: root normalized-name status {:#010x}",
        query(root.as_raw_handle(), FileNormalizedNameInformation).cast_unsigned()
    );
    for bytes in [16384u32, 65532, 65536, 65540] { println!("PROBE unc diag: root normalized-name with {bytes} bytes: {:#010x}", query_sized(root.as_raw_handle(), FileNormalizedNameInformation, bytes).cast_unsigned()); }
    for (label, attributes) in [("OBJ_DONT_REPARSE", OBJ_DONT_REPARSE), ("no flag", 0)] {
        let wide = "a.txt".encode_utf16().collect::<Vec<u16>>();
        let name = UNICODE_STRING {
            Length: 10,
            MaximumLength: 10,
            Buffer: wide.as_ptr().cast_mut(),
        };
        let object = OBJECT_ATTRIBUTES {
            Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: root.as_raw_handle(),
            ObjectName: &name,
            Attributes: attributes,
            SecurityDescriptor: std::ptr::null(),
            SecurityQualityOfService: std::ptr::null(),
        };
        let mut handle: HANDLE = std::ptr::null_mut();
        let mut status_block = IO_STATUS_BLOCK::default();
        // SAFETY: scratch probe; every pointer is a live local.
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                &object,
                &mut status_block,
                std::ptr::null(),
                FILE_ATTRIBUTE_NORMAL,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                FILE_OPEN,
                FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                std::ptr::null(),
                0,
            )
        };
        let tag = if status >= 0 { query(handle, FileAttributeTagInformation) } else { -1 };
        let normalized = if status >= 0 { query(handle, FileNormalizedNameInformation) } else { -1 };
        println!(
            "PROBE unc diag: NtCreateFile a.txt ({label}) {:#010x}; tag query {:#010x}; normalized-name {:#010x}",
            status.cast_unsigned(),
            tag.cast_unsigned(),
            normalized.cast_unsigned()
        );
    }
}
```

### `probe34_cli.rs`

The endings of `vibra run` in JSON (probes 16, 17), compiled in
`crates/vibra-cli/tests/it`.

```rust
//! Gate-34 scratch probes of `vibra run` endings in JSON. Not part of the
//! repository: copied into `crates/vibra-cli/tests/it/`, run, removed.
#![allow(clippy::all, clippy::pedantic, unused)]

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::support::TempDir;

const VIBRA: &str = env!("CARGO_BIN_EXE_vibra");

fn vibra(directory: &Path, arguments: &[&str]) -> (i32, Vec<u8>, Vec<u8>) {
    let mut child = Command::new(VIBRA)
        .args(arguments)
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary starts");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("the binary ends");
    (output.status.code().unwrap_or(-1), output.stdout, output.stderr)
}

/// Runs `vibra --format json run ARGS` and prints what the row needs.
fn row(label: &str, directory: &Path, arguments: &[&str]) -> Value {
    let mut line = vec!["--format", "json", "run"];
    line.extend_from_slice(arguments);
    let (code, stdout, stderr) = vibra(directory, &line);
    let text = String::from_utf8(stdout).expect("UTF-8");
    let lines = text.lines().count();
    let document: Value = serde_json::from_str(text.trim_end()).expect("one JSON value");
    let valid = vibra_schema::validate_command_result(&document)
        .map(|()| "valid".to_owned())
        .unwrap_or_else(|violation| format!("INVALID: {violation}"));
    let payload = &document["payload"];
    let codes = document["diagnostics"]
        .as_array()
        .map(|all| {
            all.iter()
                .map(|d| d["code"].as_str().unwrap_or("?").to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    println!(
        "PROBE cli {label}: exit {code} lines {lines} {valid} result {} status {} limits {} \
         codes {codes:?} applicationCode {} startup {} failure {} bindings {} trap {} \
         payloadKeys {} notice {} stdout {:?} revision {}",
        document["result"],
        document["status"],
        document["limits"],
        payload["applicationCode"],
        payload["startup"]["status"],
        payload["startup"]["failure"]["code"],
        payload["startup"]["bindings"]
            .as_array()
            .map(|b| b
                .iter()
                .map(|b| format!("{}={}", b["name"], b["status"]))
                .collect::<Vec<_>>()
                .join(","))
            .unwrap_or_default(),
        payload["trap"],
        payload.as_object().map(|o| o.len()).unwrap_or(0),
        document["notice"],
        payload["stdout"],
        document["workspaceRevision"],
    );
    if !stderr.is_empty() {
        println!("PROBE cli {label}: stderr {:?}", String::from_utf8_lossy(&stderr));
    }
    document
}

const ECHO: &str = "use @std.env.EnvReadArgs enduse\n\nuse @std.io.StdoutText enduse\n\n\
    use @std.process.ExitCode enduse\n\n\
    fn lines\n  takes {items: List(Str)}\n  returns text: Str\n  do\n    \
    return Iter.fold(\n      items\n      \"\"\n      fn\n        \
    takes {text: Str item: Str}\n        returns joined: Str\n        do\n          \
    return Str.concat(Str.concat(text item) \"|\")\n      endfn\n    )\nendfn\n\n\
    proc main\n  takes {args: EnvReadArgs out: StdoutText}\n  returns code: ExitCode\n  \
    performs {EnvReadArgs StdoutText}\n  do\n    \
    let Result.ok(value: items) be await EnvReadArgs.read(args) else\n      \
    return ExitCode(1u8)\n    endlet\n    \
    let _ be await StdoutText.write(out lines(items)) endlet\n    \
    return ExitCode(CODEu8)\nendproc\n";

fn echo_project(label: &str, code: u8, extra_deps: &str) -> TempDir {
    let document = format!(
        "{{format: @project.v2 package: {{name: \"p\" version: \"0.1.0\"}} targets: [\
         {{name: @app kind: @bin root: \"src/app\" entry: @app.main.main \
         effects: [@std.env.EnvReadArgs @std.io.StdoutText] grants: [\
         @args: {{provider: @host.env.args resource: {{kind: @processArguments}} after: []}} \
         @out: {{provider: @host.io.stdout resource: {{kind: @processStream}} after: []}}]}}] \
         dependencies: {extra_deps}}}"
    );
    TempDir::project(
        label,
        &document,
        &[("src/app/main.vib", &ECHO.replace("CODE", &code.to_string()))],
    )
}

fn dir_project(label: &str, path: &str) -> TempDir {
    let document = format!(
        "{{format: @project.v2 package: {{name: \"p\" version: \"0.1.0\"}} targets: [\
         {{name: @app kind: @bin root: \"src/app\" entry: @app.main.main \
         effects: [@std.fs.DirReadText] grants: [\
         @notes: {{provider: @host.fs.directory resource: {{kind: @fixedPath path: {path}}} \
         policy: {{format: @scopePolicy.v1 family: @filesystem expression: {{kind: @fileRule \
         root: @notes prefix: [] depth: @tree actions: [@read] names: {{kind: @anyName}}}}}} \
         after: []}}]}}] dependencies: [:]}}"
    );
    let main = "use @std.fs.DirReadText enduse\n\nuse @std.process.ExitCode enduse\n\n\
        proc main\n  takes {notes: DirReadText}\n  returns code: ExitCode\n  \
        performs {DirReadText}\n  do\n    return ExitCode(0u8)\nendproc\n";
    TempDir::project(label, &document, &[("src/app/main.vib", main)])
}

fn cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases/RUNTIME")
}

fn copy_tree(from: &Path, to: &Path) {
    let mut pending = vec![from.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(from).unwrap();
            if path.is_dir() {
                fs::create_dir_all(to.join(relative)).unwrap();
                pending.push(path);
            } else {
                fs::copy(&path, to.join(relative)).unwrap();
            }
        }
    }
}

#[test]
fn probe34_cli_endings() {
    let ok = echo_project("p34-ok", 0, "[:]");
    row("ok", ok.path(), &[]);
    row("option-like arguments", ok.path(), &["--", "--format", "json", "--workspace", "x", "--", "-h"]);
    let failed = echo_project("p34-failed", 7, "[:]");
    row("programFailed", failed.path(), &[]);
    let git = echo_project(
        "p34-git",
        0,
        "[\"money\": {kind: @git git: \"https://example.com/money.git\" rev: \"0123456789abcdef0123456789abcdef01234567\"}]",
    );
    row("git dependency (unavailable)", git.path(), &[]);
    row("selector not an atom", ok.path(), &["app"]);
    row("unknown target", ok.path(), &["@nope"]);
    let library = TempDir::project(
        "p34-lib",
        "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} targets: [{name: @app kind: @lib root: \"src/app\"}] dependencies: [:]}",
        &[("src/app/x.vib", "fn one\n  takes {value: I32}\n  returns result: I32\n  do\n    return value\nendfn\n")],
    );
    row("library only", library.path(), &[]);
    row("library selected", library.path(), &["@app"]);
    let empty = TempDir::new("p34-empty");
    row("no project", empty.path(), &[]);
    let broken = TempDir::project("p34-broken", "{format: @project.v2", &[]);
    row("unreadable project", broken.path(), &[]);
    let missing = dir_project("p34-missing", "\"does-not-exist\"");
    row("required leaf refused", missing.path(), &[]);
    let target = TempDir::new("p34-target");
    fs::create_dir(target.path().join("real")).unwrap();
    let junction = target.path().join("j");
    let plain = |path: &Path| {
        let text = path.display().to_string();
        text.strip_prefix(r"\\?\").map_or(text.clone(), str::to_owned)
    };
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(plain(&junction))
        .arg(plain(&target.path().join("real")))
        .output()
        .unwrap();
    assert!(status.status.success());
    let acquisition = dir_project(
        "p34-acq",
        &serde_json::to_string(&plain(&junction)).unwrap(),
    );
    row("acquisition failed (root is a junction)", acquisition.path(), &[]);
    for case in ["V2-RUNTIME-run-exhausted", "V2-RUNTIME-run-static-error", "V2-RUNTIME-run-hello"] {
        let tree = TempDir::new(&case.to_lowercase());
        copy_tree(&cases().join(case).join("tree"), tree.path());
        row(case, tree.path(), &[]);
    }
    // Human format of the exhaustion row: stderr only.
    let tree = TempDir::new("p34-exhausted-human");
    copy_tree(&cases().join("V2-RUNTIME-run-exhausted").join("tree"), tree.path());
    let (code, stdout, stderr) = vibra(tree.path(), &["run"]);
    println!(
        "PROBE cli exhausted human: exit {code} stdout {:?} stderr {:?}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
}
```

### `stdin-probe.ps1`

Text windows on a pipe and shared standard input (probes 20–23).

```powershell
# Gate-34 scratch probe: the one-byte peek of a text read on a pipe.
# Runs `vibra run` with its standard input a pipe written from here, for
# windows (max) 4..7 and inputs whose 4-, 3- and 2-byte scalars sit at every
# offset, with and without an inputBytes bound equal to the input's length.
param([string]$Vibra, [string]$Work)

$ErrorActionPreference = 'Stop'

function New-Workspace([string]$dir, [int]$max, [string]$limits) {
    New-Item -ItemType Directory -Force "$dir\src\app" | Out-Null
    $project = "{format: @project.v2 package: {name: `"p`" version: `"0.1.0`"} targets: [{name: @app kind: @bin root: `"src/app`" entry: @app.main.main effects: [@std.io.StdinText @std.io.StdoutText] grants: [@src: {provider: @host.io.stdin resource: {kind: @processStream} $limits after: []} @out: {provider: @host.io.stdout resource: {kind: @processStream} after: []}]}] dependencies: [:]}"
    $main = @"
use @std.io.IoError enduse

use @std.io.StdinText enduse

use @std.io.StdoutText enduse

use @std.process.ExitCode enduse

proc pump
  takes {src: StdinText out: StdoutText left: U32}
  returns code: ExitCode
  performs {StdinText StdoutText}
  do
    match left
      case 0u32
        return ExitCode(3u8)
      case _
        match await StdinText.read(src ${max}u64)
          case Result.ok(value: "")
            return ExitCode(0u8)
          case Result.ok(value: text)
            let _ be await StdoutText.write(out Str.concat("[" Str.concat(text "]"))) endlet
            return await pump(src out U32.sub(left 1u32))
          case Result.err(error: IoError.limitExceeded())
            return ExitCode(33u8)
          case Result.err(error: IoError.invalidText())
            return ExitCode(34u8)
          case Result.err(error: IoError.unsupported())
            return ExitCode(35u8)
          case _
            return ExitCode(39u8)
        endmatch
    endmatch
endproc

proc main
  takes {src: StdinText out: StdoutText}
  returns code: ExitCode
  performs {StdinText StdoutText}
  do
    return await pump(src out 1000u32)
endproc
"@
    [IO.File]::WriteAllText("$dir\project.vibon", $project)
    [IO.File]::WriteAllText("$dir\src\app\main.vib", $main.Replace("`r`n", "`n"))
}

function Invoke-Piped([string]$dir, [byte[]]$bytes) {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $Vibra
    $info.Arguments = "--workspace `"$dir`" run"
    $info.UseShellExecute = $false
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($info)
    $process.StandardInput.BaseStream.Write($bytes, 0, $bytes.Length)
    $process.StandardInput.BaseStream.Flush()
    $process.StandardInput.Close()
    $memory = New-Object IO.MemoryStream
    $process.StandardOutput.BaseStream.CopyTo($memory)
    $err = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    [pscustomobject]@{ Code = $process.ExitCode; Out = $memory.ToArray(); Err = $err }
}

$utf8 = New-Object Text.UTF8Encoding($false)
$tail = "$([char]::ConvertFromUtf32(0x1D11E))$([char]0x20AC)$([char]0xE9)z"
$runs = 0; $bad = @()
foreach ($bound in @('none', 'exact')) {
    foreach ($max in 4..7) {
        foreach ($k in 0..7) {
            $text = ('a' * $k) + $tail
            $bytes = $utf8.GetBytes($text)
            $limits = if ($bound -eq 'exact') { "limits: {inputBytes: $($bytes.Length)u64}" } else { '' }
            $dir = Join-Path $Work "w-$bound-$max-$k"
            New-Workspace $dir $max $limits
            $result = Invoke-Piped $dir $bytes
            $out = $utf8.GetString($result.Out)
            $chunks = [regex]::Matches($out, '\[([^\]]*)\]') | ForEach-Object { $_.Groups[1].Value }
            $joined = ($chunks -join '')
            $expected = if ($bound -eq 'exact') { 33 } else { 0 }
            $ok = ($joined -eq $text) -and ($result.Code -eq $expected)
            $runs++
            if (-not $ok) { $bad += "bound=$bound max=$max k=$k code=$($result.Code) out=$out err=$($result.Err.Trim())" }
            if ($k -eq 3) { "PROBE peek bound=$bound max=$max k=$k code=$($result.Code) chunks=$($chunks -join ' | ')" }
        }
    }
}
"PROBE peek: $runs runs, $($bad.Count) differ from the input or the expected code"
$bad | ForEach-Object { "PROBE peek BAD: $_" }

# A bound one byte short of the input: the delivered text is the longest
# whole-scalar prefix within the bound, then limitExceeded.
$text = 'aaa' + $tail
$bytes = $utf8.GetBytes($text)
$dir = Join-Path $Work 'w-short'
New-Workspace $dir 5 "limits: {inputBytes: $($bytes.Length - 1)u64}"
$result = Invoke-Piped $dir $bytes
"PROBE peek short bound: code=$($result.Code) out=$($utf8.GetString($result.Out)) input=$($bytes.Length) bound=$($bytes.Length - 1)"

# Two runs sharing one standard input, a regular file: the first reads one
# window that ends inside a scalar, the second reads what is left.
$first = Join-Path $Work 'w-first'
New-Item -ItemType Directory -Force "$first\src\app" | Out-Null
New-Workspace $first 4 ''
$once = [IO.File]::ReadAllText("$first\src\app\main.vib").Replace('return await pump(src out 1000u32)', 'return await pump(src out 1u32)')
[IO.File]::WriteAllText("$first\src\app\main.vib", $once)
$second = Join-Path $Work 'w-second'
New-Workspace $second 64 ''
$file = Join-Path $Work 'shared-input.txt'
[IO.File]::WriteAllBytes($file, $utf8.GetBytes("abc$([char]0x20AC)rest"))
$batch = Join-Path $Work 'shared.cmd'
[IO.File]::WriteAllText($batch, "@echo off`r`n`"$Vibra`" --workspace `"$first`" run`r`necho.`r`necho first=%errorlevel%`r`n`"$Vibra`" --workspace `"$second`" run`r`necho.`r`necho second=%errorlevel%`r`n")
$shared = cmd /c "`"$batch`" < `"$file`"" 2>&1
"PROBE shared regular file: $($shared -join ' / ')"
$sharedPipe = cmd /c "type `"$file`" | `"$batch`"" 2>&1
"PROBE shared pipe: $($sharedPipe -join ' / ')"
```

### `pipe-timing.ps1`

A slow pipe, a split scalar and the exact bound in JSON (probes 21, 24, 25).

```powershell
# Gate-34 scratch probe: a pipe whose writer is slower than the program.
# Reuses the workspaces of stdin-probe.ps1: w-none-4-0 reads with max 4 and
# no bound; w-exact-4-0 has the bound inputBytes: 10u64.
param([string]$Vibra, [string]$Work)

function Start-Vibra([string]$arguments) {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $Vibra
    $info.Arguments = $arguments
    $info.UseShellExecute = $false
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    [Diagnostics.Process]::Start($info)
}

# Slow pipe: "ab", a pause with the pipe open, then "cd".
foreach ($format in @('', '--format json ')) {
    $p = Start-Vibra "$format--workspace `"$Work\w-none-4-0`" run"
    $stream = $p.StandardInput.BaseStream
    $stream.Write([byte[]](0x61, 0x62), 0, 2); $stream.Flush()
    Start-Sleep -Milliseconds 1500
    try { $stream.Write([byte[]](0x63, 0x64), 0, 2); $stream.Flush(); $late = 'written' }
    catch { $late = "write failed: $($_.Exception.Message)" }
    try { $p.StandardInput.Close() } catch {}
    $out = $p.StandardOutput.ReadToEnd(); $p.WaitForExit()
    if ($format) {
        $j = $out | ConvertFrom-Json
        $reads = ($j.payload.auditTrace | Where-Object { $_.op -eq 'io.stdinText' }).Count
        "PROBE slow-pipe json: exit $($p.ExitCode) appCode $($j.payload.applicationCode) stdout $($j.payload.stdout) late=$late reads=$reads"
    } else {
        "PROBE slow-pipe human: exit $($p.ExitCode) stdout $out late=$late"
    }
}

# Split scalar: "x" and the first byte of "é", a pause, then its second
# byte and "y". The input is valid UTF-8; exit code 34 is invalidText.
foreach ($trial in 1..3) {
    $p = Start-Vibra "--format json --workspace `"$Work\w-none-4-0`" run"
    $stream = $p.StandardInput.BaseStream
    $stream.Write([byte[]](0x78, 0xC3), 0, 2); $stream.Flush()
    Start-Sleep -Milliseconds 1500
    try { $stream.Write([byte[]](0xA9, 0x79), 0, 2); $stream.Flush(); $late = 'written' }
    catch { $late = 'refused' }
    try { $p.StandardInput.Close() } catch {}
    $j = $p.StandardOutput.ReadToEnd() | ConvertFrom-Json; $p.WaitForExit()
    "PROBE split-scalar trial $trial : exit $($p.ExitCode) appCode $($j.payload.applicationCode) stdout $($j.payload.stdout) late=$late"
    if ($trial -eq 1) {
        $j.payload.auditTrace | Where-Object { $_.op -eq 'io.stdinText' } |
            ForEach-Object { "PROBE split-scalar read: $($_.result -replace 'typeInfo: \{.*?\]\}\]\} ', '')" }
    }
}

# Exact bound: the whole input at once, a bound equal to its 10 bytes.
$p = Start-Vibra "--format json --workspace `"$Work\w-exact-4-0`" run"
$bytes = (New-Object Text.UTF8Encoding($false)).GetBytes(
    "$([char]::ConvertFromUtf32(0x1D11E))$([char]0x20AC)$([char]0xE9)z")
$p.StandardInput.BaseStream.Write($bytes, 0, $bytes.Length); $p.StandardInput.Close()
$j = $p.StandardOutput.ReadToEnd() | ConvertFrom-Json; $p.WaitForExit()
"PROBE exact-bound json: exit $($p.ExitCode) appCode $($j.payload.applicationCode) stdout $($j.payload.stdout)"
$j.payload.auditTrace | Where-Object { $_.op -eq 'io.stdinText' } | ForEach-Object {
    if ($_.result -match 'variant: @ok payload: \{value: ("[^"]*")\}') { "PROBE exact-bound read $($_.args) -> ok $($Matches[1])" }
    else { "PROBE exact-bound read $($_.args) -> $($_.result)" }
}
```

### `deps-probe.ps1`

Local dependencies and global unit names (probes 27, 28).

```powershell
# Gate-34 scratch probe: local dependencies as authority, and global unit
# names. Each case is a workspace checked with `vibra --format json check`.
param([string]$Vibra, [string]$Work)
$ErrorActionPreference = 'Stop'

function Write-Text([string]$path, [string]$text) {
    New-Item -ItemType Directory -Force (Split-Path $path -Parent) | Out-Null
    [IO.File]::WriteAllText($path, $text.Replace("`r`n", "`n"))
}

function Deps([string[]]$entries) {
    if ($entries.Count -eq 0) { return '[:]' }
    '[' + (($entries | ForEach-Object { $k, $p = $_ -split '=', 2; "`"$k`": {kind: @path path: `"$p`"}" }) -join ' ') + ']'
}

$one = "fn one`n  takes {value: I32}`n  returns result: I32`n  visibility @public`n  do`n    return value`nendfn`n"

function New-Lib([string]$dir, [string]$name, [string[]]$deps, [string]$module) {
    Write-Text "$dir\project.vibon" "{format: @project.v2 package: {name: `"$name`" version: `"1.0.0`"} targets: [{name: @$name kind: @lib root: `"src/$name`"}] dependencies: $(Deps $deps)}"
    if (-not $module) { $module = $one }
    Write-Text "$dir\src\$name\m.vib" $module
}

function New-Root([string]$dir, [string[]]$deps, [string]$main) {
    Write-Text "$dir\project.vibon" "{format: @project.v2 package: {name: `"app`" version: `"0.1.0`"} targets: [{name: @app kind: @lib root: `"src/app`"}] dependencies: $(Deps $deps)}"
    Write-Text "$dir\src\app\main.vib" $main
}

function Use([string[]]$units) { ($units | ForEach-Object { "use @$_.m as $_ enduse`n" }) -join "`n" }

function Check([string]$label, [string]$dir) {
    $json = & $Vibra --format json --workspace $dir check
    $doc = $json | ConvertFrom-Json
    $codes = ($doc.diagnostics | ForEach-Object { "$($_.code) [$($_.message)]" }) -join '; '
    "PROBE dep $label : $($doc.result) exit=$LASTEXITCODE $codes"
}

function Junction([string]$link, [string]$target) {
    cmd /c "mklink /J `"$link`" `"$target`"" | Out-Null
}

$i = 0
function Fresh { $script:i++; $d = Join-Path $Work "d$script:i"; New-Item -ItemType Directory -Force "$d\w" | Out-Null; New-Item -ItemType Directory -Force "$d\outside" | Out-Null; return $d }

# D1 baseline
$d = Fresh; New-Lib "$d\w\lib\money" 'money' @() ''; New-Root "$d\w" @('money=lib/money') (Use @('money'))
Check 'D1 baseline' "$d\w"

# D2 junction to a directory outside the root
$d = Fresh; New-Lib "$d\outside\money" 'money' @() ''; New-Item -ItemType Directory -Force "$d\w\lib" | Out-Null; Junction "$d\w\lib\j" "$d\outside\money"; New-Root "$d\w" @('money=lib/j') (Use @('money'))
Check 'D2 junction outside' "$d\w"

# D2b directory symbolic link outside
$d = Fresh; New-Lib "$d\outside\money" 'money' @() ''; New-Item -ItemType Directory -Force "$d\w\lib" | Out-Null
try { New-Item -ItemType SymbolicLink -Path "$d\w\lib\s" -Target "$d\outside\money" | Out-Null; New-Root "$d\w" @('money=lib/s') (Use @('money')); Check 'D2b dir symlink outside' "$d\w" } catch { "PROBE dep D2b dir symlink outside : NOT RUN $($_.Exception.Message)" }

# D3 junction inside the root
$d = Fresh; New-Lib "$d\w\vendor\money" 'money' @() ''; New-Item -ItemType Directory -Force "$d\w\lib" | Out-Null; Junction "$d\w\lib\j" "$d\w\vendor\money"; New-Root "$d\w" @('money=lib/j') (Use @('money'))
Check 'D3 junction inside' "$d\w"

# D4 lexical escape
$d = Fresh; New-Lib "$d\outside\money" 'money' @() ''; New-Root "$d\w" @('money=../outside/money') (Use @('money'))
Check 'D4 dotdot escape' "$d\w"

# D4b absolute and drive paths
$d = Fresh; New-Lib "$d\outside\money" 'money' @() ''; New-Root "$d\w" @("money=$($d.Replace('\','/'))/outside/money") (Use @('money'))
Check 'D4b absolute path' "$d\w"

# D5 short name, D6 case alias
$d = Fresh; New-Lib "$d\w\lib\moneylongname" 'money' @() ''
$short = (cmd /c "for %I in (`"$d\w\lib\moneylongname`") do @echo %~snxI").Trim()
New-Root "$d\w" @("money=lib/$short") (Use @('money'))
Check "D5 short name lib/$short" "$d\w"
New-Root "$d\w" @('money=LIB/MONEYLONGNAME') (Use @('money'))
Check 'D6 case alias' "$d\w"

# D7 `..` after a link that points outside
$d = Fresh; New-Lib "$d\w\lib\money" 'money' @() ''; New-Item -ItemType Directory -Force "$d\outside\deep" | Out-Null; New-Lib "$d\outside\money" 'money' @() "fn one`n  takes {value: I32}`n  returns result: I32`n  visibility @public`n  do`n    return I32.add(value 1000i32)`nendfn`n"; Junction "$d\w\lib\jout" "$d\outside\deep"
New-Root "$d\w" @('money=lib/jout/../money') (Use @('money'))
Check 'D7 dotdot after an outside link' "$d\w"

# D8 the root itself
$d = Fresh; New-Root "$d\w" @('app2=.') ''
Check 'D8 path is the root' "$d\w"

# D9 marker is a file symbolic link; D10 marker is a hard link
$d = Fresh; New-Lib "$d\outside\money" 'money' @() ''; New-Item -ItemType Directory -Force "$d\w\lib\money\src\money" | Out-Null; Copy-Item "$d\outside\money\src\money\m.vib" "$d\w\lib\money\src\money\m.vib"
try { New-Item -ItemType SymbolicLink -Path "$d\w\lib\money\project.vibon" -Target "$d\outside\money\project.vibon" | Out-Null; New-Root "$d\w" @('money=lib/money') (Use @('money')); Check 'D9 marker symlink' "$d\w" } catch { "PROBE dep D9 : NOT RUN $($_.Exception.Message)" }
$d = Fresh; New-Lib "$d\outside\money" 'money' @() ''; New-Item -ItemType Directory -Force "$d\w\lib\money\src\money" | Out-Null; Copy-Item "$d\outside\money\src\money\m.vib" "$d\w\lib\money\src\money\m.vib"
New-Item -ItemType HardLink -Path "$d\w\lib\money\project.vibon" -Target "$d\outside\money\project.vibon" | Out-Null; New-Root "$d\w" @('money=lib/money') (Use @('money'))
Check 'D10 marker hard link' "$d\w"

# D11 source root is a junction to outside
$d = Fresh; New-Lib "$d\outside\money" 'money' @() ''; Write-Text "$d\w\lib\money\project.vibon" (Get-Content "$d\outside\money\project.vibon" -Raw); New-Item -ItemType Directory -Force "$d\w\lib\money\src" | Out-Null; Junction "$d\w\lib\money\src\money" "$d\outside\money\src\money"; New-Root "$d\w" @('money=lib/money') (Use @('money'))
Check 'D11 source root junction' "$d\w"

# D12 a module file that is a symbolic link to a file outside
$d = Fresh; New-Lib "$d\outside\money" 'money' @() ''; Write-Text "$d\w\lib\money\project.vibon" (Get-Content "$d\outside\money\project.vibon" -Raw); New-Item -ItemType Directory -Force "$d\w\lib\money\src\money" | Out-Null
try { New-Item -ItemType SymbolicLink -Path "$d\w\lib\money\src\money\m.vib" -Target "$d\outside\money\src\money\m.vib" | Out-Null; New-Root "$d\w" @('money=lib/money') (Use @('money')); Check 'D12 module file symlink outside' "$d\w" } catch { "PROBE dep D12 : NOT RUN $($_.Exception.Message)" }
# D12b module file hard link to outside
$d = Fresh; New-Lib "$d\outside\money" 'money' @() ''; Write-Text "$d\w\lib\money\project.vibon" (Get-Content "$d\outside\money\project.vibon" -Raw); New-Item -ItemType Directory -Force "$d\w\lib\money\src\money" | Out-Null
New-Item -ItemType HardLink -Path "$d\w\lib\money\src\money\m.vib" -Target "$d\outside\money\src\money\m.vib" | Out-Null; New-Root "$d\w" @('money=lib/money') (Use @('money'))
Check 'D12b module file hard link' "$d\w"

# D13 a dependency module names a unit its project does not declare
$d = Fresh; New-Lib "$d\w\lib\other" 'other' @() ''; New-Lib "$d\w\lib\money" 'money' @() "use @other.m enduse`n`nfn one`n  takes {value: I32}`n  returns result: I32`n  visibility @public`n  do`n    return m.one(value)`nendfn`n"; New-Root "$d\w" @('money=lib/money', 'other=lib/other') (Use @('money', 'other'))
Check 'D13 undeclared unit from a dependency' "$d\w"
# D14 a dependency module names the root target
$d = Fresh; New-Lib "$d\w\lib\money" 'money' @() "use @app.main enduse`n`n$one"; New-Root "$d\w" @('money=lib/money') ((Use @('money')) + "`nfn two`n  takes {value: I32}`n  returns result: I32`n  visibility @public`n  do`n    return value`nendfn`n")
Check 'D14 dependency names the root target' "$d\w"
# D14b a dependency names `tests`
$d = Fresh; New-Lib "$d\w\lib\money" 'money' @() "use @tests.x enduse`n`n$one"; New-Root "$d\w" @('money=lib/money') (Use @('money'))
Check 'D14b dependency names @tests' "$d\w"

# U1 two libraries named money at different depths
$d = Fresh; New-Lib "$d\w\lib\money" 'money' @() ''; New-Lib "$d\w\lib\a\vendor\money" 'money' @() ''; New-Lib "$d\w\lib\a" 'a' @('money=vendor/money') ''; New-Root "$d\w" @('money=lib/money', 'a=lib/a') (Use @('money', 'a'))
Check 'U1 two money libraries' "$d\w"
# U2 one directory through a case alias, a junction and a short name
$d = Fresh; New-Lib "$d\w\lib\unitslongname" 'units' @() ''
$short = (cmd /c "for %I in (`"$d\w\lib\unitslongname`") do @echo %~snxI").Trim()
Junction "$d\w\lib\uj" "$d\w\lib\unitslongname"
New-Lib "$d\w\lib\a" 'a' @('units=../UNITSLONGNAME') ''; New-Lib "$d\w\lib\b" 'b' @('units=../uj') ''; New-Lib "$d\w\lib\c" 'c' @("units=../$short") ''
New-Root "$d\w" @('a=lib/a', 'b=lib/b', 'c=lib/c', 'units=lib/unitslongname') (Use @('a', 'b', 'c', 'units'))
Check "U2 diamond by alias, junction, short name ($short)" "$d\w"
# U3 a key equal to a root target
$d = Fresh; New-Lib "$d\w\lib\app" 'app' @() ''; New-Root "$d\w" @('app=lib/app') ''
Check 'U3 key equals the root target' "$d\w"
# U4 tests and std in a dependency's file
$d = Fresh; New-Lib "$d\w\lib\tests" 'tests' @() ''; New-Lib "$d\w\lib\money" 'money' @('tests=../tests') ''; New-Root "$d\w" @('money=lib/money') (Use @('money'))
Check 'U4 tests in a dependency' "$d\w"
$d = Fresh; New-Lib "$d\w\lib\std" 'std' @() ''; New-Lib "$d\w\lib\money" 'money' @('std=../std') ''; New-Root "$d\w" @('money=lib/money') (Use @('money'))
Check 'U4b std in a dependency' "$d\w"
# U5 a dependency package with a second library named like the root target
$d = Fresh; Write-Text "$d\w\lib\money\project.vibon" "{format: @project.v2 package: {name: `"money`" version: `"1.0.0`"} targets: [{name: @money kind: @lib root: `"src/money`"} {name: @app kind: @lib root: `"src/app`"}] dependencies: [:]}"; Write-Text "$d\w\lib\money\src\money\m.vib" $one; Write-Text "$d\w\lib\money\src\app\m.vib" $one; New-Root "$d\w" @('money=lib/money') (Use @('money'))
Check 'U5 unselected library named like the root target' "$d\w"
# U6 a dependency package's own target named std (not selected)
$d = Fresh; Write-Text "$d\w\lib\money\project.vibon" "{format: @project.v2 package: {name: `"money`" version: `"1.0.0`"} targets: [{name: @money kind: @lib root: `"src/money`"} {name: @std kind: @lib root: `"src/std`"}] dependencies: [:]}"; Write-Text "$d\w\lib\money\src\money\m.vib" $one; Write-Text "$d\w\lib\money\src\std\m.vib" $one; New-Root "$d\w" @('money=lib/money') (Use @('money'))
Check 'U6 unselected target named std' "$d\w"
```
