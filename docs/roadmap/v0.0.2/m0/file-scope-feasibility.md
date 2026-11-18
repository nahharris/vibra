# Windows file-scope feasibility

The feasibility check of [M0](README.md): can a Windows driver enforce a
scoped file grant as the
[effects chapter](../../../spec/03-effects.md#filesystem-policies) and the
[runtime chapter](../../../spec/06-runtime.md#closed-operation-semantics)
require? The adapter is the crate
[`vibra-fs-scope`](../../../../crates/vibra-fs-scope/src/lib.rs). Its tests
run on the real filesystem under the temporary directory.

**Verdict: meets the contract.**

## Approach

A `ScopedDir` holds one directory handle, acquired once. Every operation
walks from it one name at a time and performs its access on the last handle
it opened and checked.

| Operation | What it does |
| --- | --- |
| `ScopedDir::open_root(path)` | Opens `path` once through the ordinary Win32 path rules, as a directory, and refuses it if it is itself a reparse point |
| `open_read(&[name])` | Opens an existing file for reading |
| `create_new(&[name])` | Creates a new file; any existing entry is `alreadyExists` |
| `open_for_replace(&[name])` | Opens an existing file for writing, without changing it |
| `ScopedFile::replace_contents(bytes)` | The deliberate replacement step, on the opened handle |

Every result is a `Result` with the closed enum `ScopeError`:
`deniedReparse`, `deniedName`, `notFound`, `alreadyExists`, `unsupported`,
and `io`. A wrong kind of entry is `io` with `NotADirectory` or
`IsADirectory`, and an operating-system refusal is `io` with
`PermissionDenied`. The crate has no panic path and no `unwrap`. On every
platform other than Windows each operation returns `unsupported`, after the
lexical check below.

### The exact calls

1. **Lexical check, before any call.** Each name of the request must be a
   plain name under the rules of `RelativePath`: non-empty; not `.` or `..`;
   no `/`, `\`, `:`, NUL, or other control character; no trailing dot or
   space; and not a reserved device name with or without an extension. The
   device names are `CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`,
   `COM0`–`COM9`, `LPT0`–`LPT9`, and `COM` or `LPT` with a superscript 1, 2,
   or 3, compared in any ASCII case against the part before the first dot
   with trailing spaces removed. An empty request is `deniedName`. These
   rules make absolute, drive-relative, UNC, `\\?\`, `\??\`, and stream
   inputs unrepresentable.
2. **The root.** `CreateFileW` through `std::fs::OpenOptions`, with access
   `FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE`,
   sharing read, write, and delete, and flags
   `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`. Delete
   sharing lets the directory be renamed while it is held; the handle keeps
   referring to it.
3. **Each component.** `NtCreateFile` with `RootDirectory` set to the
   previous handle, `ObjectName` set to that one name, `Attributes` set to
   `OBJ_DONT_REPARSE`, and `FILE_OPEN_REPARSE_POINT |
   FILE_SYNCHRONOUS_IO_NONALERT` added to the create options. A link is
   opened as the link, never as its target.

   | Open | Access | Sharing | Disposition | Options |
   | --- | --- | --- | --- | --- |
   | Intermediate directory | `FILE_TRAVERSE \| FILE_READ_ATTRIBUTES \| SYNCHRONIZE` | read, write, delete | `FILE_OPEN` | none |
   | Read | `FILE_READ_DATA \| FILE_READ_ATTRIBUTES \| SYNCHRONIZE` | read, write, delete | `FILE_OPEN` | none |
   | Replace | `FILE_WRITE_DATA \| FILE_READ_ATTRIBUTES \| SYNCHRONIZE` | read | `FILE_OPEN` | none |
   | Create | `FILE_WRITE_DATA \| FILE_READ_ATTRIBUTES \| DELETE \| SYNCHRONIZE` | read | `FILE_CREATE` | none |

   No open asks for `FILE_DIRECTORY_FILE` or `FILE_NON_DIRECTORY_FILE`, so a
   link of either kind is opened and then refused as a reparse point, and a
   create over an existing directory is a collision, not a wrong kind.
4. **Each opened entry is checked on its own handle,** in this order:
   - `NtQueryInformationFile(FileAttributeTagInformation)`: an entry with
     `FILE_ATTRIBUTE_REPARSE_POINT` is `deniedReparse`, whatever its tag:
     symbolic links, junctions, mount points, and every other reparse point.
   - The kind: an intermediate entry must be a directory and the leaf must
     not be one.
   - `NtQueryInformationFile(FileNormalizedNameInformation)`: the last
     component of the normalized name, which is the long name in the case
     stored on disk, must equal the requested name in exact UTF-16 units.
     Anything else is `deniedName`. A filesystem that does not answer this
     class is `unsupported`.
5. **Release.** Intermediate handles are closed when the operation ends; no
   resolved directory is kept between operations.
6. **A create that fails its check** is marked for deletion with
   `NtSetInformationFile(FileDispositionInformation)` before its handle
   closes, and the check's error is returned.

`NtCreateFile` statuses map to the enum: `STATUS_OBJECT_NAME_NOT_FOUND`,
`STATUS_OBJECT_PATH_NOT_FOUND`, and `STATUS_NO_SUCH_FILE` to `notFound`;
`STATUS_OBJECT_NAME_COLLISION` to `alreadyExists`;
`STATUS_OBJECT_NAME_INVALID` to `deniedName`;
`STATUS_REPARSE_POINT_ENCOUNTERED` and `STATUS_STOPPED_ON_SYMLINK` to
`deniedReparse`; every other failure to `io`.

### Replacement

`open_for_replace` uses `FILE_OPEN` and never `FILE_OVERWRITE`,
`FILE_OVERWRITE_IF`, or `FILE_SUPERSEDE`, so the open leaves the file as it
was. `replace_contents` then sets the end of file to zero (`SetEndOfFile`,
through `File::set_len`), and writes the new bytes from offset zero, on the
same checked handle. The replacement is in place: the file keeps its
identity and every name it has. It is not transactional; a failure can
leave the file empty or holding a prefix, as the runtime chapter allows.

### Dependency and unsafe code

The one new dependency is `windows-sys` 0.61.2, already in the lock file and
in the local cargo cache, with the features `Wdk_Foundation`,
`Wdk_Storage_FileSystem`, `Win32_Foundation`, `Win32_Security`,
`Win32_Storage_FileSystem`, and `Win32_System_IO`. The crate restates the
workspace lints with `unsafe_code` allowed and `unsafe_op_in_unsafe_fn` and
`clippy::undocumented_unsafe_blocks` denied. Its five `unsafe` blocks are
all in `src/windows.rs`, each with a `SAFETY` comment: the `NtCreateFile`
call, taking ownership of its handle, the two `NtQueryInformationFile`
calls, and the `NtSetInformationFile` call. The architecture test permits
`unsafe` in that one file of the crate and nowhere else in it.

## Cases

Machine: Windows 11, build 26300, NTFS on `C:`. Symbolic links can be
created without elevation (Developer Mode). The volume generates 8.3 names
(`NtfsDisable8dot3NameCreation` is 2, per volume, and `C:` creates them).
Every case ran; none is NOT RUN. Each test that needs a link, a junction, or
a short name detects whether the machine can make one and prints
`NOT RUN <case>: <reason>` instead of passing when it cannot.

Each attack asserts the specific denial for read, replace, and create, and
that the outside target was not read, created, or modified: the outside tree
is compared with a snapshot taken before the attack, and where the outside
secret is the target it is held open with no sharing for the whole attack,
so a denial cannot follow an open of its data.

### Positive controls

| Case | Test | Result |
| --- | --- | --- |
| Read an existing file | `controls::reads_an_existing_file` | passed |
| Create a new file | `controls::creates_a_new_file` | passed |
| Nested directories to depth 3: read, create, replace | `controls::opens_nested_directories_to_depth_three` | passed |
| Replace without truncation at open: a second handle reads the old bytes after `open_for_replace` returns and before `replace_contents` | `controls::replace_does_not_truncate_at_open` | passed |
| A missing file or parent | `controls::a_missing_file_is_not_found` | passed: `notFound` |
| The root is renamed and another directory takes its path; operations still reach the renamed directory | `controls::the_root_follows_the_directory_it_opened` | passed |
| A root that is a file | `controls::a_root_that_is_a_file_is_refused` | passed: `io` `NotADirectory` |

### Attack cases

| Case | Test | Result |
| --- | --- | --- |
| File symlink to outside | `attacks::file_symlink_to_outside_is_denied` | denied as required: read and replace `deniedReparse`; create `alreadyExists` |
| Directory symlink to outside, below it and as the leaf | `attacks::directory_symlink_to_outside_is_denied` | denied as required: `deniedReparse`; create of the link's own name `alreadyExists` |
| Junction to outside (`mklink /J`), below it and as the leaf | `attacks::junction_to_outside_is_denied` | denied as required: `deniedReparse`; create of the junction's own name `alreadyExists` |
| A symlink as an intermediate component | `attacks::symlink_as_an_intermediate_component_is_denied` | denied as required: `deniedReparse` |
| A directory swapped for a junction between two opens through the same held root | `attacks::a_directory_swapped_for_a_junction_between_opens_is_denied` | denied as required: the first open reads the directory, the second is `deniedReparse` |
| A root that is a junction | `attacks::a_root_that_is_a_junction_is_denied` | denied as required: `deniedReparse` |
| `..` escape, `.`, empty component, empty request, `..\` inside a name | `attacks::dot_dot_escape_is_denied` | denied as required: `deniedName` |
| Absolute path, and a drive-rooted `\Users\…` path | `attacks::absolute_path_is_denied` | denied as required: `deniedName` |
| Drive-relative `C:foo`, and `C:` as a component | `attacks::drive_relative_path_is_denied` | denied as required: `deniedName` |
| UNC `\\localhost\C$\…`, and UNC as components | `attacks::unc_path_is_denied` | denied as required: `deniedName` |
| `\\?\`, `\??\`, and `\\.\` prefixes | `attacks::verbatim_prefix_is_denied` | denied as required: `deniedName` |
| `/` or `\` inside a component | `attacks::a_separator_inside_a_component_is_denied` | denied as required: `deniedName` |
| Case-different file name and directory name | `attacks::a_case_different_name_is_denied` | denied as required: read and replace `deniedName`; create over the file's alias `alreadyExists`; create below the directory's alias `deniedName` |
| 8.3 short name of a long-named file and of a long-named directory | `attacks::an_eight_dot_three_short_name_is_denied` | denied as required: read and replace `deniedName`; create over the short name `alreadyExists` |
| Trailing dot | `attacks::a_trailing_dot_is_denied` | denied as required: `deniedName` |
| Trailing space | `attacks::a_trailing_space_is_denied` | denied as required: `deniedName` |
| `file.txt::$DATA` | `attacks::the_default_data_stream_syntax_is_denied` | denied as required: `deniedName` |
| `file.txt:stream` | `attacks::a_named_stream_is_denied` | denied as required: `deniedName`; no stream created |
| `NUL` and `nul`, as a leaf and as a directory | `attacks::the_nul_device_is_denied` | denied as required: `deniedName`; nothing created |
| `CON` | `attacks::the_con_device_is_denied` | denied as required: `deniedName` |
| `COM1` | `attacks::the_com1_device_is_denied` | denied as required: `deniedName` |
| `aux.txt` | `attacks::a_device_name_with_an_extension_is_denied` | denied as required: `deniedName` |
| Create over an existing file, and over its case alias | `attacks::create_over_an_existing_file_is_refused` | denied as required: `alreadyExists`; contents unchanged |
| Create through a dangling file symlink, below a dangling directory symlink, and replace through the dangling link | `attacks::create_through_a_dangling_symlink_is_refused` | denied as required: `alreadyExists`, `deniedReparse`, `deniedReparse`; no target created |
| Hard link to an outside file | `attacks::a_hard_link_to_an_outside_file_is_followed_as_the_specification_states` | recorded: see below |

**Hard links.** A hard link inside the root to a file outside it is an
ordinary entry of the root: the same file under a second name, with no
reparse point. The normalized name of a handle opened through the link is
the link's own name, so the name check passes. A read through the permitted
name returns the outside file's data; `open_for_replace` changes nothing at
open, and `replace_contents` then changes the data the outside name also
shows; `create_new` over the link is `alreadyExists`. This is the limit the
effects chapter states: a policy does not establish where data came from or
that a write is seen only under the permitted name, and a read-only grant
still cannot write. The adapter cannot tell such a file from an ordinary one
by its name alone. It can read the link count of the opened handle
(`FileStandardInformation`, `NumberOfLinks`), which would let a driver
refuse a replace of a file with more than one name; the specification does
not ask for that.

### The checks carry the cases

Two mutations of `src/windows.rs`, each restored afterwards, show that the
handle checks, not only the lexical rules, decide the cases:

| Mutation | Tests that fail |
| --- | --- |
| The real-name comparison always passes | the case-different and the 8.3 cases |
| No `FILE_OPEN_REPARSE_POINT`, no `OBJ_DONT_REPARSE`, no attribute check | the file symlink, directory symlink, junction, intermediate symlink, swapped junction, and dangling symlink cases |

## Residual risks

- **An alias is opened before it is refused.** For a case-different or
  short-name alias, the adapter opens the real entry with the operation's
  access, checks it, and closes it without reading or writing data; for a
  link it does the same to the link itself, never to its target. The open
  is visible to the system: it can break an opportunistic lock, hold a share
  mode for that moment, and appear in an audit log. A replace open asks for
  write access before the name check, so an alias of a read-only file would
  be `io` `PermissionDenied` rather than `deniedName`; this order was not
  tested.
- **Every reparse point is refused.** That includes cloud-file placeholders
  (OneDrive), deduplicated files, and app execution aliases, not only links.
  A grant over a synchronized folder with placeholders cannot read them.
- **The root is acquired through the Win32 path rules.** Links among its
  ancestors are followed once, at acquisition; only the root directory
  itself must not be a reparse point. The runtime resolves a root from the
  target's recipe, so this is the trusted step.
- **Only NTFS on a local fixed volume was tested.** ReFS, FAT, exFAT, SMB
  shares, and directories with per-directory case sensitivity were not.
  A filesystem that does not answer `FileNormalizedNameInformation` is
  `unsupported`, so the adapter fails closed there; whether each of them
  answers it, and with the stored case, is unmeasured.
- **The non-Windows stub was not compiled here.** The machine has only the
  `x86_64-pc-windows-msvc` target installed.
- **Hard links,** as above.
- **Cost.** Each component takes four system calls (the open, two queries,
  and the close) and a 64 KiB name buffer. No cost was measured.

## Questions for the specification

1. **Which `FsError` case does a link refusal produce?** The runtime chapter
   says a driver refuses to traverse a link, but no `FsError` case names that
   refusal. `denied` means the policy rejected the request and nothing was
   dispatched, while a link is refused after its entry was opened. The same
   question holds for an alias that fails the real-name check.
2. **The device-name list.** "A reserved Windows device name" should be the
   exact set: whether `CONIN$`, `CONOUT$`, `COM0`, `LPT0`, and the
   superscript forms are reserved, and whether the stem is taken before the
   first dot with trailing spaces removed, as this adapter does.
3. **Hard links on replace.** Whether a driver MAY or MUST refuse to replace
   a file whose link count is above one, given that it can see the count on
   the opened handle.
4. **Wrong kind of entry.** The adapter's enum has no case for a directory
   where a file is required; the runtime's `FsError` has `notAFile` and
   `notADirectory`. A driver above this adapter would map `io` with
   `IsADirectory` and `NotADirectory` to them; whether the adapter should
   carry those two cases itself is open.
5. **Replacement strategy.** Whether in-place truncate-and-write on the
   checked handle is the required meaning of "replace the whole contents",
   or whether a driver may write a new file and rename it over the old name,
   which would give a new file identity and drop the other names of a hard
   link.
