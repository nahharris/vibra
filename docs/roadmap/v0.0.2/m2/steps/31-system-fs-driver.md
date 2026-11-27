# Step 31: system-fs-driver

Kind: vertical. Slice 7, the directory driver: `@host.fs.directory` on
Windows over `vibra-fs-scope`, with the gate deciding on the opened
object. On Linux and macOS the provider stays without a driver.
Depends on: 13, 30.
Crates: `vibra-host`.

## Prerequisites

- Read: [effects
  chapter](../../../../spec/03-effects.md#filesystem-policies) "Filesystem
  policies" whole; [runtime
  chapter](../../../../spec/06-runtime.md#closed-operation-semantics)
  "Filesystem, common rules" to "File writes"; [programs
  chapter](../../../../spec/04-programs-and-packages.md#startup-order) the
  `@host.fs.directory` row; [the feasibility
  record](../../m0/file-scope-feasibility.md), residual risks;
  [architecture](../architecture.md) "The order of a mutating call",
  "The gate", "Settlement", section 3, and rows A21 to A26;
  [questions](../spec-questions.md) Q7, Q29.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fs-scope/src/windows.rs:170` | `pub fn open_root` | acquisition |
| `crates/vibra-fs-scope/src/windows.rs:197` | `pub fn open_read` | reads |
| `crates/vibra-fs-scope/src/windows.rs:319` | `pub fn create_new` | exclusive creation |
| `crates/vibra-fs-scope/src/windows.rs:204` | `pub fn open_for_replace` | replacement without truncation |
| `crates/vibra-fs-scope/src/lib.rs:56` | `pub enum ScopeError` | mapped to `FsError` cases |

From step 13: `list`, `stat`, `links`, `real_names`, `replace_contents`,
`open_parent`, `ScopedParent::create_new`, `check_name`. From step 14:
`path::check_name` and its vector file. From step 15: `Gate`, `Found`,
`Completion`, `Progress`.

Proposed: `vibra-host::system::fs::{DirectoryAdapter, map_error}`.

## Files owned

- `crates/vibra-host/src/system.rs`
- `crates/vibra-host/src/system/fs.rs`
- `crates/vibra-host/tests/it/fs.rs`
- `crates/vibra-host/tests/it/system.rs`

## Ordered tasks

1. `identity(@host.fs.directory)`: `Some` on Windows, `None` elsewhere.
2. `acquire`: resolve `@fixedPath` against the working directory at
   startup; open the root once with `open_root`. Classify: nothing there,
   not a directory, permission denied, not Unicode are `Refused`; a
   volume on which the adapter cannot report normalized names or link
   counts, and any other error, are `Failed`.
3. Each operation on an existing entry, in this order: open through
   `vibra-fs-scope` from the root with the request's names (the prefix of
   a `sub` included, Q7); build `Found` from the opened object
   (`real_names`, the found action, `size`, `links`); `Gate::admit`; only
   then read, replace, or return metadata. A refusal of the gate returns
   `Completion::Refused`. Invariant: no byte is read or truncated, and no
   metadata is returned, before `admit` returns `Ok`.
4. A write of an existing file: `open_for_replace`, which does not
   truncate; `Found` with `Replace` and the link count; `Gate::admit`;
   then `replace_contents` on that handle.
5. A write where `open_for_replace` finds no entry is a create, in this
   order: `open_parent` with the names before the leaf, which keeps the
   parent's handle and reports its real names; `Found` with `Create` and
   the parent's real names followed by the proposed leaf; `Gate::admit`;
   and only then `ScopedParent::create_new(leaf)`, which creates
   exclusively, relative to the held parent. A `create_new` that finds an
   entry (a race) is a completed `denied` and never falls back to
   replace. A parent that does not exist is `notFound`. Invariant: the
   one authority check of a create comes before the entry exists, and it
   governs the parent object the creation uses; nothing is created and
   then removed.
6. Where the adapter cannot hold the parent, report real names, or report
   a link count, the reply is `unsupported`, before any change.
7. `map_error`: `DeniedReparse` to a completed `denied`; `NotFound`,
   `NotAFile`, `NotADirectory`, `NotUnicode` (`invalidText`), access
   denied (`permissionDenied`), `Unsupported` (`unsupported`, before
   acting), anything else `other`. Every completion carries its
   `Progress`: the bytes read or written when the count is known, zero
   for a refusal of the adapter, `Uncertain` for a failure after a write
   began whose count the system does not report.
8. `release` closes the root handle. The driver has no remove, rename,
   append, or directory creation.
9. `tests/it/system.rs`: the row `ATK-driver-absent-at-admission` of
   step 30 runs where `identity(@host.fs.directory)` is `None`, so from
   this step it is compiled for Linux and macOS only.

## Test matrix

Host tests on Windows through `Context` over a `SystemDriver`, each in a
new temporary directory `r`; a row prints `NOT RUN` with its reason when
the machine cannot create its object. `P(…)` is the leaf's policy. After
every refusal of a write, the test reads the directory with `std::fs`:
"unchanged" means the listing of `r`, recursively, and the bytes of every
file equal what they were before the call. The reply alone is never the
evidence.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| read text | positive | `r/a.txt` `hello`; `P(tree read anyName)`; `fs.readText ["a.txt"]` | `Ok("hello")`; one event | 06 Host registry |
| each operation | positive | read bytes, list, stat, write text (new), write bytes (existing) | each `Ok`; the directory holds the results | 07 Required coverage 12 |
| `ATK-fs-alias-name` | negative | `r/NOTES.TXT`; `P(suffix ".txt")`; read `["notes.txt"]` | `Err(denied)`, a completed event; the file is not read | 03: "A different-case or short-name alias MUST NOT satisfy a name test the real name fails" |
| alias that passes | positive | `r/Notes.txt`; `P(suffix ".txt")`; read `["NOTES.TXT"]` | `Ok` | 03: "Check the name the filesystem actually reports" |
| short name | negative | `r/longfilename.text`; `P(suffix ".TEX")`; read by its 8.3 name | `Err(denied)` | the same |
| symlink leaf | negative | `r/l.txt` a symbolic link outside | `Err(denied)`, completed | 06: "A refusal the driver makes after it has opened an entry … is `denied`" |
| junction component | negative | `r/j` a junction outside; read `["j" "a.txt"]` | `Err(denied)`; the outside file is not opened | 06: "It MUST refuse to traverse a symbolic link or reparse point" |
| `ATK-fs-hard-link-replace` | negative | `r/a.txt` with a second link outside `r`; `P(replace)`; write | `Err(denied)`; both names keep their content and their size | 03 Hard links: "MUST be refused with `denied`" |
| hard link read | positive | the same file; `P(read)`; read | `Ok` | 03: "Reading such a file is allowed" |
| replace in place | positive | a plain file `r/a.txt`, replaced; its file index read before and after | the file identity (index) is unchanged | 06: "A driver MUST NOT write a new file and rename it over the old one" |
| create exclusive | negative | `P(create)` only; `r/a.txt` exists; write `["a.txt"]` | `Err(denied)`, completed; `r` unchanged | 06: "it MUST NOT fall back to truncating an existing file" |
| create over an alias | negative | `P(create)`; `r/A.TXT` exists; write `["a.txt"]` | `Err(denied)`; `r` unchanged | 03: "an existing entry, under any alias, is not replaced" |
| create in the checked parent | positive | `r/out` a directory; `P(prefix ["out"], create)`; write `["out" "n.txt"]` | `Ok`; `r/out/n.txt` holds the text and `r` holds nothing else new | 03: "Perform the access on the object that was opened and checked" |
| `ATK-fs-create-denied-on-real-parent` | negative | `r/out` a directory; `P(prefix ["OUT"], create)`; write `["OUT" "n.txt"]` | `Err(denied)`, a completed event; `r` unchanged: `r/out` is empty | 03: "Never truncate or create before authorization" |
| create below a short-name parent | negative | `r/longdirectoryname`; `P(prefix [its 8.3 name], create)`; write below that 8.3 name | `Err(denied)`; `r` unchanged | 03: "A different-case or short-name alias MUST NOT satisfy a name test the real name fails" |
| create loses a race | negative | `P(create)`; a test hook creates `r/n.txt` after `admit` returned and before `create_new` | `Err(denied)`, completed; `r/n.txt` holds the hook's bytes | 03: "Creating a new file is exclusive" |
| replace only, new file | negative | `P(replace)`; write `["new.txt"]` | `Err(denied)`, completed; `r` unchanged | 06: "The policy must authorize the concrete action, create or replace" |
| denied leaves no trace on disk | negative | `P(suffix ".txt")`; write `["a.bin"]` | `Err(denied)` before the adapter; `r` unchanged | 06 step 3 |
| no parent creation | negative | write `["no" "a.txt"]` | `Err(notFound)`; `r/no` does not exist | 06: "do not create parent directories" |
| list needs a directory rule | negative | `P(file rule, read)`; `fs.readEntries []` | `Err(denied)` | 03: "A file rule grants no listing" |
| list sorted | positive | entries `b`, `A`, `a` | `["A" "a" "b"]` | 06: "sorted by Unicode scalar" |
| stat a link | negative | `stat ["l.txt"]` | `Err(denied)` | 06: "It does not follow a link" |
| read a directory | negative | `fs.readText ["d"]` | `Err(notAFile)` | 06 |
| invalid text | negative | a file `[255]`; `fs.readText` | `Err(invalidText)` | 06 |
| read over the limit | negative | a 5-byte file; `inputBytes: 4` | `Err(limitExceeded)`; the adapter's read call counter is zero; no event (Q29) | 06: "before any content is read" |
| `sub` walks from the root | positive | `sub ["d"]`; rename `r/d` to `r/e` and create a new `r/d` between two reads | the second read sees the new `r/d` | 06: "The driver MUST NOT cache a resolved child anchor between invocations" |
| root renamed | positive | rename `r`, create another at the old path, read | the renamed directory's content | 03: "Authority follows the opened directory, not its path" |
| `ATK-fs-no-normalized-name` | negative | the adapter's name query is made to fail by a test hook; a read, then a write of a new file | `Err(unsupported)` both times; nothing read; `r` unchanged; no event (Q29) | 03: "return `FsError.unsupported` before acting" |
| names agree with the adapter | positive | the vector file of step 14, on the three platforms | `path::check_name` and `vibra_fs_scope::check_name` agree on each vector | 03 Reserved device names: "the same closed set is rejected on every platform" |
| no remove and no rename | negative, architecture test | `crates/vibra-fs-scope/src` and `crates/vibra-host/src/system/fs.rs` searched for a delete disposition, a rename, and `std::fs::remove` | none | 03 Actions: "Append, remove, and rename do not exist yet" |
| acquisition refusals | negative | path names nothing; names a file | `Refused` | 04, the `@host.fs.directory` row |
| acquisition failure | negative | the test hook reports a volume without normalized names | `Failed` | 04: "a directory on which the driver cannot enforce the leaf's policy" |
| not Windows | boundary | `identity(@host.fs.directory)` on Linux and macOS, in CI | `None` | 04: "A driver the platform does not support at all" |

## Diagnostics and schema changes

None.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-fs-scope -p vibra-host -p vibra-workspace --all-features -- --nocapture
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. Every `NOT RUN` line is a row without evidence on this
machine and is copied into the report with its reason.

## Stays unavailable

`@host.fs.directory` on Linux and macOS: `@tool.unavailable` at
admission.

## Excluded

Remote volumes, filesystems other than NTFS (refused as failures),
OneDrive placeholders (refused as links), append, remove, rename.

## Completion evidence

- [ ] Each of the 33 rows is a test; the report gives pass, fail, or
      `NOT RUN` for each.
- [ ] `fs.rs` contains no `unsafe` and no `std::fs` call that takes a
      joined path; the report lists every call into `vibra-fs-scope`.
- [ ] In the write path `Gate::admit` is the last call before
      `replace_contents` and before `create_new`: the report quotes the
      function.
- [ ] CI is green on the three platforms.
- [ ] The validation commands exit 0.
