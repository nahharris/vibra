---
name: vibra-toolchain
description: Build and drive the Vibra pre-alpha toolchain, the `vibra` CLI. Use to build `vibra`, lay out a project (`project.vibon`, `src/`, `tests/`), and run `vibra fmt`, `vibra check`, and `vibra test`. It also covers the JSON envelope, diagnostics, exit codes, and fixing a failing check or test in a Vibra project.
---

# Vibra toolchain

The `vibra` CLI is one binary with a small, closed command surface. It is
**pre-alpha**. This build supports `fmt`, `check`, `test`, and `help`; every
other command answers that it is unavailable (exit 4). Run `vibra help` to see
the surface of the build you have. The language itself is specified in
`docs/spec/01-source-language.md` of the Vibra repository, and
`examples/hello` is a project that checks and passes its tests.

## 1. Build `vibra`

Build it from a clone of `https://github.com/nahharris/vibra` with the
pinned Rust toolchain:

```sh
cargo build --release --locked -p vibra-cli   # produces target/release/vibra
target/release/vibra help                     # there is no --version flag
```

The published v0.0.1 binaries read an earlier S-expression syntax, not this
one. Ask the user before installing anything system-wide.

## 2. Lay out a project

```text
hello/
  project.vibon            # the project record
  src/hello/greeting.vib   # module @hello.greeting of target @hello
  tests/greeting.vib       # test module @tests.greeting
```

`project.vibon` holds one record:

```vibon
{
  format: @project.v2
  package: {name: "hello" version: "0.1.0"}
  targets: [{name: @hello kind: @lib root: "src/hello"}]
  dependencies: [:]
}
```

- A target's modules are the `.vib` files under its `root`; the path below
  the root names the module, so `src/hello/greeting.vib` is
  `@hello.greeting`.
- Every `.vib` file under `tests/` is a test module, `@tests.<path>`. Tests
  reach only the `visibility @public` declarations of the modules they
  import.
- Use library targets (`kind: @lib`). A binary target (`kind: @bin`) is
  reported `@tool.unavailable`, and so is any `proc`: operations and grants
  are not supported by this build.
- `dependencies` stays `[:]`. Only the embedded `@std` library is available.

A test module imports `@std.assert` and the modules it tests:

```vibra
use @std.assert enduse

use @hello.greeting enduse

test "greets the world"
  do
    assert.equal("Hello, world!" greeting.message(greeting.Greeting.world()))
endtest
```

`assert.equal(expected actual)` compares two values; `assert.true` and
`assert.false` take one `Bool`.

## 3. The command surface

```text
vibra [--format human|json] [--workspace PATH] fmt PATH [--write]
vibra [--format human|json] [--workspace PATH] check [TARGET]
vibra [--format human|json] [--workspace PATH] test [TEST]
vibra help
```

- Global options go **before** the command, each at most once:
  `vibra --format json check`.
- `--workspace PATH` is the project root, the directory that holds
  `project.vibon`. Without it, the root is the current directory. A root with
  no `project.vibon` is `@project.notFound`.
- `fmt PATH` formats **one file**, `.vib` or `.vibon`. Without `--write` it
  previews the canonical text and changes nothing. A file the reader
  recovered from a syntax error keeps its bytes.
- `check` checks every target and every test module. `check @hello` checks
  one target. A target is an atom, never a path.
- `test` runs every test in the reference interpreter. To run one, pass its
  selector: `vibra test '@tests.greeting::"greets the world"'`. Quote it so
  the shell keeps the double quotes.

Exit codes:

| Code | `result` | Meaning |
| --- | --- | --- |
| 0 | `@command.ok` | Success |
| 1 | `@command.diagnostics`, `@command.testFailed` | Errors in the source or the project, or a failing test |
| 2 | `@command.invalidInput` | Bad command line, unknown target, or unknown or malformed test selector |
| 3 | `@command.operationalFailure` | An operational failure, such as a test running out of memory |
| 4 | `@command.unavailable` | A command, or valid code, this build does not support (`@tool.unavailable`) |
| 5 | `@command.trap` | A test stopped with a runtime trap |
| 6 | `@command.programFailed` | Belongs to `run`, which this build does not support |

## 4. The edit–check loop

After each change:

1. `vibra fmt path/to/file.vib --write` for every file you touched, including
   `project.vibon`. The format is canonical, so never hand-format.
2. `vibra --format json check` for the diagnostics. Each one has a `code`, a
   `level`, a `message`, and a `primarySpan` with `sourceId` (a
   project-relative path) and `startPosition` `{line, column}`, plus
   `related` spans that point at the cause. Human output prints one line per
   diagnostic, `path:line:column: @level @code: message`, on stderr.
3. Fix the first error first; later errors often follow from it. A
   `@tool.unavailable` diagnostic marks valid Vibra this build cannot check
   yet: rewrite around it instead of looking for a syntax error.
4. `vibra test`, or `vibra --format json test`.

Report the commands you ran and their exit codes. Never claim that something
works without a passing `check` or `test`.

## 5. Reading test results

The human report has one line per test, `selector result`, and a last line
`passed P, failed F, selected S`. Under a test that did not pass:

```text
@tests.greeting::"greets by name" @test.assertionFailed
  failure: assert.equal expected "Hi, Ada!", actual "Hello, Ada!" at tests/greeting.vib:12:5
```

`expected` is the first operand of `assert.equal`. A test stops at its first
failed assertion. Every test runs in a fresh state, and tests that perform
operations are reported `@test.unavailable`.

## References

- [references/cli.md](references/cli.md): the JSON envelope, the diagnostic
  document, the payload of each supported command, and the test result
  atoms.
