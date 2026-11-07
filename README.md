# Vibra

[![Status: pre-alpha](https://img.shields.io/badge/status-pre--alpha-cba6f7)](docs/roadmap/v1.md)
[![CI](https://github.com/nahharris/vibra/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/nahharris/vibra/actions/workflows/ci.yml)
[![Rust 1.96+](https://img.shields.io/badge/rust-1.96%2B-dea584?logo=rust)](https://www.rust-lang.org/)
[![License: MIT or Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE)

**A vibe-coding-first programming language.**

Vibra is a general-purpose language designed for software written and maintained
with coding agents. Its syntax, types, effects, tools, and runtime are designed
together to help agents make changes that people can understand and trust.

Vibra is in **pre-alpha**. You can write and run small programs, but the
language and tooling are still evolving and are not ready for production.

[Read the introduction](https://www.harrisonn.dev/series/vibra/introducing-vibra/) ·
[Explore the documentation](docs/index.md) ·
[See the roadmap](docs/roadmap/v1.md) ·
[Releases](https://github.com/nahharris/vibra/releases)

## ✨ Why Vibra?

Coding agents can write a lot of code, but the people using that code still
need to understand what it does and whether it meets their intent. Vibra aims
to make that work easier by putting more information in the program and making
it available to both the compiler and the tools an agent uses.

- **Explicit types and failures.** Function signatures describe inputs,
  outputs, and possible failures. `result` and `option` make failure and
  absence part of the program's data.
- **Static effects.** Functions declare the kinds of operations they may
  perform, such as reading a file or writing to the console. The compiler
  checks those declarations and the effects allowed by a project target.
- **Functional by design.** Vibra is expression-oriented, with first-class
  functions and immutable values. Pure computations can be composed without
  introducing effects.
- **Fewer unnecessary choices.** Explicit boundaries and a canonical
  formatter give people and agents a consistent language to work with.
- **Structured tooling.** Machine-readable diagnostics and workspace queries
  are part of the design. Richer agent-facing queries, transactional edits,
  and MCP support are still ahead on the roadmap.

These checks catch some mistakes before execution. They do not establish that
an agent understood a request, prove a program correct, or sandbox its runtime.
Effects are checked statically; they are not runtime grants.

## 👋 Hello, Vibra

```lisp
(import io @std.io)

(defn main () (result void io.io-error)
  effects: (io.stdout)
  (io.stdout.text "Hello, world!\n"))
```

`io.stdout.text` can fail, so `main` returns a `result` containing either a
successful `void` value or an `io.io-error`. It also declares the `io.stdout`
effect. The project's binary target must consent to that effect too:

```lisp
(record
  format: @project.v1
  package: (record name: "hello" version: "0.1.0")
  targets: (array
    (record
      name: @app
      kind: @bin
      root: "src/app"
      entry: @app.main.main
      effects: (array @std.io.stdout)))
  dependencies: (dict))
```

Save the program as `src/app/main.vib` and the project description as
`project.vibon`. Vibra uses S-expressions for source and non-executable data,
with `.vib` and `.vibon` serving distinct roles.

## Try the pre-alpha

Find binaries on the [releases page](https://github.com/nahharris/vibra/releases),
or build the CLI from source using the pinned Rust toolchain:

```sh
cargo build --release --locked -p vibra-cli
```

With `vibra` on your path, create a starter project:

```sh
vibra project init hello
cd hello
vibra check
vibra run
```

The starter contains an empty `main` function. To try the hello world above,
replace the generated project description with the example `project.vibon`
and save the example source as `src/app/main.vib`.

You can also check and run an existing project from its directory:

```sh
vibra check
vibra run
```

The repository also contains [example projects](examples/), including a
[console example](examples/m4-console/) and a [file-processing demo](examples/m4-demo/).
The CLI currently supports project initialization, formatting, checking,
running, and testing. Use `vibra help` to see the available commands.

## 🧭 Where the project stands

The current implementation includes a reader and formatter, a nominal static
type system, an interpreter, a WebAssembly backend, static effect checking,
and host operations for console, filesystem, environment, clock, and random
access. It also has the beginnings of a standard library and workspace tooling.

Vibra is developed specification-first. The [charter](docs/spec/00-charter.md)
and [language specifications](docs/index.md) define the target; the
[roadmap](docs/roadmap/v1.md) tracks the work toward a usable toolchain.
Reproducible dependencies, fuller agent-facing tooling, and WebAssembly build
products are still to come. The [post-v1 direction](docs/roadmap/beyond-v1/README.md)
explores ideas such as verification, concurrency, and interactive applications.

An earlier implementation is preserved in [the archive](archive/pre-v1/) as
part of the project's history. The current implementation is a fresh start and
does not promise compatibility with that version.

## License

Vibra is licensed under the terms in [LICENSE](LICENSE).
