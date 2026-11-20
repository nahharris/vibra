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

The project in [examples/hello](examples/hello) is a library with one
module and one test module. `src/hello/greeting.vib`:

```vibra
type Greeting
  case world
  case named {name: Str}
  visibility @public
endtype

fn message
  takes {greeting: Greeting}
  returns text: Str
  visibility @public
  do
    return match greeting
      case Greeting.world()
        "Hello, world!"
      case Greeting.named(name: name)
        Str.concat("Hello, " Str.concat(name "!"))
    endmatch
endfn
```

`Greeting` is a type with two cases, and `message` is a function: it takes
a greeting, returns text, and performs nothing. The `match` covers both
cases; leaving one out is an error the checker reports before anything runs.
Every form ends with its own closer, such as `endtype`, `endmatch`, and
`endfn`.

`tests/greeting.vib` imports the module and checks two results:

```vibra
use @std.assert enduse

use @hello.greeting enduse

test "greets the world"
  do
    assert.equal("Hello, world!" greeting.message(greeting.Greeting.world()))
endtest

test "greets by name"
  do
    assert.equal("Hello, Ada!" greeting.message(greeting.Greeting.named(name: "Ada")))
endtest
```

`project.vibon` names the package and its one target:

```vibon
{
  format: @project.v2
  package: {name: "hello" version: "0.1.0"}
  targets: [{name: @hello kind: @lib root: "src/hello"}]
  dependencies: [:]
}
```

A `.vibon` file is written with the same braces, brackets, and literals as
Vibra source, and holds data only.

## Try the pre-alpha

The language is specified in [the documentation](docs/index.md) and is being
implemented for [v0.0.2](docs/roadmap/v0.0.2/README.md). The
[v0.0.1 binaries](https://github.com/nahharris/vibra/releases) run the
earlier S-expression syntax, not this one.

To build the CLI from source with the pinned Rust toolchain:

```sh
cargo build --release --locked -p vibra-cli
```

Then, from `examples/hello`:

```sh
vibra check
vibra test
vibra fmt src/hello/greeting.vib
```

`vibra check` checks every target and test module, `vibra test` runs the
tests in the reference interpreter, and `vibra fmt PATH` previews the
canonical format of one file (`--write` applies it). `--format json` gives
each command's result as one JSON document. `vibra help` lists every command
and which ones this build supports; the others report that they are
unavailable. Operations, grants, and `vibra run` arrive with milestone 2 of
v0.0.2.
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
interpreter and static effects, in an S-expression syntax. That syntax is
not kept.

- [v0.0.2](docs/roadmap/v0.0.2/README.md) plans the redesigned language on
  the interpreter, grants for host operations, an agent repair loop, and a
  first native slice.
- [v0.0.3](docs/roadmap/v0.0.3/README.md) plans native executables on
  Windows, Linux, and macOS.
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
