# Vibra

A vibe-coding-first programming language: **YAML** surface (strict subset), **static typing**, functional core, compiles to **WebAssembly**.

- **Specification:** [DRAFT.md](DRAFT.md)
- **Philosophy:** [PHILOSOPHY.md](PHILOSOPHY.md)
- **Schemas and tooling contracts:** [below](#schemas-and-tooling-contracts)
- **Examples:** [examples/](examples/)
- **[Container images](docs/containers.md)**
- **[Native toolchain archives](docs/distribution.md)**

## Run (MVP)

From the repo root (or any directory, using paths as you like—there is no required project layout):

```sh
cargo run -- run examples/hello.vibra
# After `cargo install --path .`:
vibra run examples/hello.vibra
```

This parses the entry `.vibra` file, resolves `$import` **relative to that file’s directory** (Python-style), lowers stdlib-qualified calls from `$wasm` declarations, and executes them through the current runtime path. Argument forwarding is explicit: call-site args are validated against stdlib signatures and forwarded into the declared `$wasm.args` contract.

Source execution emits and enters a deterministic WebAssembly module through the versioned `vibra_v1` host boundary. See [the ABI design](docs/wasm-abi.md) for the entrypoint, capability, value-layout, import-validation, and compatibility contract.

Project dependencies may also declare statically resolved WebAssembly
libraries behind explicit typed `$wasm` wrappers. The supported scalar and
caller-owned-buffer ABI is specified in [the static wasm FFI design](docs/static-wasm-ffi.md).

## Structural code tools

`vibra code` queries and transactionally edits project-owned Vibra files through
typed structural paths. Pipelines can be inline, read from stdin, or loaded from
a file:

```sh
vibra code '- $code.file: src/main.vibra
- $code.at: [main, do, 0, "$io.println"]
- $code.replace: Changed'

vibra code - < refactor.vibra
vibra code --file refactor.vibra --write
```

Path strings select mapping keys and non-negative integers select sequence
indices. Query pipelines print YAML. Editing pipelines preview a structured
report and unified diff; `--write` rechecks source revisions and applies every
changed file atomically.

Available stages include file/path navigation, children/parent traversal,
structural find and projection, save/load, replace/delete, mapping
insert/upsert/rename, sequence insert/splice, copy/move, and workspace-wide
symbol or import-alias rename.

The same structural model is exposed by [stdlib/src/code.vibra](stdlib/src/code.vibra)
through forms, typed key/index paths, revision-bound nodes, structural patterns
with captures, and every edit primitive. Recoverable operations return typed
result enums rather than aborting execution.

`vibra exec` remains available for evaluating a single Vibra expression:

```sh
vibra exec '"hello"' --format raw
```

Use `--arg name=value`, `--arg-file name=path`, and `--import alias=path` to
provide its explicit inputs.

## Schemas and tooling contracts

The JSON Schema files in [`schemas/`](schemas/) describe the machine-readable
Vibra surface used by editors, LSP clients, structural-code tooling, and
automation. They complement the language rules in [DRAFT.md](DRAFT.md); they
are not a complete specification of compiler behavior.

- **Source surface:** [`module-surface.schema.json`](schemas/module-surface.schema.json),
  [`function.schema.json`](schemas/function.schema.json),
  [`macro.schema.json`](schemas/macro.schema.json),
  [`type-expr.schema.json`](schemas/type-expr.schema.json),
  [`expression.schema.json`](schemas/expression.schema.json), and
  [`source-annotations.schema.json`](schemas/source-annotations.schema.json).
- **Projects, packages, and diagnostics:** [`project-manifest.schema.json`](schemas/project-manifest.schema.json),
  [`project-lock.schema.json`](schemas/project-lock.schema.json),
  [`dependency-resolution.schema.json`](schemas/dependency-resolution.schema.json),
  [`package-manifest.schema.json`](schemas/package-manifest.schema.json),
  [`release-metadata.schema.json`](schemas/release-metadata.schema.json),
  [`diagnostic.schema.json`](schemas/diagnostic.schema.json), and the stable
  code registry in [`linter-codes.json`](schemas/linter-codes.json).
- **Structural code:** [`code-form.schema.json`](schemas/code-form.schema.json),
  [`code-path.schema.json`](schemas/code-path.schema.json),
  [`code-query.schema.json`](schemas/code-query.schema.json), and
  [`code-change-set.schema.json`](schemas/code-change-set.schema.json).
- **Editor and documentation queries:** [`query-response.schema.json`](schemas/query-response.schema.json)
  specifies the `vibra/contextAt` (and `vibra query`) response shape, and
  [`docs-response.schema.json`](schemas/docs-response.schema.json) specifies
  `vibra docs --format yaml|json`.
- **Async conformance:** [`async-task-trace.schema.json`](schemas/async-task-trace.schema.json)
  defines deterministic structured-task traces. The semantics, implementation
  milestones, and normative vectors are documented in
  [`docs/async-structured-concurrency.md`](docs/async-structured-concurrency.md).

Each schema has a canonical `$id` under `https://vibra.dev/schemas/`. Tooling
should use the schema that matches its boundary and treat the expression and
module-surface schemas as deliberately permissive where contextual compiler
validation is required.

## Macros

Function-shaped `$macro` declarations expand after import resolution and before
normal lowering/typechecking. `$quote`, `$unquote`, and sequence `$splice`
construct syntax; generated bindings are hygienic, while `$capture` explicitly
requests caller-scoped syntax.

```yaml
identity:
  $macro:
    input: $code.expr-syntax
  return: $code.expr-syntax
  do:
    - $return:
        $quote:
          $unquote: $args.input
```

Macro execution is deterministic and limited to 64 nested expansions,
1,000,000 evaluation steps, and 100,000 generated nodes per module load.
`vibra expand path/to/module.vibra` prints the canonical expanded module.

## Format and lint

Vibra's CLI is YAML-first: every command that emits CLI-owned output uses structured YAML by default. Use `--format yaml` to request it explicitly, or select a supported alternative such as `json`, `human`, `raw`, or `sarif`. Program-owned stdout from `vibra run` is passed through unchanged.

`vibra fmt` and `vibra lint` follow the same convention. JSON and SARIF remain opt-in compatibility formats for external automation.

```sh
vibra fmt                 # check every .vibra/.vibra.yaml file under .
vibra fmt src --write     # rewrite changed files in place
vibra fmt src --format json

vibra lint
vibra lint src --category style
vibra lint src --format json
vibra lint src --format sarif
vibra lint src --deny-warnings
```

`vibra fmt` is check-only by default. It exits `0` when all files are canonical, exits `1` when check mode finds formatting drift, and only mutates files with `--write`.

`vibra lint` emits diagnostics with stable codes, severity, and spans matching [schemas/diagnostic.schema.json](schemas/diagnostic.schema.json). Warning-only lint runs exit `0` unless `--deny-warnings` is set. Errors always fail. YAML `#` comments are forbidden; use structural annotations:

```yaml
BadName:
  =comment: This external name is intentionally preserved.
  =lint:
    disable: [W-STYLE-001]
  $literal: 1
```

`=comment` is ignored by compilation. `=lint` applies to its mapping and
descendants and cannot suppress syntax or compiler errors.

## Projects

`project.vibra` is the canonical project manifest. New projects can be scaffolded with:

```sh
vibra init hello
vibra init hello --template lib
vibra init hello --template workspace
```

`vibra init` creates `project.vibra`, target source files under `src/`, and an offline stdlib seed under `dep/std`. The manifest records `std` as an exact-revision Git dependency on [nahharris/vibra-stdlib](https://github.com/nahharris/vibra-stdlib); `vibra sync` can reproduce that same tree. Imports remain relative by default; imports beginning with `@` resolve through project targets or dependencies and honor a dependency's declared library source root:

```yaml
io:
  $import: "@std/io.vibra"
core:
  $import: "@core/lib.vibra"
```

Use `vibra sync` to export exact Git revisions recursively into package-local `dep/<name>` trees and write the committed `project.lock.vibra`. Exported trees contain no Git metadata. The lock records identities, exact revisions, SHA-256 tree hashes, vendor paths, and alias edges. `vibra check` validates the lock, rejects modified vendor trees, and then validates targets, dependencies, and `@` imports:

```sh
vibra sync hello
vibra check hello
```

See [docs/project-layout.md](docs/project-layout.md) and [schemas/project-manifest.schema.json](schemas/project-manifest.schema.json).
The proposed post-v1 deterministic source version solver is specified in
[docs/version-solving.md](docs/version-solving.md); it does not change today's
exact-revision workflow.

### Compile-time file embedding

`$embed` turns a package-owned file into a Vibra expression while loading the
module. A string value uses the file extension to select `txt`, `bin`, `yaml`,
`json`, `toml`, or `xml`; use the mapping form when the extension is ambiguous:

```yaml
message: {$embed: assets/message.txt}
logo: {$embed: {path: assets/logo.dat, format: binary}}
settings: {$embed: {path: assets/settings.conf, format: toml}}
```

Text produces `$str`, binary produces `$array<$uint8>`, and structured formats
produce statically typed records and arrays. Paths are relative to the module,
must be normalized, and cannot leave the nearest package root (including via a
symlink). Embedded raw bytes and their package-relative paths participate in
the deterministic compiler fingerprint; no runtime filesystem grant is needed.
Malformed content uses `E-EMBED-004`, invalid structured shapes use
`E-EMBED-005`, and path/sandbox failures use `E-EMBED-002`/`E-EMBED-003`.

### Symbol documentation

Use `vibra docs` to read compile-time `=doc` annotations without running a
program. Pass a source module or a project directory, followed by an optional
qualified symbol. Plain output is the default; Markdown preserves examples and
other formatting from the annotation, while YAML and JSON are intended for
tooling.

```sh
vibra docs src/app/main.vibra main
vibra docs src/app/main.vibra io.println --format markdown
vibra docs src/app/main.vibra --format yaml
vibra docs . --target app --format json
```

With no symbol, the command lists every documented package, module, function,
type, interface, constant, macro, and inherent definition visible from the
selected entry module. Prefixing a lookup with `$` is optional. Package docs use
`package.=doc` in `project.vibra`; module docs use a top-level `=doc`. Imported
symbols are addressed through their import alias, such as `io.println`.

The command reports source documentation and does not execute macros beyond the
compiler's normal load-time expansion. Generated implementation methods that
have no source `=doc` are not listed. The structured documentation records are
also exposed to editor tooling.

### Language server

Run `vibra lsp` and configure an editor to communicate with the process over
standard input/output. The server implements LSP lifecycle requests,
full-document synchronization, syntax/style diagnostics for unsaved buffers,
canonical formatting, and local-document hover, definition, reference, and
completion requests. Hover uses `=doc` text.

Navigation and completion are currently top-level and local to the open
document; they do not resolve imported symbols or infer expression types.
Compile diagnostics continue to use the saved workspace. The advertised
capability shape is documented by
[`schemas/lsp-capabilities.schema.json`](schemas/lsp-capabilities.schema.json).

### Executable application packages

`vibra build` produces a deterministic `.vapp` ZIP containing the selected bin's
`program.wasm`, its complete project and vendored dependency source graph, and a
SHA-256 inventory in `package.vibra`. Timestamps, permissions, compression, and
entry ordering are fixed, so identical inputs produce identical bytes:

```sh
vibra build hello --output hello.vapp
vibra package inspect hello.vapp
vibra package verify hello.vapp
vibra run hello.vapp
```

Use `--bin <name>` when a project declares multiple bin targets. Verification
rejects missing, modified, duplicate, undeclared, or path-unsafe entries before
execution. The metadata contract is documented by
[`schemas/package-manifest.schema.json`](schemas/package-manifest.schema.json).

The compiler repository pins that same stdlib revision as the `stdlib` Git submodule. Clone contributors' checkouts with `git clone --recurse-submodules`, or initialize an existing checkout with `git submodule update --init --recursive`.

Functions use canonical labeled declarations: `$function: $void` for zero arguments, `$function: $self` for a method receiver, or a singleton labeled mapping for the primary argument. Additional arguments use sibling `args:`, and function bodies reference every argument through `$args.<name>`.

**Policies:** authority roots declare aggregate `$policy` arguments. The runtime
intersects those declarations with `--allow-*` approvals, then code explicitly
narrows the live value with `$policy.narrow` into a typed
`$capability.<domain>` argument for privileged helpers.

```sh
vibra run examples/fs-roundtrip.vibra --allow-read=. --allow-write=.
```

Filesystem policy checks use canonical ancestry: a `dir` scope for `path/root` does not authorize a sibling such as `path/root2`.

**Host ABI:** `$wasm` is a checked binding to the closed `vibra_v1` registry,
not an authority escape hatch. The compiler validates the module/import pair,
every argument and capability domain, and the exact return type. Run
`vibra effects <path>` to inspect the reachable host surface without executing it.

**Current subset:** entry module defines `main` with `args: $void`, `return: $void`, and a `do:` sequence of stdlib-qualified calls (including `$let` bindings of non-void returns and ordered `$match` sequence arms with explicit `case:` entries). Canonical `$for` traversal covers half-open signed integer `$range` values, arrays, insertion-ordered string-key maps, and Unicode scalar values from strings; `$break: null` and `$continue: null` target the nearest loop. Entry and imported modules may also define **user functions** (`do:` with `$let` / `$match` / `$return`) and **generic functions** (`$function` with the `=where` annotation declaring type parameters and bounds); generic calls pass explicit type arguments in the same mapping as value arguments (see [DRAFT.md](DRAFT.md)). `io` and `fs` functions declared in [stdlib/src/io.vibra](stdlib/src/io.vibra) and [stdlib/src/fs.vibra](stdlib/src/fs.vibra) are executable via the runtime execution backend.

Pure collection operations live in [stdlib/src/collections.vibra](stdlib/src/collections.vibra): generic arrays support safe lookup and copy-on-return updates, while deterministic string-key maps preserve insertion order and use explicit `option`/`result` outcomes.

Text and conversion foundations live in `stdlib/src/text.vibra`,
`stdlib/src/bytes.vibra`, and `stdlib/src/convert.vibra`. `$str` is valid UTF-8:
string traversal and text offsets count Unicode scalar values, while explicitly
named byte operations count UTF-8 bytes. Decoding arbitrary bytes and parsing
primitives are fallible typed operations; formatting is deterministic and
locale-free (`nan`, `inf`, and `-inf` are the canonical float spellings).
Potentially growing text/byte operations honor the runtime allocation limit.

Time, environment, and system helpers remain explicitly capability-gated.
`time` distinguishes unit-safe `duration` values from monotonic `instant`
values, provides checked addition/elapsed arithmetic, and never uses wall time
for elapsed measurement. `env.list` filters its deterministic name list through
the supplied `env-read` scopes, while get/set/remove check the requested name.
`sys` exposes configured program arguments, current/executable/temp locations,
and structured operating-system, architecture, and family fields under the
existing `system-info` grant.

Networking uses typed `net.address`, `tcp-stream`, `tcp-listener`, and
`udp-socket` resources. Address parsing is pure; hostname resolution and every
outbound TCP/UDP target require an explicit matching `net-connect` scope, while
bind operations require `net-listen`. TCP supports connect/listen/accept,
bounded byte I/O, deadlines, shutdown, and deterministic close; UDP supports
bind/connect/send-to/receive-from. All socket resources share the instance host
resource limit and are reclaimed at instance teardown. Until cross-module
interface method forwarding is supported, `net.stream-read`, `stream-write`,
and `close-*` expose the common fd behavior directly rather than claiming an
unusable `$fs.readable`/`writable`/`closeable` implementation.

## Type System Snapshot

- Primitive numerics: `$int8/$int16/$int32/$int64`, `$uint8/$uint16/$uint32/$uint64`, `$float32/$float64`
- Canonical primitive intrinsics provide checked integer arithmetic, IEEE float arithmetic, comparisons, boolean operations, bitwise operations, bounded shifts, and explicit non-trapping exact numeric conversion with a typed fallback. Operands must have the same type; Vibra never widens or narrows them implicitly.
- Explicit annotations are required on function signatures (`args` + `return`)
- Algebraic unions are supported in lowering with direct syntax (`$union: [...]`, `$enum: {...}`, constructors, `$match`); optional values use the tagged `stdlib/src/option.vibra` enum because `$option` sugar and direct `$void` union members are rejected
- Value patterns use the single ordered-arm `$match: <expr>` plus sibling `when:` form; pattern variables are written as `{ $bind: name }`, wildcard as `{ $wildcard: null }`, and arm bindings remain local to the arm
- Generic functions and types declare type parameters via the `=where` annotation; call sites pass type params as keys alongside value args (e.g. `{ $f: { t: $int64, x: 7 } }`)
- `$newtype` creates nominal wrappers that require explicit `$cast` to cross to/from the inner type; transparent aliases still coerce implicitly, and other conversions use explicit `$from` / `$into` interface calls
- `=where` bounds (`t: [$some-iface, ...]`) are checked nominally against `=impl` blocks at call sites and type-position instantiations (`E-BOUND-001`)
- Inherent operations on a type live under its `=defs` annotation; explicit interface implementations live under `=impl` and use the reserved `$self` type to refer to the implementing type
- Interface methods can be invoked **type-qualified** (`$type.iface.method: { ... }`) or, when the method has a `$self`-typed argument, **interface-qualified** (`$iface.method: { x: $val, ... }`) -- the compiler dispatches on the static type of the `$self` argument
- Rust-inspired tagged enums available:
  - [stdlib/src/option.vibra](stdlib/src/option.vibra)
  - [stdlib/src/result.vibra](stdlib/src/result.vibra)

Import option and instantiate its generic type explicitly:

```yaml
option:
  $import: ./stdlib/src/option.vibra
maybe-name:
  $record:
    value:
      $option.option:
        t: $str
```

Construct values with `$option.option.some: "name"` or `$option.option.none`.
Both tagged types provide `is-*` queries and `unwrap-or` defaulting. `option`
also provides `and`/`or`; `result` provides `and`/`or` plus conversions to its
ok/error options. These combinators are eager and effect-transparent: arguments
are evaluated before the call. Vibra does not yet have first-class function
values, so callback combinators such as `map` and `and-then` are intentionally
not exposed with a misleading file-specific or dynamically typed callback.

Error propagation is explicit with `$match`: return the unchanged
`$result.result.err` payload from the error arm and continue from the ok arm.
This preserves the exact error type and structured context and keeps effects
visible. There is currently no implicit `?`-style control-flow form.

- `io`/`fs` APIs use nominal paths and file-mode handles with reusable
  `readable`/`writable`/`closeable` stream interfaces. Bounded reads use an
  empty successful chunk for EOF, partial writes report progress, and file
  rename/copy/open options and typed directory entries remain capability gated
- `process` uses a typed command record containing an explicit executable,
  argument array, environment, cwd, and stdio policy without shell parsing;
  capture/inherit/null execution returns typed status and output, and
  exposes instance-owned child handles with typed wait/kill outcomes; stream
  mode provides child pipes through the shared file reader/writer interfaces.
  `run` rejects stream mode (use `spawn`), and live children are terminated and
  reaped when their program instance ends
- Kebab-case is recommended for every symbol; non-kebab symbols emit warnings

## Examples

```sh
# Interactive stdin path
cargo run -- run examples/ask-name.vibra

# Filesystem roundtrip (requires explicit approval)
cargo run -- run examples/fs-roundtrip.vibra --allow-read=. --allow-write=.
```

## Tests

`vibra test` discovers `.vibra` files under `tests/` and runs each top-level
`$test` declaration as an isolated test case. Test modules do not need `main`.
The `$test` value is a non-empty kebab-case profile; a bare `vibra test` runs
the capability-free `core` profile.

```yaml
test:
  $import: "@std/test.vibra"

truth:
  $test: core
  do:
    - $test.assert: true
```

```sh
vibra test
vibra test --filter truth
vibra test --profile core --tag language
vibra test --deny-skips --deny-warnings
vibra test --jobs 4 --timeout-ms 30000 --fail-fast
vibra test --format yaml --report-file report.yaml
```

Profiles and tags only select tests; they never confer host permissions.
Capability tests declare sibling `policy` and must be run with the matching
explicit `--allow-*` flag. `workspace: temp` tests additionally need
`--allow-test-workspace read`, `write`, or `read-write`; without it, they are
reported as skipped. See [`tests/README.md`](tests/README.md) for expected
errors, typed assertion helpers, profile contracts, and the complete flag
reference.

Files named `foo.*.vibra` are loaded as parts of the same module as
`foo.vibra` when `foo.vibra` exists. A common convention is to place unit
tests beside the module in `foo.test.vibra`; the suffix is only a naming
convention and does not carry special semantics.

## Build & test

```sh
cargo build
cargo test
```

## License

MIT OR Apache-2.0 (see `Cargo.toml`).
