# Step 13: fs-scope-complete

Kind: infrastructure. Slice 7, below the driver: the Windows file-scope
adapter gains what the six `Dir…` operations need and it lacks: listing,
metadata, the link count, entry kinds, and the full reserved-name set. The
attack tests of the feasibility record become its permanent suite.
Depends on: 01.
Crates: `vibra-fs-scope`.

## Prerequisites

- Read: [effects
  chapter](../../../../spec/03-effects.md#filesystem-policies) "Filesystem
  policies", from "The driver MUST enforce a file rule" to the hard-link
  rule; [runtime
  chapter](../../../../spec/06-runtime.md#closed-operation-semantics)
  "Filesystem, common rules", "`DirList.entries`", "`DirStat.stat`", "File
  writes"; [the feasibility record](../../m0/file-scope-feasibility.md),
  its attack list and residual risks.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fs-scope/src/lib.rs:37` | `pub enum ScopeError` | gains `NotAFile`, `NotADirectory`, `NotUnicode` where missing |
| `crates/vibra-fs-scope/src/lib.rs:83` | `pub struct ScopedFile` | gains `links`, `real_names`, `size` |
| `crates/vibra-fs-scope/src/windows.rs:54` | `pub struct ScopedDir` | gains `list` and `stat` |
| `crates/vibra-fs-scope/src/windows.rs:127` | `pub fn open_root` | unchanged |
| `crates/vibra-fs-scope/src/windows.rs:163` | `pub fn open_for_replace` | opens without truncation |
| `crates/vibra-fs-scope/src/name.rs:58` | `pub(crate) fn check` | the name profile; gains the closed device set |
| `crates/vibra-fs-scope/src/other.rs:10` | `pub struct ScopedDir` | the stub of other platforms; gains the same signatures |

Proposed: `ScopedDir::{list, stat}`, `ScopedFile::{links, size,
real_names, replace_contents}`, `EntryKind`.

## Files owned

- `crates/vibra-fs-scope/src/`
- `crates/vibra-fs-scope/tests/`

## Ordered tasks

1. `name::check` rejects exactly the chapter's set: a stem, compared
   without case, of `CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`,
   `COM0` to `COM9`, `LPT0` to `LPT9`, and the six superscript forms; the
   stem is the part before the first dot with trailing spaces removed.
   Invariant: the function is the same on every platform.
2. Every open returns the names the filesystem reports for each component
   (`real_names`) and the entry's kind: file, directory, other. A regular
   file where a directory is required is `NotADirectory`; the reverse is
   `NotAFile`. Invariant: nothing is read before the caller sees them.
3. `ScopedFile::links()` reports the link count of the opened handle;
   `size()` its byte length.
4. `open_for_replace` opens with write access and no truncation;
   `replace_contents(bytes)` truncates and writes on that handle.
   Invariant: between the open and `replace_contents` the file is
   unchanged.
5. `list(path)`: walks to the directory, returns the entry names without
   `.` and `..`, sorted by Unicode scalar; a name that is not Unicode is
   `NotUnicode`.
6. `stat(path)`: opens without following a link and returns kind and
   size, zero for a kind that is not a regular file.
7. Every operation walks from the root handle and closes every
   intermediate handle before it returns.
8. `other.rs` has the same signatures and returns `Unsupported` from
   `open_root`. It compiles on Linux and macOS.

## Test matrix

Host tests in `tests/it/attacks.rs` and `tests/it/controls.rs`, Windows only;
each creates its objects in a new temporary directory and prints `NOT RUN`
with the reason when the machine refuses to create one. `r` is the scoped
root.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| read a file | positive | `r/a.txt` holding `hello`; `open_read(["a.txt"])` | 5 bytes, `real_names` `["a.txt"]`, kind file | 03: "Perform the access on the object that was opened and checked" |
| `ATK-fs-file-symlink` | negative | `r/link.txt` a symbolic link to a file outside `r` | `ScopeError::Link`; the target is not opened | 03: "without following symbolic links, junctions, or other reparse points" |
| `ATK-fs-junction` | negative | `r/j` a junction to a directory outside; `open_read(["j" "a.txt"])` | `ScopeError::Link` | the same |
| intermediate symlink | negative | `r/d` a directory symlink; `open_read(["d" "a.txt"])` | `ScopeError::Link` | the same |
| `ATK-fs-swapped-junction` | negative | a thread replaces `r/d` by a junction in a loop while 10,000 reads of `r/d/a.txt` run | every read is the inside file or `Link`; never the outside content | 03: "Checking a path and then reopening it by name is not conforming" |
| `ATK-fs-case-alias` | boundary | `r/Notes.TXT`; `open_read(["notes.txt"])` | opens; `real_names` is `["Notes.TXT"]` | 03: "Check the name the filesystem actually reports" |
| `ATK-fs-short-name` | boundary | `r/longfilename.text`; open by its 8.3 name | opens; `real_names` is the long name | 03: "A different-case or short-name alias MUST NOT satisfy a name test the real name fails" |
| `ATK-fs-replace-no-truncate` | positive | `open_for_replace(["a.txt"])`, then drop the handle without writing | the file still holds `hello` | 03: "Never truncate or create before authorization" |
| replace writes in place | positive | `open_for_replace` then `replace_contents("x")` on a file with a second hard link | both names read `x`; the file identity is unchanged | 06 File writes: "truncate-and-write in place on the checked handle" |
| `ATK-fs-create-over-alias` | negative | `r/Notes.txt` exists; `create_new(["NOTES.TXT"])` | `ScopeError::Exists`; contents unchanged | 03: "Creating a new file is exclusive: an existing entry, under any alias, is not replaced" |
| create a new file | positive | `create_new(["new.txt"])` | created, empty | the same |
| link count one | boundary | a plain file | `links()` is 1 | 03 Hard links: "when the platform reports a link count above one" |
| link count two | boundary | a file with a hard link outside `r` | `links()` is 2 | the same |
| `ATK-fs-root-renamed` | positive | open `r`, rename it, create another directory at the old path, read `a.txt` | the content of the renamed directory | 03: "Authority follows the opened directory, not its path" |
| list sorted | positive | entries `b`, `A`, `é`, `a` | `["A" "a" "b" "é"]` | 06 `DirList.entries`: "sorted by Unicode scalar" |
| list a file | negative | `list(["a.txt"])` | `NotADirectory` | 06: "a regular file where a directory is required as `notADirectory`" |
| list through a junction | negative | `list(["j"])` | `Link` | 06: "It MUST refuse to traverse a symbolic link or reparse point" |
| stat a file | positive | `stat(["a.txt"])` | file, 5 | 06 `DirStat.stat`: "the byte length of a regular file" |
| stat a directory | positive | `stat(["d"])` | directory, 0 | 06: "zero for any other kind" |
| stat a link | negative | `stat(["link.txt"])` | `Link`; the target's size is not returned | 06: "It does not follow a link" |
| read a directory | negative | `open_read(["d"])` | `NotAFile` | 06: "a directory or other non-regular entry where a file is required as `notAFile`" |
| missing parent | negative | `open_read(["no" "a.txt"])` | `NotFound` | 06: "the selected entry, or a parent it needs, does not exist" |
| each reserved stem | negative | `nul`, `NUL.txt`, `con .log`, `COM0`, `LPT9.x`, `CONIN$`, `COM¹`, `aux.tar.gz` through `name::check` | rejected | 03 Reserved device names: "`nul`, `NUL.txt`, and `con .log` are rejected" |
| near a reserved stem | positive | `nul2`, `com10`, `console`, `lpt` | accepted | 03: "and `nul2` is not" |
| name profile | negative | `.`, `..`, `a/b`, `a\b`, `a:b`, `a.`, `a `, a name with U+0007, the empty name | each rejected | 03 Paths: "A name is non-empty and is rejected if…" |
| alternate stream | negative | `open_read(["a.txt::$DATA"])` | rejected by `name::check` | 03 Paths: contains `:` |
| no handle kept | positive | after 1,000 operations, the process handle count is that of the root alone | equal | 03: "Keep no intermediate directory handle between operations" |
| other platforms | boundary | `open_root` on Linux and macOS (compiled in CI) | `Unsupported` | 03: "return `FsError.unsupported` before acting" |

## Diagnostics and schema changes

None.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-fs-scope -p vibra-host --all-features -- --nocapture
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. Count the `NOT RUN` lines: each is a row without
evidence on this machine and is listed in the report with its reason; a
symlink row needs Developer Mode or an elevated shell.

## Stays unavailable

The adapter on Linux and macOS (`Unsupported`); remote volumes and
filesystems other than NTFS, which step 31 refuses.

## Excluded

Policy, the gate, and the driver (31); append, remove, rename.

## Completion evidence

- [ ] Each of the 28 rows is a test; the report gives pass, fail, or
      `NOT RUN` for each.
- [ ] `unsafe` appears only in `src/windows.rs`.
- [ ] CI builds the crate on the three platforms.
- [ ] The validation commands exit 0.
