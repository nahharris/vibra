# M2 step 33 — dual run and attack runs

One case run against the fake world and against the real directory
adapter, with the two outcomes compared; and the attack cases that need a
real filesystem, each run through a whole target. The harness is
`crates/vibra-workspace/tests/it/dual_run.rs` (`materialize`, `run_both`,
`compare`); the attack runs are
`crates/vibra-workspace/tests/it/attacks_run.rs`.

**Agreement is sampled evidence.** A row of the dual run shows that the two
drivers agreed on that one case: that world, that program, that platform.
It is evidence over the stated domain only, as 03 "Tests and model
profiles" says of every comparison of a real driver against its model; it
does not prove the drivers equivalent, and passing in the fake world does
not qualify the real adapter (07 Required coverage 11).

## How a case runs

1. **World half.** The tree runs under `Launch::World` with the world
   document.
2. **System half.** `materialize` writes the world's `directories` and
   `files` into a new temporary directory, making a directory
   case-sensitive (`fsutil file setCaseSensitiveInfo`) when its entries
   differ only in case. Each `@fixedPath` of the project file and each
   argument that is a virtual path are re-rooted there. The tree runs under
   `Launch::System { arguments, capture: true }`, which captures the
   program's standard output and standard error.
3. **Comparison.** `compare` reads the `@command.*` result, the
   application code, both streams, each binding's status, and every event
   field by field, source origins included. It leaves out the trace's
   `drivers`, each event's `driver`, the results of clock and random
   operations, and the message of a startup failure (its code is
   compared). The canonical text of a re-rooted argument is mapped back to
   its virtual path in the reads of `args`. The first difference is
   reported with the event's index and the field name, and both outcomes
   are printed whole.

**Domain.** The system half takes the process's own standard input and
environment, so a world with `stdin` or `env` is outside the harness's
domain and `materialize` refuses it. No row of the matrix sets either.

## Platform

Windows 11 Home, build 26300, NTFS on `C:`, Developer Mode on (file
symbolic links without elevation), 8.3 names generated, per-directory case
sensitivity available without elevation. The temporary directories are
under `%TEMP%`. Linux and macOS were not run: there the directory driver
answers no identity, and each row prints `NOT RUN … no directory driver`
(the world half of a dual row still runs).

`NOT RUN` lines on this machine: none.

## Dual rows

| Row | Test | Platform | Result | Domain of the evidence |
| --- | --- | --- | --- | --- |
| dual: copy | `dual_run::dual_copy` | Windows | both halves agree: `@command.ok`, code 0, stdout `hello from a`, 9 events; `out/a.txt` on disk holds the text | `C`, one 12-byte text file through an `@argumentPath` leaf with a `.txt` read rule, created under a `@fixedPath` leaf |
| dual: denied by policy | `dual_run::dual_denied_by_policy` | Windows | both agree: code 32 (`denied`), no `fs.readText` event, 6 events | `C` asking for `a.bin` under the `.txt` suffix rule |
| dual: not found | `dual_run::dual_not_found` | Windows | both agree: code 31 (`notFound`), one completed `fs.readText` event | `C` asking for an absent `no.txt` |
| dual: required refused | `dual_run::dual_required_refused` | Windows | both agree: `@command.operationalFailure`, `@source` refused with `@grant.requiredUnavailable`, the other leaves released | `C` with argument 0 naming an absent `/missing` |
| dual: listing | `dual_run::dual_listing` | Windows | both agree: `["A" "a" "b"]`, code 0, 2 events | a case-sensitive directory of three one-byte files, listed through a `@self` directory rule |
| dual: limits | `dual_run::dual_limits` | Windows | both agree: code 33 (`limitExceeded`) | `C` reading a 5-byte file through `source` with `inputBytes: 3u64` |
| dual: replace then read | `dual_run::dual_replace_then_read` | Windows | both agree: create, replace, read back, stdout `two`; `out/n.txt` on disk holds `two` | two leaves on one `@fixedPath`, one write-only and one read-only |
| dual: invalid text | `dual_run::dual_invalid_text` | Windows | both agree: code 34 (`invalidText`) | `C` reading the one-byte file `[255]` as text |
| dual: disagreement is reported | `dual_run::dual_disagreement_is_reported` | Windows (pure) | `compare` of two outcomes that differ in the `result` of event 6 fails with `event 6 field `result``, both sides printed | the comparison itself |

## Attack rows

Each attack runs under `Launch::System` only. The test builds its objects
in the temporary directory after `materialize`, then lists the world and
the outside objects recursively, reading every file's bytes and recording
each link by its target without following it, before and after the run.
A refusal is judged by that comparison, never by the reply alone.

