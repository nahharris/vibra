# Step 17 — filesystem operations

Prerequisite: Step 16b merged. Stage 4B behavior step: `fs.read`, `fs.write`, and
`fs.metadata` in both backends, with the `path` wrapper and the typed host errors.

## Read before editing

- [Runtime](../../spec/06-runtime.md): **The host registry** (the filesystem rows
  and the meaning of each error variant), **The host world** (filesystem
  resolution), **The audit trace**.
- [Effects](../../spec/03-effects.md): the `fs` rows of the inventory and the
  `at` parameter note.
- [Types](../../spec/02-type-system.md): **Nominal declarations** (a wrapper and its
  public constructor).
- [Ledger](decision-ledger.md) rows D17.2, D17.3, D17.5, D17.6, D20.2.

## Scope

1. `@std.fs` with `path = (deftype path str)` and its public constructor, `fs-error`,
   `fs-kind`, `fs-metadata`, the roots `read`, `write`, `metadata`, and the six
   operations: `fs.read.file`, `fs.read.text`, `fs.read.entries`, `fs.write.file`,
   `fs.write.text`, `fs.metadata.stat`. The parameter is spelled `at`.
2. The system provider's filesystem operations with each OS failure mapped to a
   closed variant (`not-found`, `permission-denied`, `not-a-file`, `not-a-directory`,
   `invalid-path` for a NUL, `invalid-text`, `other`), no trap.
3. The world's filesystem: ancestors implied, `..` against the root, writes to the
   copy, entries sorted by Unicode scalar order.
4. Both backends, with arguments of kind wrapper, `str`, and `bytes` and results of
   kind `bytes`, `str`, `array` of `str`, `record`, and `enum`.

## Entry points

| File | Use |
| --- | --- |
| `crates/vibra-host` (Step 16a) | The system provider and the world, where the filesystem lands |
| `stdlib/src/std/` and `stdlib/manifest.vibon` | The new module and the six `host` symbols; its digest in the manifest |
| `crates/vibra-wasm/src/layout.rs` | The kinds the builders create for these results (`array` of `str`, `record`, `enum`) |
| `conformance/cases/` | New `V1-RUNTIME` and `V1-EFFECT` cases with `host-world` inputs |

## Ordered tasks

1. Write `@std.fs`, add its digest and the symbols, and check that a parameter named
   `path` is not used (`@name.redeclaration`).
2. Implement the six operations once, with the mapping from OS errors, and the
   world's behavior.
3. Add the error-variant matrix as host tests against the system provider using a
   temporary directory, and as corpus cases against the world.
4. Add the demo-shaped case: read a file named by an argument-like world path,
   transform it, write the result, and read it back.
5. Move the cases to `matched`.

Invariants preserved: the language does not normalize paths; a host failure is
always an `err` and never a trap; `fs.read.entries` order does not depend on the
host; the world's writes never reach the real filesystem.

## Test matrix

- Positive: read a file as bytes and as text; list a directory (sorted, no `.` or
  `..`); write creating and replacing; stat a file, a directory, and an `other`
  kind where the host has one; non-ASCII names and contents.
- Negative: every error variant of every row: missing file and missing parent,
  a directory read as a file, a file listed as a directory, a path with NUL, invalid
  UTF-8 as text, a name that is not Unicode (system provider test on Unix),
  a permission failure (system provider test where the host allows it).
- Recovery: a failed write leaves the program running and the next call sees the
  prior contents (not atomic, and the test says so).
- Boundary: the empty file, a 4 MiB file, a directory of 10,000 entries, a path of
  a thousand components, `..` at the root of the world.
- Formatter: none beyond Step 16b.

## Diagnostic and schema changes

None.

## Validation

The [pre-merge list](validation.md#before-merging-each-step) and `cargo test
--locked --offline -p vibra-host -p vibra-wasm-run -p vibra-conformance`.

## Excluded

Append, remove, create-directory (later registry minors), streaming handles, path
algebra, symlink creation, permissions management.

## Completion evidence

The `fs` rows of the registry each have a success case and every reachable error
case in both backends with identical events.

## As built

Landed on its branch, conditional on its pull request merging; the base is the head
of Step 16b-2 (PR #376, `5889e81`), which had not merged. The six `fs` rows are
callable from source in both backends at once.

- **The module.** `stdlib/src/std/fs.vib` declares `path` (the wrapper
  `(deftype path str)` with its public constructor `(fs.path "text")`), `fs-error`,
  `fs-kind`, `fs-metadata`, and the roots `read`, `write`, and `metadata` with the
  six `visibility: @public` operations, each taking its path as `at`. The manifest
  lists `@std.fs` and the six `fs` symbols and stays `0.3.0`. The checker needed no
  change: `host_operation` already builds the `path`, `fs-metadata`, and
  `(array str)` types of a row.
- **The operations.** None was added in a backend. `HostCall`, `perform`, and
  `build_reply` in `vibra-host` already read the wrapper, the `str`, and the `bytes`,
  call the provider, and build the `bytes`, `str`, `array`, `record`, and `enum`
  results; the interpreter and the Wasm runner supply `HostValues` only. The world
  provider resolves paths from its root with no links; the system provider hands the
  text to the operating system.
- **One provider change.** A path whose nearest existing ancestor is a file is
  `not-a-directory` in the system provider on every platform (ledger D34.1); the
  operating system says so on Unix and says the path was not found on Windows.

### Rulings

- D34.1, above. It applies the registry's meaning of `not-a-directory` ("a file where
  a directory is needed, ... as a parent") and adds no rule to the language.
- The entry outcome of an `err` result (`@command.program-failed`, exit 6) is
  Step 19; until then `vibra run` reports it as `@command.ok` with the `err` as
  `programResult`, and the process test does not assert the outcome.

### Cases

Added, all `matched` in both backends: `V1-RUNTIME-fs-read-file-and-text`,
`V1-RUNTIME-fs-invalid-text`, `V1-RUNTIME-fs-errors-are-values` (not-found for a file
and a parent, not-a-file, not-a-directory as a target and as a parent, invalid-path),
`V1-RUNTIME-fs-read-entries-sorted`, `V1-RUNTIME-fs-write-then-read`,
`V1-RUNTIME-fs-metadata-stat`, `V1-RUNTIME-fs-error-propagates-with-try`,
`V1-RUNTIME-fs-demo-path-from-stdin`, and `V1-RUNTIME-fs-test-world-writes-are-fresh`
(`workspace-test`). Negatives: `V1-EFFECT-host-target-array-omits-fs-read` and
`V1-EFFECT-host-function-ceiling-omits-fs-read`. No case was re-expected, deleted, or
downgraded. Parity: 9 rows added, 151 of 151 `matched`.

### Evidence

- `fs_operations_m4_step17` (6 portable host tests and 3 Unix-only) drives all six
  rows through `HostCall::read`, `perform`, and `build_reply` once over the system
  provider in a real directory under the system temporary directory and once over a
  world, and requires one reply from both: reads, the empty file, non-ASCII names and
  contents, sorted entries, stat, every error variant of every row, replace and
  create, a failed write that leaves the prior contents, a 4 MiB file, a path of a
  thousand components, and a directory of 10,000 entries. The Unix-only tests cover a
  name that is not Unicode, a permission failure, and a device as the `other` kind.
- `process_m4_filesystem` (2 tests) runs the real binary over real files: the demo
  shape with a path from standard input, and a missing file as the `err` of its call.

### Not run on this platform

The development host was Windows, so the three Unix-only host tests (`cfg(unix)`)
were neither compiled nor run there; a Unix host runs them.
