# Step 02: publish-primitive

Kind: infrastructure. The one way the engine will put bytes under a name:
a new file created exclusively under a reserved name in the held
directory, written, flushed, then renamed over the destination relative to
the same held directory with replace-if-exists and POSIX semantics; and
the removal of a reserved file. The crate keeps its rules: no link
followed, every real name checked, the hard-link limit decided by the
caller from the link count the open reports.
Depends on: —.
Crates: `vibra-fs-scope`.

## Prerequisites

- Read: [03 Filesystem policies](../../../../spec/03-effects.md#filesystem-policies)
  rules 1 to 6 and "Two limits of this design";
  [05 Transactional edit plans](../../../../spec/05-tooling.md#transactional-edit-plans)
  ("publish all files, or none"; "An interrupted application is never
  reported as applied"); [architecture](../architecture.md) section 3 and
  rows I8, I9; [M0 file-scope feasibility](../../m0/file-scope-feasibility.md);
  [questions](../spec-questions.md) Q15, Q16, Q17.
- Microsoft's documentation of `FILE_RENAME_INFO`,
  `FILE_RENAME_POSIX_SEMANTICS`, `SetFileInformationByHandle`,
  `FILE_DISPOSITION_INFO_EX`, and `FlushFileBuffers`, as the architecture
  quotes them.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fs-scope/src/lib.rs:51` | `pub use windows::{ScopedDir, ScopedListing, ScopedParent, close, is_regular_file}` | the held root and its operations |
| `crates/vibra-fs-scope/src/lib.rs:248` | `pub fn replace_contents` | the truncate-and-write step the publisher no longer uses for a document |
| `crates/vibra-fs-scope/src/lib.rs:218` | `pub fn links` | the link count the caller checks |
| `crates/vibra-fs-scope/src/windows.rs:228` | `pub fn open_for_replace` | the destination is opened first for its real names and link count |
| `crates/vibra-fs-scope/src/windows.rs:266` | `pub fn open_parent` | the held directory a reserved file is created in |
| `crates/vibra-fs-scope/src/windows.rs:366` | `pub fn create_new` | exclusive creation relative to the held directory |
| `crates/vibra-fs-scope/src/name.rs:64` | `pub fn check_name` | every name of a request |
| `crates/vibra-fs-scope/tests/it/attacks.rs:1` | `//!` | the model of an attack row on a real filesystem |

Proposed: `ScopedFile::flush`, `ScopedFile::rename_into(parent, leaf)`,
`ScopedParent::remove_file(leaf)`; on every other platform each is
`ScopeError::Unsupported`.

## Files owned

- `crates/vibra-fs-scope/src/lib.rs`
- `crates/vibra-fs-scope/src/windows.rs`
- `crates/vibra-fs-scope/src/other.rs`
- `crates/vibra-fs-scope/tests/it/main.rs`
- `crates/vibra-fs-scope/tests/it/publish.rs`

## Ordered tasks

1. `ScopedFile::flush`: `FlushFileBuffers` on the handle. Invariant: the
   data and the file's own metadata are written to the disk when it
   returns; nothing is said of the directory.
2. `ScopedFile::rename_into(self, parent, leaf)`:
   `SetFileInformationByHandle` with `FileRenameInfoEx`, `RootDirectory`
   the held directory of `parent`, `FileName` the one name `leaf` after
   `check_name`, flags `FILE_RENAME_REPLACE_IF_EXISTS |
   FILE_RENAME_POSIX_SEMANTICS`. The handle is consumed. Invariant: the
   request names one component relative to a held handle; no path is
   joined.
3. `ScopedParent::remove_file(leaf)`: open `leaf` relative to the held
   directory with `DELETE` access and no reparse following, require a
   regular file, then `FileDispositionInfoEx` with
   `FILE_DISPOSITION_DELETE | FILE_DISPOSITION_POSIX_SEMANTICS`.
   Invariant: a link, a directory, or another kind is refused and nothing
   is removed.
4. The crate documentation: the publish sequence as its callers use it
   (open the destination for its real names and link count, open the
   parent, create the reserved file, write, flush, rename over), the three
   parts of the contract in the architecture's words, and the sentence
   that the crate decides no name and no policy.
5. `other.rs`: the three functions return `Unsupported`.

## Positions

| Rule | Positions, each with a positive and a negative row |
| --- | --- |
| the destination's name never refers to a partial document | a failure before the write; after a partial write; after the flush and before the rename; against: after the rename (the new bytes) |
| the rename is relative to the held directory | the plain destination; against: a link in the directory's place; an alias of the leaf; a destination with two names |

## Test matrix

Host tests on Windows in `tests/it/publish.rs`; elsewhere each row prints
`NOT RUN: no scoped directory adapter`. `D` is a temporary directory
holding `src/app/main.vib` with known bytes.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| publish replaces | positive | create `.main.vib.vibra-publish` in `src/app`, write new bytes, flush, rename over `main.vib` | `main.vib` holds the new bytes; no reserved file remains; the directory listing has the one entry it had | 05: "publish all files, or none" |
| publish creates | positive | the same with no `main.vib` present | `main.vib` exists with the bytes | 05: the approval's destination may not exist yet (Q7) |
| `ATK-publish-failure-before-rename` | negative | a hook fails the write after half the bytes, and separately fails the flush | `main.vib` holds the old bytes; the reserved file holds a prefix or all of the new bytes; nothing else changed | 05: "An interrupted application is never reported as applied" |
| `ATK-publish-reader-keeps-old-bytes` | positive | a second handle holds `main.vib` open for reading across the publish | that handle reads the old bytes to its end; a new open of `main.vib` reads the new bytes | the architecture's quote of `FILE_RENAME_POSIX_SEMANTICS`: "Existing handles to the replaced file continue to be valid… Any subsequent opens of the target name will open the renamed file" |
| `ATK-publish-read-only-destination` | negative | `main.vib` has the read-only attribute | the rename fails with `ScopeError::Io`; `main.vib` is unchanged; the caller's test shows the reserved file removed | the architecture's quote: a read-only target fails even with replace |
| `ATK-publish-symlink-refused` | negative | `src/app` replaced by a symbolic link to a directory outside `D` | `open_parent` is `DeniedReparse`; nothing is created anywhere | 03 rule 1: "without following symbolic links, junctions, or other reparse points" |
| `ATK-publish-alias-refused` | negative | the request names `MAIN~1.VIB` or `Main.vib` while the real name is `main.vib` | the real names differ from the request and the caller refuses before creating | 03 rule 2: "A different-case or short-name alias MUST NOT satisfy a name test the real name fails" |
| `ATK-publish-hard-link-refused` | negative | `main.vib` has a second hard link elsewhere in `D` | `open_for_replace` reports `links() == 2`; the caller refuses before creating; both names unchanged | 03: "Replacing it MUST be refused with `denied` when the platform reports a link count above one" |
| reserved name exists | negative | a leftover `.main.vib.vibra-publish` is present | `create_new` is `AlreadyExists`; after `remove_file` of the leftover the sequence completes | architecture, section 3, "A leftover" |
| remove follows no link | negative | `.main.vib.vibra-publish` is a symbolic link to a file outside `D` | `remove_file` is `DeniedReparse`; the outside file and the link are unchanged | 03 rule 1 |
| remove refuses a directory | negative | `.main.vib.vibra-publish` is a directory | `NotAFile`; nothing removed | 03 rule 4: "Never truncate or create before authorization" |
| rename into another directory refused | negative | `rename_into` given a `leaf` containing `\` or `..` | `DeniedName`; nothing moved | 03 rule 3: the name profile |
| off Windows | boundary | every row on another platform | `Unsupported`; each row prints `NOT RUN` | Q16 |

## Diagnostics and schema changes

None: the crate reports `ScopeError`; the engine maps it in step 15.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-fs-scope -p vibra-host -p vibra-workspace -p vibra-cli --all-features -- --nocapture
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0; on Windows no row prints `NOT RUN`. The M2 driver tests
of `vibra-host` pass unchanged: nothing the drivers call changes.

## Stays unavailable

Everything observable: no command uses the primitive until step 15.

## Excluded

Directory creation; a rename across directories; any name or policy
decision, which stays with the caller; a POSIX adapter.

## Completion evidence

- [ ] Each row is a test, named in the report; each `ATK-` row was seen to
      fail with its protection removed, and the report says how.
- [ ] The report quotes the three parts of the contract from the crate
      documentation and states that the rename names one component
      relative to a held handle.
- [ ] The validation commands exit 0.
