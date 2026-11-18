# Vibra

[![Status: pre-alpha](https://img.shields.io/badge/status-pre--alpha-cba6f7)](docs/roadmap/README.md)
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
[See the roadmap](docs/roadmap/README.md) ·
[Releases](https://github.com/nahharris/vibra/releases)

## ✨ Why Vibra?

Coding agents can write a lot of code, but the people using that code still
need to understand what it does and whether it meets their intent. Vibra aims
to make that work easier by putting more information in the program and making
it available to both the compiler and the tools an agent uses.

- **Explicit types and failures.** Signatures describe inputs, outputs, and
  possible failures. `Result` and `Option` make failure and absence part of
  the program's data, and nothing a program computes can crash it.
- **Contracts checked before the program runs.** A `requires`, `ensures`, or
  `where` is either proved by the checker or written with an `else` that
  states the value returned when it does not hold.
- **Effects you can read.** A `proc` lists the operations it may perform in
  `performs {…}`. A `fn` performs nothing. The compiler checks both, and the
  project file lists everything a program may do.
- **Authority is a value.** Reading a directory needs a grant for that
  directory, passed as an ordinary parameter. The project file says which
  folder, which files, and which actions; the runtime enforces it.
- **Functional by design.** Values are immutable, functions are first-class,
  and there are no loops or mutable variables.
- **Fewer unnecessary choices.** One spelling per construct, typed closers
  such as `endfn` and `endif`, and a canonical formatter give people and
  agents a consistent language to work with.
- **Structured tooling.** Diagnostics, queries, evaluation, tests, and edits
  have machine-readable forms designed for agents.

These checks catch some mistakes before execution. They do not establish that
an agent understood a request or prove a program correct. Grants limit what
Vibra code can do through the runtime; they are not an operating-system
sandbox.

## 👋 Hello, Vibra

```vibra
use @std.io.StdoutText enduse
use @std.process.ExitCode enduse

proc main
  takes {out: StdoutText}
  returns code: ExitCode
  performs {StdoutText}
  do
    match await StdoutText.write(out "Hello, world!\n")
      case Result.ok(value: _)
        return ExitCode(0u8)
      case Result.err(error: _)
        return ExitCode(1u8)
    endmatch
endproc
```

`main` takes `out`, a grant to write text to standard output, and declares
in `performs` that writing is the one operation it may perform. The write can
fail, so its result is a `Result` and both cases are handled.

The project file says where that grant comes from:

```vibon
{
  format: @project.v2
  package: {name: "hello" version: "0.1.0"}
  targets: [
    {
      name: @app
      kind: @bin
      root: "src/app"
      entry: @app.main.main
      effects: [@std.io.StdoutText]
      grants: [
        @out: {
          provider: @host.io.stdout
          resource: {kind: @processStream}
          after: []
        }
      ]
    }
  ]
  dependencies: [:]
}
```

Save the program as `src/app/main.vib` and the project description as
`project.vibon`. A `.vibon` file is written with the same braces, brackets,
and literals as Vibra source, and holds data only.

## Try the pre-alpha

The language was redesigned after v0.0.1, and this page describes the
redesign. It is specified in [the documentation](docs/index.md) and is being
implemented for [v0.0.2](docs/roadmap/v0.0.2/README.md). The
[v0.0.1 binaries](https://github.com/nahharris/vibra/releases) and the code
on `main` still run the earlier S-expression syntax, so the example above
does not run on them yet.

To build the CLI from source with the pinned Rust toolchain:

```sh
cargo build --release --locked -p vibra-cli
```

The commands are the same in both: `vibra project init`, `vibra fmt`,
`vibra check`, `vibra test`, and `vibra run`. Use `vibra help` to see what
your build supports.
### Skills for coding agents

This repository is a plugin marketplace for Claude Code, Cursor, and Codex.
The `vibra` plugin ships two skills. `vibra-language` covers writing Vibra
source, and `vibra-toolchain` covers driving the `vibra` CLI.

- **Claude Code:** `/plugin marketplace add nahharris/vibra`, then
  `/plugin install vibra@vibra`
- **Codex:** `codex plugin marketplace add nahharris/vibra`, then
  `codex plugin add vibra@vibra`
- **Cursor:** add `nahharris/vibra` as a plugin marketplace from a GitHub
  repository, then install `vibra`
- **Any agent, through [`npx skills`](https://github.com/vercel-labs/skills):**
  `npx skills add nahharris/vibra`. Add `--skill vibra-language` to pick one
  skill, `-a claude-code` (or `codex`, `cursor`, ...) to pick an agent, and
  `-g` to install for every project

## 🧭 Where the project stands

Vibra is developed specification-first. The [charter](docs/spec/00-charter.md)
and [language specifications](docs/index.md) define the language contracts.

[v0.0.1](docs/roadmap/v0.0.1/README.md) shipped a checked language with an
interpreter, static effects, and a WebAssembly backend, in an S-expression
syntax. That syntax is not kept.

- [v0.0.2](docs/roadmap/v0.0.2/README.md) plans the redesigned language on
  the interpreter, grants for host operations, an agent repair loop, and a
  first native slice.
- [v0.0.3](docs/roadmap/v0.0.3/README.md) plans native executables on
  Windows, Linux, and macOS, and removes WebAssembly.
- [v0.0.4](docs/roadmap/v0.0.4/README.md) plans dependencies, tool adapters,
  and bounded local concurrency.
- [v0.0.5](docs/roadmap/v0.0.5/README.md) plans service behavior and
  recovery.

The [scratchpad](docs/roadmap/scratchpad/README.md) collects unscheduled
proposals.
An earlier implementation is preserved in [the archive](archive/pre-v1) as
part of the project's history. The current implementation is a fresh start and
does not promise compatibility with that version.

## License

Vibra is licensed under the terms in [LICENSE](LICENSE).
