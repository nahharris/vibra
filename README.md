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

This parses the entry `.vibra` file, resolves `$import` **relative to that file’s directory** (Python-style), lowers stdlib-qualified calls from `$wasm` declarations, and executes them through the current runtime path. Argument forwarding is now explicit: call-site args are validated against stdlib signatures and forwarded into the declared `$wasm.args` contract.

**Preopens:** by default the embedded runner does **not** preopen host directories (stdio is enough for hello). Programs that use [`stdlib/fs.vibra`](stdlib/fs.vibra) need at least one preopened path; configure [`RunConfig::preopen_host_dirs`](src/runtime/wasi_env.rs) when embedding, or add CLI flags when the compiler exposes them.

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

# Filesystem roundtrip (requires preopen)
cargo run -- run examples/fs-roundtrip.vibra --preopen .
```

## Build & test

```sh
cargo build
cargo test
```

## License

MIT OR Apache-2.0 (see `Cargo.toml`).