| Row | Test | Platform | Result | Domain of the evidence |
| --- | --- | --- | --- | --- |
| `ATK-run-symlink-denied` | `attacks_run::windows::atk_run_symlink_denied` | Windows | passed: code 32 (`denied`), one completed `fs.readText` event, the outside text in neither stdout nor the destination, the directory unchanged | a file symbolic link `/in/l.txt` to a file outside the grant |
| `ATK-run-junction-denied` | `attacks_run::windows::atk_run_junction_denied` | Windows | passed: code 32, the directory unchanged | `/in/sub` a junction outside, `sub/a.txt` asked through a `@tree` rule |
| `ATK-run-alias-denied` | `attacks_run::windows::atk_run_alias_denied` | Windows | passed: `LONGSE~1.DAT` asked under an `@exact` rule holding that short name: code 32, one completed event, the directory unchanged | the 8.3 short name of `/in/longsecretname.dat` |
| `ATK-run-hard-link-read-allowed` | `attacks_run::windows::atk_run_hard_link_read_allowed` | Windows | passed: the read succeeds, stdout and the copy hold the linked data | `/in/a.txt` a hard link to a file outside |
| `ATK-run-hard-link-replace-denied` | `attacks_run::windows::atk_run_hard_link_replace_denied` | Windows | passed: code 42 (write `denied`), one completed `fs.writeText` event, the outside file and the directory unchanged | the destination `out/a.txt` a hard link to a file outside |
| `ATK-run-denied-leaves-file` | `attacks_run::windows::atk_run_denied_leaves_file` | Windows | passed: code 42, the existing file and the directory unchanged | `dest` permitting `create` only, its file existing |
| `ATK-run-create-denied-leaves-directory` | `attacks_run::windows::atk_run_create_denied_leaves_directory` | Windows | passed: code 42, one completed event, `out` holds no entry, the directory unchanged | a policy prefix `["OUT"]` with `create`, the directory `out` on disk, the entry writing `["OUT" "n.txt"]` |
| `ATK-run-root-replaced` | `attacks_run::windows::atk_run_root_replaced` | Windows | passed: the entry read `original` from the renamed directory; `in/a.txt` at the old path holds `replacement` | `in` renamed to `inOld` and a new `in` made after startup, before the entry's read |
| `ATK-run-prefix-by-names` | `attacks_run::windows::atk_run_prefix_by_names` | Windows | passed: code 32, no `fs.readText` event, the directory unchanged | a prefix `["reports"]` and the sibling `reportsOld/a.txt` |
| `ATK-run-swapped-component` | `attacks_run::windows::atk_run_swapped_component` | Windows | passed: 1,000 reads in one entry, during 117 to 222 swaps per run, gave the inside text, `denied`, or `notFound`, and never the outside text; the directory unchanged | `/in/sub` renamed between a directory and a junction outside by another thread |
| `ATK-run-ambient-path` | `attacks_run::windows::atk_run_ambient_path` | Windows | passed: `..`/`x`, `C:\x`, and `\\?\C:\x` each give code 21 (no `RelativePath`), no `fs` host call, the directory unchanged | names taken from the program's arguments |

The codes are those of the copy program `C` and its siblings: 0 done; 21
the names are no `RelativePath`; 31 `notFound`, 32 `denied`, 33
`limitExceeded`, 34 `invalidText`, 35 `invalidPath`, 39 another read error;
a write error adds 10.

**`notFound` in the swapped row.** Windows has no atomic exchange of two
directory entries, so the swapping thread renames in two steps, and
between them `sub` does not exist; a read in that moment is `notFound`.
The row's protection, that no read reaches the outside content, holds over
every read.

## Each attack row fails with its protection removed

Each protection was removed by a temporary edit, the rows it guards were
run, and the edit was reverted (`git checkout` of the file); the working
tree was clean afterwards.

| Mutation | Protection removed | Rows that fail | Where they fail |
| --- | --- | --- | --- |
| M1 | `vibra-fs-scope/src/windows.rs`: no `OBJ_DONT_REPARSE`, no `FILE_OPEN_REPARSE_POINT`, and no refusal of an opened component that is a reparse point | `ATK-run-symlink-denied`, `ATK-run-junction-denied`, `ATK-run-swapped-component` | the code is not 32 (`denied`); the swapped row reads the outside text (`x` marks) |
| M2 | `vibra-host/src/broker.rs`: the gate does not evaluate the policy on the real names the driver found | `ATK-run-alias-denied`, `ATK-run-create-denied-leaves-directory` | the code is not 32, and not 42 for the create |
| M3 | the gate's hard-link rule never refuses a replace | `ATK-run-hard-link-replace-denied` | the code is not 42 |
| M3r | the reverse, for the positive row: the link-count rule applied to reads | `ATK-run-hard-link-read-allowed` | the code is not 0: the read no longer succeeds |
| M4 | `vibra-host/src/system/fs.rs`: the opened file is truncated before the gate decides | `ATK-run-denied-leaves-file`, `ATK-run-hard-link-replace-denied` | the directory comparison: the file and the outside file are empty |
| M6 | `vibra-host/src/system`: every operation reopens the held directory by its path | `ATK-run-root-replaced` | stdout is not `original`: the entry no longer reads the renamed directory |
| M7 | `vibra-host/src/policy.rs`: a prefix is compared as a string | `ATK-run-prefix-by-names` | the code is not 32 |
| M8 | `vibra-ir/src/names.rs`: `fs.validNames` accepts every name | `ATK-run-ambient-path` | the code is not 21: the construction succeeds |
