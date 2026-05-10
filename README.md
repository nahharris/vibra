# Vibra

A vibe-coding-first programming language: **YAML** surface (strict subset), **static typing**, functional core, compiles to **WebAssembly**.

- **Specification:** [DRAFT.md](DRAFT.md)
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

## Exec

`vibra exec` evaluates one inline Vibra expression and writes the result to stdout. It auto-imports [stdlib/code.vibra](stdlib/code.vibra) as `code`, so tooling can parse and rewrite Vibra source without patch files:

```sh
vibra exec '"hello"' --format raw
vibra exec '{$code.get: {$code.parse: $src}, path: "/main/do/0/$io.println"}' --arg-file src=examples/hello.vibra --format raw
```

Use `--arg name=value` for string bindings, `--arg-file name=path` for file contents, and `--import alias=path` for additional modules. Code paths use JSON Pointer strings; `code.set`, `code.remove`, and `code.append` return a new document string and preserve comments/formatting where the editor can attach them.

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

**Permissions and grants:** privileged stdlib APIs take explicit grant arguments. The runtime mints grants from CLI consent flags and exposes them through `main` when it declares `args: { grants: $security.grants }`. Each grant field is matchable as `granted` or `denied`, so programs can degrade behavior when access is unavailable. Default policy is deny for privileged actions; stdout/stderr output remains baseline for CLI usability.

```sh
vibra run examples/fs-roundtrip.vibra --allow-read=. --allow-write=.
```

Filesystem grants use canonical ancestry checks; `--allow-read path/root` does not authorize a sibling like `path/root2`. The legacy `--preopen` flag remains as a compatibility alias that seeds both read and write filesystem grants for the embedded interpreter.
Grants can also be attenuated before delegation; for example `fs.narrow-read` and `fs.narrow-write` derive child grants scoped to a subpath without widening the original authority.

**Known escape hatch:** arbitrary `$wasm` declarations are still accepted. The grant model currently applies to grant-aware stdlib APIs, not to untrusted modules that define their own `$wasm` shims. Future work should make `$wasm` trusted-stdlib-only or require an explicit unsafe/trust policy.

**Current subset:** entry module defines `main` with `args: $void`, `return: $void`, and a `do:` sequence of stdlib-qualified calls (including `$let` bindings of non-void returns and ordered `$match` sequence arms with explicit `pattern:` entries). Entry and imported modules may also define **user functions** (`do:` with `$let` / `$match` / `$return`) and **generic functions** (`$function` with the `=where` annotation declaring type parameters and bounds); generic calls pass explicit type arguments in the same mapping as value arguments (see [DRAFT.md](DRAFT.md)). `io` and `fs` functions declared in [stdlib/io.vibra](stdlib/io.vibra) and [stdlib/fs.vibra](stdlib/fs.vibra) are executable via the runtime execution backend.

## Type System Snapshot

- Primitive numerics: `$int8/$int16/$int32/$int64`, `$uint8/$uint16/$uint32/$uint64`, `$float32/$float64`
- Explicit annotations are required on function signatures (`args` + `return`)
- Algebraic unions are supported in lowering with direct syntax (`$union: [...]`, `$enum: {...}`, constructors, `$match`)
- Value patterns use the single ordered-arm `$match` form; pattern variables are written as `{ $bind: name }`, wildcard as `{ $wildcard: null }`, and arm bindings remain local to the arm
- Generic functions and types declare type parameters via the `=where` annotation; call sites pass type params as keys alongside value args (e.g. `{ $f: { t: $int64, x: 7 } }`)
- `$newtype` creates nominal wrappers that require explicit `$cast` to cross to/from the inner type; transparent aliases still coerce implicitly
- `=where` bounds (`t: [$some-iface, ...]`) are checked nominally against `=impl` blocks at call sites and type-position instantiations (`E-BOUND-001`)
- Inherent operations on a type live under its `=defs` annotation; explicit interface implementations live under `=impl` and use the reserved `$self` type to refer to the implementing type
- Interface methods can be invoked **type-qualified** (`$type.iface.method: { ... }`) or, when the method has a `$self`-typed argument, **interface-qualified** (`$iface.method: { x: $val, ... }`) -- the compiler dispatches on the static type of the `$self` argument
- Rust-inspired unions available:
  - [stdlib/option.vibra](stdlib/option.vibra)
  - [stdlib/result.vibra](stdlib/result.vibra)
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
