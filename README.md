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

This parses the entry `.vibra` file, resolves `$import` **relative to that file’s directory** (Python-style), compiles a **small fixed subset** to `wasm32`, and executes it with **embedded Wasmer**. The host provides `env.println(ptr, len)` (UTF-8 in guest linear memory).

**Current subset:** entry module must define `main` as `$function` with empty `args`, `return: $void`, and `do:` containing exactly one call: `$alias.println: "literal string"`, where `alias` maps via `$import` to a module whose `println` matches the stub in [stdlib/io.vibra](stdlib/io.vibra). Anything else should produce a clear error until the compiler grows.

## Build & test

```sh
cargo build
cargo test
```

## License

MIT OR Apache-2.0 (see `Cargo.toml`).
