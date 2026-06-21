# Vibra

A vibe-coding-first programming language: **YAML** surface (strict subset), **static typing**, functional core, compiles to **WebAssembly**.

- **Specification:** [DRAFT.md](DRAFT.md)
- **Philosophy:** [PHILOSOPHY.md](PHILOSOPHY.md)
- **Schemas (tooling / LSP):** [schemas/](schemas/)
- **Examples:** [examples/](examples/)

## Run (MVP)

From the repo root (or any directory, using paths as you like—there is no required project layout):

```sh
cargo run -- run examples/hello.vibra
# After `cargo install --path .`:
vibra run examples/hello.vibra
```

This parses the entry `.vibra` file, resolves `$import` **relative to that file’s directory** (Python-style), lowers stdlib-qualified calls from `$wasm` declarations, and executes them through the current runtime path. Argument forwarding is explicit: call-site args are validated against stdlib signatures and forwarded into the declared `$wasm.args` contract.

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

The same structural model is exposed by [stdlib/code.vibra](stdlib/code.vibra)
through forms, typed key/index paths, revision-bound nodes, structural patterns
with captures, and every edit primitive. Recoverable operations return typed
result enums rather than aborting execution.

`vibra exec` remains available for evaluating a single Vibra expression:

```sh
vibra exec '"hello"' --format raw
```

Use `--arg name=value`, `--arg-file name=path`, and `--import alias=path` to
provide its explicit inputs.

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

`vibra fmt` and `vibra lint` are YAML-first tooling commands. Their default output is structured YAML for vibe-coding workflows; JSON and SARIF are opt-in compatibility formats for external automation.

```sh
vibra fmt                 # check every .vibra/.vibra.yaml file under .
vibra fmt src --write     # rewrite changed files in place
vibra fmt src --output json

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

`vibra init` creates `project.vibra`, target source files under `src/`, and a local stdlib copy under `dep/std`. Imports remain relative by default; imports beginning with `@` resolve through project targets or dependencies:

```yaml
io:
  $import: "@std/io.vibra"
core:
  $import: "@core/lib.vibra"
```

Use `vibra sync` to clone/fetch pinned git dependencies into `dep/<name>`, and `vibra check` to validate the manifest, targets, dependencies, and `@` imports:

```sh
vibra sync hello
vibra check hello
```

See [docs/project-layout.md](docs/project-layout.md) and [schemas/project-manifest.schema.json](schemas/project-manifest.schema.json).

Functions use canonical labeled declarations: `$function: $void` for zero arguments, `$function: $self` for a method receiver, or a singleton labeled mapping for the primary argument. Additional arguments use sibling `args:`, and function bodies reference every argument through `$args.<name>`.

**Policies:** authority is moving to unforgeable `$policy` values passed as normal function arguments. `main` receives the root policy value from the runtime, code explicitly narrows it with `$policy.narrow`, and privileged APIs consume the narrowed value through ordinary args. The current branch is migrating the stdlib from the previous grant side channel to this model.

```sh
vibra run examples/fs-roundtrip.vibra --allow-read=. --allow-write=.
```

Filesystem policy checks use canonical ancestry: a `dir` scope for `path/root` does not authorize a sibling such as `path/root2`.

**Known escape hatch:** arbitrary `$wasm` declarations are still accepted. The grant model currently applies to grant-aware stdlib APIs, not to untrusted modules that define their own `$wasm` shims. Future work should make `$wasm` trusted-stdlib-only or require an explicit unsafe/trust policy.

**Current subset:** entry module defines `main` with `args: $void`, `return: $void`, and a `do:` sequence of stdlib-qualified calls (including `$let` bindings of non-void returns and ordered `$match` sequence arms with explicit `case:` entries). Entry and imported modules may also define **user functions** (`do:` with `$let` / `$match` / `$return`) and **generic functions** (`$function` with the `=where` annotation declaring type parameters and bounds); generic calls pass explicit type arguments in the same mapping as value arguments (see [DRAFT.md](DRAFT.md)). `io` and `fs` functions declared in [stdlib/io.vibra](stdlib/io.vibra) and [stdlib/fs.vibra](stdlib/fs.vibra) are executable via the runtime execution backend.

## Type System Snapshot

- Primitive numerics: `$int8/$int16/$int32/$int64`, `$uint8/$uint16/$uint32/$uint64`, `$float32/$float64`
- Explicit annotations are required on function signatures (`args` + `return`)
- Algebraic unions are supported in lowering with direct syntax (`$union: [...]`, `$enum: {...}`, constructors, `$match`); optional values use the tagged `stdlib/option.vibra` enum because `$option` sugar and direct `$void` union members are rejected
- Value patterns use the single ordered-arm `$match: <expr>` plus sibling `when:` form; pattern variables are written as `{ $bind: name }`, wildcard as `{ $wildcard: null }`, and arm bindings remain local to the arm
- Generic functions and types declare type parameters via the `=where` annotation; call sites pass type params as keys alongside value args (e.g. `{ $f: { t: $int64, x: 7 } }`)
- `$newtype` creates nominal wrappers that require explicit `$cast` to cross to/from the inner type; transparent aliases still coerce implicitly, and other conversions use explicit `$from` / `$into` interface calls
- `=where` bounds (`t: [$some-iface, ...]`) are checked nominally against `=impl` blocks at call sites and type-position instantiations (`E-BOUND-001`)
- Inherent operations on a type live under its `=defs` annotation; explicit interface implementations live under `=impl` and use the reserved `$self` type to refer to the implementing type
- Interface methods can be invoked **type-qualified** (`$type.iface.method: { ... }`) or, when the method has a `$self`-typed argument, **interface-qualified** (`$iface.method: { x: $val, ... }`) -- the compiler dispatches on the static type of the `$self` argument
- Rust-inspired tagged enums available:
  - [stdlib/option.vibra](stdlib/option.vibra)
  - [stdlib/result.vibra](stdlib/result.vibra)

Import option and instantiate its generic type explicitly:

```yaml
option:
  $import: ./stdlib/option.vibra
maybe-name:
  $record:
    value:
      $option.option:
        t: $str
```

Construct values with `$option.option.some: "name"` or `$option.option.none`.
- `io`/`fs` APIs use nominal `path`, `bytes`, and file-mode types, with `readable`/`writable`/`appendable`/`closeable` interfaces to reject invalid file-mode operations
- Kebab-case is recommended for every symbol; non-kebab symbols emit warnings

## Examples

```sh
# Interactive stdin path
cargo run -- run examples/ask-name.vibra

# Filesystem roundtrip (requires grants)
cargo run -- run examples/fs-roundtrip.vibra --allow-read=. --allow-write=.
```

## Tests

`vibra test` discovers `.vibra` files under `tests/` and runs each top-level
`$test` declaration as an isolated test case. Test modules do not need `main`.

```yaml
test:
  $import: "@std/test.vibra"

truth:
  $test:
    do:
      - $test.assert: true
```

```sh
vibra test
vibra test --filter truth
vibra test --jobs 4 --timeout-ms 30000 --fail-fast
vibra test --report yaml --report-file report.yaml
```

Runtime permission flags match `vibra run`; pass `--allow-read`,
`--allow-write`, `--allow-env`, or `--allow-all` to grant test code access to
privileged stdlib APIs.

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
