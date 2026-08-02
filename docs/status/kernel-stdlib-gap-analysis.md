# Vibra kernel and standard-library gap analysis

Status: exploration report, 2026-07-19

> **Note:** this report predates the removal of Vibra's capability/policy
> authority system (fully decommissioned; every host operation is now
> unconditionally available at runtime). References below to "capability
> gating," "capability review," "capability attenuation," and similar design
> constraints describe a system that no longer exists; read them as
> historical context, not current design guidance. Generic uses of
> "capability" meaning general feature/ability are unaffected.

## Purpose and scope

This report compares Vibra's current language kernel and standard library with
the low-to-mid-level facilities that make Rust and Go practical for general
systems and application programming. It is a capability comparison, not a goal
to copy either library package-for-package. Vibra's WebAssembly target,
capability model, static typing, value-copy semantics, and single-threaded v1
runtime should determine the eventual API shapes.

The audit covers:

- value operations and control flow;
- strings, bytes, collections, iteration, conversion, and errors;
- I/O, filesystem, environment, time, randomness, process, and networking;
- memory/resource lifecycle and concurrency foundations;
- testing and diagnostics required to make those APIs dependable.

Out of scope for the initial backlog are higher-level protocols and formats
(HTTP, TLS, JSON, compression, databases), platform-specific APIs, reflection,
and a broad collection zoo. Those belong after the foundational interfaces are
usable.

## Sources and method

Vibra evidence was taken from `docs/archive/yaml-surface-draft.md`, `README.md`, `stdlib/src/*.vib`, the
`vibra_v1` host implementation in `src/`, and the Rust and Vibra test suites.
The comparison baseline uses official documentation:

