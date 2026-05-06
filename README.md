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

**Current subset:** entry module defines `main` with `args: $void`, `return: $void`, and a `do:` sequence of stdlib-qualified calls (including `$let` bindings of non-void returns and `$match` over unions). `io` and `fs` functions declared in [stdlib/io.vibra](stdlib/io.vibra) and [stdlib/fs.vibra](stdlib/fs.vibra) are executable via the runtime execution backend.

## Type System Snapshot

- Primitive numerics: `$int8/$int16/$int32/$int64`, `$uint8/$uint16/$uint32/$uint64`, `$float32/$float64`
- Explicit annotations are required on function signatures (`args` + `return`)
- Algebraic unions are supported in lowering with direct syntax (`$union: [...]`, `$enum: {...}`, constructors, `$match`)
- Rust-inspired unions available:
  - [stdlib/option.vibra](stdlib/option.vibra)
  - [stdlib/result.vibra](stdlib/result.vibra)
- `io`/`fs` APIs currently use raw primitives (`$int64` and `$str`) while the type system is still early
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
