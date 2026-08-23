# 7. Standard library

Status: draft

The v1 standard library is a **kernel**: the smallest set of modules a real
command-line program needs, and nothing else. It is written in Vibra, lives in
this repository, and is versioned with the compiler.

Everything here is normative in the sense that the module must exist with the
listed name, effect roots, and error type. The exact function inventory of each
module is fixed during milestone M7 and recorded in this document as it lands;
a module's surface is not open to interpretation once written.

## Module inventory

### Pure modules

No effect roots. Every function is total or returns a `result`.

| Module | Provides |
| --- | --- |
| `core` | Primitive types, `result`, `option`, `ordering`, primitive operations. Implicitly in scope in every file. |
| `text` | `str` operations: length, slice, split, join, trim, case, search, replace, UTF-8 iteration. |
| `bytes` | Immutable byte sequences, and conversion to and from `str` with explicit encoding failure. |
| `array` | Construction, indexing, slicing, `map`, `filter`, `fold`, `sort` over `(array t)`. |
| `map` | Lookup, insert, remove, iteration over `(map k v)`. Keys must implement `hashable`. |
| `math` | Checked, wrapping and saturating arithmetic; min, max, abs; float predicates. |
| `convert` | Every conversion between primitive types. Lossy conversions return `result`. Atom to `str`. |
| `path` | Pure path manipulation: join, parent, extension, normalize. Touches no filesystem. |
| `error` | The `error` interface and helpers for composing error types across module boundaries. |

### Effectful modules

| Module | Roots | Error type |
| --- | --- | --- |
| `stream` | `stream.read`, `stream.write`, `stream.manage` | `stream.error` |
| `io` | `io.stdin`, `io.stdout`, `io.stderr` | `stream.error` |
| `fs` | `fs.read`, `fs.write`, `fs.metadata` | `fs.error` |
| `env` | `env.read`, `env.write` | `env.error` |
| `process` | `process.spawn`, `process.wait`, `process.signal` | `process.error` |
| `sys` | `sys.info`, `sys.exit` | `sys.error` |
| `time` | `time.now`, `time.sleep` | `time.error` |
| `random` | `random.generate` | `random.error` |

### The test module

| Module | Provides |
| --- | --- |
| `test` | `scenario` and `case` declarations, and typed assertions. No effect roots; assertions are pure. |

## Core interfaces

These interfaces are declared in `core` and implemented for the primitive types
by the standard library. Because implementations live with the type
([Types](04-types.md)), user code cannot add implementations for primitives —
wrap them in a `newtype` instead.

| Interface | Required members |
| --- | --- |
| `display` | `display.show ((self self)) str` |
| `equatable` | `equatable.equals ((self self) (other self)) bool` |
| `comparable` | `comparable.compare ((self self) (other self)) ordering` — implies `equatable` |
| `hashable` | `hashable.hash ((self self)) uint64` — implies `equatable` |
| `error` | `error.message ((self self)) str` — implies `display` |

`ordering` is `(enum (less void) (equal void) (greater void))`.

## Errors

Every effectful module owns one error type, declared with `deftype` and
implementing `error`. Errors are enums, so a caller can match specific cases.

This is `fs.error`, written inside the `fs` module where its local name is just
`error`:

```vibra
(import stream "@std/stream.vib")
(import text "@std/text.vib")

(deftype error
  (enum
    (not-found str)
    (permission-denied str)
    (already-exists str)
    (invalid-path str)
    (from-stream stream.error))
  doc: "Everything that can go wrong reaching the filesystem."
  implements: (error)

  (defn error.message ((self self)) str
    (match self
      (case (error.not-found (bind path))
        (text.concat "not found: " path))
      (case (error.permission-denied (bind path))
        (text.concat "permission denied: " path))
      (case (error.already-exists (bind path))
        (text.concat "already exists: " path))
      (case (error.invalid-path (bind path))
        (text.concat "invalid path: " path))
      (case (error.from-stream (bind inner))
        (error.message inner)))))
```

Two naming details that example depends on, both spelled out in
[Types](04-types.md) and [Modules and names](03-modules.md):

- A module never names itself. Inside `fs`, the type is `error` and its
  constructors are `error.not-found` and peers; `fs.error.not-found` is what
  *other* modules write.
- `error` here is both the local type and the `core` interface it implements.
  Inside the block, a member named `error.message` is the interface method,
  because a listed interface wins the prefix. `error.not-found` still resolves
  to the constructor, because the interface has no member by that name.

Crossing module boundaries requires an **explicit conversion**, because `try`
never infers one ([Expressions](05-expressions.md)). Each module provides the
conversions into its own error type that its own operations need — here, the
`from-stream` tag. A module never defines a conversion into someone else's error
type.

## Streams

`stream` defines the shared reading and writing surface, as interfaces:

| Interface | Members |
| --- | --- |
| `stream.readable` | read into a buffer; read a chunk |
| `stream.writable` | write bytes; write a `str`; flush |
| `stream.closeable` | close |

Files, standard streams, and process pipes keep distinct nominal handle types
while implementing these interfaces where their semantics permit. A file
handle is not a socket, and neither widens to the other.

Composite conveniences — read-all, write-all, copy, line iteration — are
ordinary Vibra functions built above the primitive operations, never
intrinsics. The primitive boundary stays as small as the ABI rule in
[Effects](06-effects.md) allows.

## What the CLI target needs

These are the specific surfaces the v1 acceptance programs depend on, listed so
that M7 has an unambiguous completion test:

- `env.read.args` — the argument vector as `(array str)`.
- `env.read.get` / `env.write.set` — environment variables, returning
  `(option str)` and `result` respectively.
- `io.stdin`, `io.stdout`, `io.stderr` — standard streams as handles.
- `fs.read.to-str`, `fs.read.open`, `fs.write.from-str`, `fs.write.open`,
  `fs.metadata.exists`, `fs.metadata.stat`.
- `path` join, parent, extension, normalize.
- `process.spawn.command`, `process.wait.status`, and access to the child's
  stdin, stdout and stderr as stream handles.
- `sys.exit.with-status` — terminate with a status code.
- `time.now.monotonic`, `time.now.wall`, `time.sleep.for`.
- `text` split-lines, trim, concat, contains, replace.

## Explicitly not in v1

`net`, task and concurrency primitives, JSON/TOML/XML decoding, compile-time
file embedding, templating, and argument-parsing frameworks. Each is assigned to
a wave in [Deferred and rejected](10-deferred.md).

Argument parsing in particular is a library concern, not a language concern:
v1 hands the program `env.read.args` and stops there.

## Testing convention

Every standard-library module has a matching `tests/stdlib-<module>.vib` file.
A module without one is not complete, regardless of whether its code works.

Tests that touch real host state — the filesystem, subprocesses, the real
environment — live in a non-`core` profile so that a bare `vibra test` stays
hermetic and reproducible.