- [Rust standard library](https://doc.rust-lang.org/std/index.html), including
  [primitive strings](https://doc.rust-lang.org/stable/std/primitive.str.html),
  [slices](https://doc.rust-lang.org/stable/std/primitive.slice.html),
  [collections](https://doc.rust-lang.org/stable/std/collections/index.html),
  [iterators](https://doc.rust-lang.org/stable/core/iter/),
  [results](https://doc.rust-lang.org/stable/std/result/index.html), and
  [synchronization](https://doc.rust-lang.org/stable/std/sync/index.html).
- [Go language specification](https://go.dev/ref/spec) and the official
  packages for [bytes](https://pkg.go.dev/bytes),
  [strings](https://pkg.go.dev/strings), [I/O](https://pkg.go.dev/io),
  [OS services](https://pkg.go.dev/os), and
  [synchronization](https://pkg.go.dev/sync).

The baseline is intentionally the intersection of recurring essentials, not
the union of every Rust and Go feature. Rust and Go both make sequences, maps,
strings, numeric operations, iteration, conversion, errors, stream I/O,
filesystem traversal, time, processes, networking, and concurrency directly
usable. The implementation strategies differ substantially.

## Executive assessment

Vibra already has a promising typed skeleton:

- fixed-width integer and floating-point primitives, booleans, strings, arrays,
  maps, tuples, records, unions, enums, newtypes, interfaces, generics,
  functions, mutation/references, conditionals, and exhaustive matching;
- generic `option` and `result` enums;
- nominal `bytes` with length, slicing, and UTF-8 conversion;
- capability-gated filesystem handles, environment, clock, random, process,
  networking, system information, and console I/O;
- an explicit, versioned WebAssembly host ABI and typed error enums;
- language-native tests and structural code tooling.

However, the surface is currently broader than it is deep. Basic programs
cannot yet rely on a coherent set of operations over primitives and
collections. Several APIs are declarations over minimal or placeholder host
behavior rather than useful application abstractions. The highest-value work
is therefore a vertical foundation: operations -> iteration -> text and
conversion -> reusable I/O -> complete host resources. Adding more top-level
modules before this foundation would increase surface area without increasing
composability.

## Capability matrix

| Area | Rust / Go baseline | Vibra today | Gap | Priority |
| --- | --- | --- | --- | --- |
| Numeric and boolean operations | Arithmetic, comparison, bitwise, shifts, checked/overflow behavior | Primitive types exist; the draft uses `$add` illustratively, but no general callable operation surface is defined or registered | Specify and implement typed operations, division/overflow rules, comparisons, min/max, and numeric conversion | P0 |
| Control flow for workloads | `for`/`while`/`loop`, ranges, early exit; Go `range` | `$if`, `$match`, `$do`, functions; no general loop/range/break/continue facility | Add a minimal iteration/loop construct with bounded and collection traversal semantics | P0 |
| Arrays / dynamic sequences | Rust slices/`Vec`; Go arrays/slices; indexing, length, append, copy, insert/remove | `$array` values/types and pattern matching; bytes has only `len`/`slice` | Define safe get/set, length, slice, append, insert/remove, capacity policy, and copy/alias behavior | P0 |
| Maps and sets | Hash/ordered maps, lookup, insert/delete, membership, iteration; Go built-in maps | `$map` construction and pattern matching | Define key constraints, lookup returning `option`, mutation, delete, membership, length, deterministic iteration; defer sets as a thin map abstraction | P0 |
| Strings and Unicode | UTF-8-aware traversal plus bytes, find/split/trim/case/replace; conversions | `$str` primitive; bytes conversion only | Establish UTF-8 invariant and byte-vs-code-point indexing policy; add core query, split, trim, search, replace, join, and case APIs | P0 |
| Iteration and algorithms | Rust `Iterator`; Go `range`, `slices`, `maps`, `sort` | No reusable iterator protocol or standard sequence algorithms | Introduce an iteration protocol and adapters/consumers appropriate to Vibra; initially include range, map/filter/fold, any/all/find, collect, sort | P0 |
| `Option` / `Result` ergonomics | Rich combinators and language integration (`?` in Rust; conventional propagation in Go) | Two generic enums only | Add query, transform, defaulting, conversion, and propagation ergonomics without hiding effects | P1 |
| Conversion and formatting | Rust `From`/`TryFrom`, parsing, formatting; Go `strconv`/`fmt` | Empty conversion interfaces; `bytes` UTF-8 conversion; console accepts strings | Define lossless/fallible numeric conversions, parse/format primitives, debug/display contracts, and interpolation or structured formatting | P0 |
| Stream I/O | Rust `Read`/`Write`/`BufRead`; Go `Reader`/`Writer`, copy/limit/buffer helpers | Mode-specific file interfaces and whole-string/bytes operations | Extract reusable byte-reader/writer/closer interfaces, partial-read/write contracts, EOF, buffering, copy, and line reading | P1 |
| Filesystem and paths | Rich path manipulation, metadata, open options, traversal, rename/copy, links | Strong start: nominal paths, typed modes, open/read/write/append, metadata, directory listing, canonicalize, create/remove | Add join/parent/file-name/components, rename/copy, create/truncate options, recursive removal policy, richer metadata and directory entries | P1 |
| Time | Instants, durations, wall clock, timers/sleep | Unix milliseconds only | Add `duration`, monotonic instant, checked arithmetic, sleep/timer primitives; defer calendars/time zones | P1 |
| Environment / arguments | Env lookup/enumeration, working directory, executable path, argv | Env get/set; system info string | Add remove/list, argv, cwd/chdir, executable/temp directory, and structured system information with capability review | P1 |
| Process | Spawn/run, args/env/cwd, stdio, exit status, wait/kill | `process.run(string) -> result<void, ...>` placeholder-level shape | Model command, arguments, exit status/output, stdio policy, wait/kill, and capability attenuation | P1 |
| Networking | Address types, TCP/UDP listeners/streams, DNS, deadlines | `connect(string)` and `listen(string)` return `void` | Return resource handles; add typed addresses, read/write/close, accept, DNS, UDP, timeout/deadline, and precise errors | P1 |
| Resource lifecycle | Rust RAII; Go explicit `Close` plus runtime support | Closeable file modes exist; v1 arena lifetime; net/process lack resources | Define deterministic close behavior, idempotence/use-after-close errors, cleanup at instance end, and resource limits | P0 |
| Concurrency | Threads/tasks, channels, mutex/atomics; Go goroutines/channels/context | Explicitly single-threaded v1 | First specify cancellation and async host-operation model; only then tasks/channels/synchronization. Threads may remain out of scope for Wasm v1 | P2 |
| Memory / low-level interop | Rust ownership/raw pointers/alloc; Go pointers/runtime; both expose byte-level interop | Arena-backed values/references, safe `$wasm` registry, no general raw memory API | Document layout/copy costs and resource limits; add safe buffers. Keep raw pointers/allocator APIs out unless a concrete FFI use case requires them | P2 |
| Errors and diagnostics | Standard error contracts, wrapping/context, inspection | Typed per-module enums and `result`; several error variants carry strings or lose detail | Establish common error interface/kinds, source/context chain, stable host error mapping, and non-panicking invalid-input behavior | P1 |
| Test utilities | Assertions, fixtures, benchmarks, examples | Typed assertions and profiles/tags/capability gating | Add table/property helpers, deterministic random/time injection, temp resources, and benchmarks after collection APIs stabilize | P2 |

## Kernel findings

### 1. Operations are the critical blocker

The type system can represent useful data but lacks a documented, complete way
to compute with it. Numeric and boolean operations, equality/ordering, string
comparison, collection indexing, and collection mutation need to be first-class
contracts. The draft's `$add` examples should either become real prelude
functions/interfaces or be replaced with the chosen canonical API.

Semantics must be explicit for integer overflow, division by zero, float NaN,
shift bounds, numeric widening/narrowing, equality of composites, map key
eligibility, and out-of-bounds access. A typed `option`/`result` outcome is
preferable where failure is routine; traps should be reserved for genuine
contract violations and documented consistently.

### 2. Iteration should unify collections and control flow

Rust's collection APIs rely on iterators, while Go makes traversal a language
construct through `range`. Vibra needs one coherent approach before arrays,
maps, strings, directories, and future streams independently invent traversal.
For v1, a small nominal `iterator<T>` protocol plus `$for` syntax (or a
stdlib-driven equivalent that the compiler can lower efficiently) would unlock
far more than a large catalog of collection types.

The minimum useful behavior is: ranges; forward iteration; early break and
continue; deterministic map traversal; transforming/filtering/folding; and
collecting into arrays/maps. Borrowing variants can wait because Vibra currently
copies values by default, but costs and mutation-during-iteration must be
specified.

### 3. Memory and resource semantics need a public contract

The draft explains program-instance arena lifetime for mutable cells and
references, but users also need allocation limits, value copy costs, collection
growth behavior, host-resource cleanup, and use-after-close behavior. This is
more important to a Wasm language than exposing raw pointers. Rust's ownership
model and Go's garbage-collected pointers are reference points, not templates;
Vibra can retain its safer arena/value model while making performance and
lifecycle predictable.

### 4. Concurrency should follow cancellable I/O

Implementing threads, mutexes, or Go-like channels immediately would force
premature decisions about aliasing, `Send`/`Sync`-like constraints, Wasm
threads, and host scheduling. First define cancellation/deadlines and an async
host-operation contract. A later structured-task abstraction can then build on
those rules. Native threads and shared-memory synchronization are P2 and may
remain unsupported on the first Wasm target.

## Standard-library findings

### 1. Text, bytes, collections, and conversion form one dependency cluster

The existing `bytes` newtype is a sound seed but too small for binary protocols
or efficient I/O. It needs safe indexing, mutation/building, search, concat,
comparison, and explicit UTF-8 validation outcomes. Strings need a declared
UTF-8 invariant and separate byte, Unicode scalar, and substring operations.
Numeric parsing/formatting and a display/debug contract are prerequisites for
useful CLI programs and diagnostics.

Start with one dynamic sequence and one deterministic map. Rust documents many
specialized collections, but even its guidance identifies vectors and hash maps
as covering most general storage. Deques, trees, heaps, and dedicated sets
should be demand-driven.

### 2. Existing host APIs should become composable resources

Filesystem support is Vibra's strongest host module, including typed file-mode
interfaces. The next step is to generalize reader/writer/closer behavior so
files, network streams, process pipes, and memory buffers share algorithms.
Both Rust and Go standardize these small I/O interfaces because composition is
more valuable than duplicated whole-buffer helpers.

`net.connect`, `net.listen`, and `process.run` currently return `void` on
success. Those signatures prove capability checks but cannot support useful
network or process programs. They should return typed handles/results with
deterministic lifecycle, granular errors, deadlines/cancellation, and explicit
stdio or socket I/O.

### 3. Capability safety must remain part of every API design

New filesystem, process, environment, clock, and network operations must not
silently widen authority. Resource handles should carry attenuated authority
where possible. Enumeration, DNS, current-directory changes, process env, and
recursive deletion deserve explicit threat-model review and narrow grants.

## Recommended delivery order

1. Freeze kernel operation semantics and implement primitive comparisons,
   arithmetic, conversions, and error rules.
2. Make arrays/maps safely operable and add a minimal iteration/loop protocol.
3. Build UTF-8 string/bytes and parse/format facilities on that base.
4. Enrich `option`/`result` and define a common error/context contract.
5. Introduce reusable stream I/O interfaces and deterministic resource
   lifecycle rules.
6. Complete paths/filesystem, time/environment/args, process, and networking in
   that order, reusing the stream and lifecycle contracts.
7. Specify cancellation/async execution; evaluate structured tasks and channels
   only after real I/O workloads validate the model.

Each behavior change must follow the repository policy: focused Rust tests for
compiler/runtime work, matching flat `tests/stdlib-<module>.vib` coverage for
stdlib work, both full test suites, formatting/linting, documentation updates,
and schema changes for machine-readable contracts.

## Proposed issue backlog

The following issues are intentionally outcome-oriented. Each should be split
into implementation slices only after its design contract is accepted.

### P0 — Define and implement core primitive operations

Specify callable arithmetic, comparison, boolean, and bitwise operations for
all primitive types. Include overflow, division-by-zero, shifts, float/NaN,
equality/ordering, and checked numeric conversion semantics. Reconcile the
currently illustrative `$add` syntax with actual call resolution.

Acceptance: representative operations compile and execute through both runtime
paths; invalid type combinations are compile-time diagnostics; edge semantics
have focused Rust and Vibra tests; the language spec is normative.

### P0 — Make arrays and maps usable collections

Add safe length/get/set/slice/append/insert/remove/contains operations for
arrays and length/get/insert/remove/contains for maps. Specify key constraints,
copy/alias behavior, mutation, bounds failures, growth limits, and deterministic
map iteration policy.

Acceptance: non-trivial array and word-frequency style map programs can be
written without host calls; APIs return typed optional/error outcomes; tests
cover empty, bounds, duplicate-key, mutation, and generic element cases.

### P0 — Add iteration, ranges, and loop control

Design a minimal iteration protocol and canonical traversal syntax or lowering.
Support integer ranges, arrays, maps, strings at an explicitly chosen unit, and
early `break`/`continue`. Provide foundational consumers/adapters such as fold,
map, filter, any/all, find, and collect where they fit the v1 type system.

Acceptance: one protocol serves every required source; map order and
mutation-during-iteration are specified; lowering is bounded and testable;
examples include search, reduction, transformation, and nested traversal.

### P0 — Establish string, bytes, parsing, and formatting foundations

Define the `$str` encoding invariant and indexing model. Add common length,
empty, contains, prefix/suffix, find, split, trim, replace, join, case, and
iteration operations. Expand bytes with safe get/build/concat/search and
fallible UTF-8 decoding. Add primitive parse/format plus display/debug contracts.

Acceptance: text APIs document bytes versus Unicode scalars; malformed UTF-8
and parse failures are typed; CLI-style input/validation/output can be expressed
without ad-hoc host helpers; tests include non-ASCII and boundary cases.

### P0 — Specify deterministic host-resource lifecycle and limits

Define ownership, close, duplicate-close, use-after-close, automatic instance
cleanup, maximum handles/allocation, and error mapping for files and future
network/process/timer resources. Document arena value-copy and collection-growth
costs and establish a safe buffer strategy.

Acceptance: every host handle follows one lifecycle contract; leaked resources
are reclaimed at instance end; limits produce stable typed errors; adversarial
runtime tests cover exhaustion and invalid handle use.

### P1 — Enrich option/result and standardize error context

Add essential combinators and queries (`is-*`, map, and-then, unwrap-or-style
defaulting, option/result conversion) and choose an explicit propagation
mechanism. Define a common error kind/display/source-or-context convention and
stable mapping from host failures without discarding structured details.

Acceptance: ordinary pipelines avoid repetitive exhaustive matches while
remaining explicit; propagation preserves error type/context; filesystem,
environment, process, and network errors conform to the shared contract.

### P1 — Introduce reusable stream I/O and finish filesystem/path essentials

Define byte reader, writer, closer, optional seeker, EOF, and partial-operation
contracts. Add copy, bounded read, buffered read/write, and line helpers. Adapt
file handles, then add path join/parent/name/components and filesystem
rename/copy/open-options/richer directory-entry operations.

Acceptance: algorithms operate over interfaces rather than file-specific
types; partial I/O is tested; path behavior is platform-neutral and capability
checked; filesystem tests cover race/error-sensitive operations without relying
on `exists` before action.

### P1 — Complete time, environment, arguments, and system information

Add duration and monotonic instant types, checked time arithmetic, sleep/timer
support, argv, cwd/executable/temp locations, environment remove/list, and
structured system information. Review each operation against the capability
model.

Acceptance: elapsed-time measurement does not use wall clock; durations are
unit-safe; command-line programs can inspect args and environment; grants are
narrow and tests can inject deterministic time/environment state.

### P1 — Replace placeholder process API with typed commands and child handles

Model executable, argument vector, environment/cwd, stdio policy, exit status,
captured output, spawn/wait/kill, and precise errors. Integrate child pipes with
stream interfaces and cancellation/deadline rules.

Acceptance: programs can run a command without shell-string parsing, inspect
status/stdout/stderr, stream pipes, and terminate a child; authority is
attenuated; unsupported target behavior is explicit.

### P1 — Replace placeholder networking API with typed socket resources

Define address parsing/resolution, TCP connect/listen/accept/stream operations,
UDP basics, deadlines, shutdown/close, and granular errors. Integrate streams
with common I/O and resource lifecycle contracts.

Acceptance: an echo client/server works under explicit grants; successful calls
return usable typed handles rather than `void`; DNS and direct-address authority
are reviewed; timeout and cleanup behavior is deterministic.

### P2 — Design cancellation, async host operations, and structured tasks

Specify cancellation propagation, task lifetime, failure/join behavior, and
interaction with capabilities and mutable references. Prototype async I/O and
structured tasks; evaluate typed channels. Do not commit to native threads or
shared-memory synchronization until Wasm target and alias-safety requirements
are documented.

Acceptance: design covers task leaks, cancellation, deadlines, capability
inheritance/attenuation, and mutable aliasing; a prototype demonstrates multiple
concurrent I/O operations with deterministic tests.

### P2 — Expand deterministic testing and benchmarking support

Add table/property-style helpers, seeded generators, fake clock/random sources,
temporary resource fixtures, and a benchmark contract once core collections and
iteration stabilize.

Acceptance: stdlib modules can test edge cases without ambient authority or
nondeterminism; benchmark output is machine-readable and does not change normal
test semantics.

## Explicit non-goals for the first foundation milestone

- Rust-equivalent ownership/borrowing or unsafe raw-pointer APIs.
- Every Rust collection or Go standard package.
- Native threads, mutexes, atomics, or shared-memory Wasm by default.
- HTTP/TLS, serialization formats, regular expressions, compression, database
  drivers, or cryptographic suites before streams/resources are stable.
- Calendar/time-zone APIs before duration and monotonic time are correct.

These may become valuable later, but none should block the low-to-mid-level
foundation described above.
