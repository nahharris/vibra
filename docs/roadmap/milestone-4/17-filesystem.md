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
   operations: `fs.read-file`, `fs.read-text`, `fs.read-entries`, `fs.write-file`,
   `fs.write-text`, `fs.metadata-stat`. The parameter is spelled `at`.
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
always an `err` and never a trap; `fs.read-entries` order does not depend on the
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
