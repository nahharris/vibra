---
name: vibra-toolchain
description: Install and drive the Vibra pre-alpha toolchain, the `vibra` CLI. Use to set up `vibra`, create a project with `vibra project init`, edit `project.vibon` (targets, entry, effect consent), lay out `src/` and `tests/`, and run `vibra fmt`, `vibra check`, `vibra run`, and `vibra test`. It also covers reading JSON diagnostics and exit codes and fixing a failing build or test in a Vibra project.
---

# Vibra toolchain

The `vibra` CLI is a single binary with a deliberately small, closed command
surface. It is **pre-alpha**, so if a command or flag is not listed here,
assume it doesn't exist and run `vibra help` to confirm. For writing the code
itself, use the **vibra-language** skill.

## 1. Make sure `vibra` is available

```sh
vibra help          # prints the usage grammar; there is no --version flag
```

If it's missing, install a release binary. Binaries exist for
`x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, and
`x86_64-pc-windows-msvc`. Every asset is the bare executable with a `.sha256`
file beside it.

```sh
TAG=$(gh release list --repo nahharris/vibra --limit 1 --json tagName --jq '.[0].tagName')
TRIPLE=x86_64-unknown-linux-gnu        # or aarch64-apple-darwin
mkdir -p ~/.local/bin
curl -fsSL -o ~/.local/bin/vibra \
  "https://github.com/nahharris/vibra/releases/download/$TAG/vibra-$TAG-$TRIPLE"
chmod +x ~/.local/bin/vibra            # make sure ~/.local/bin is on PATH
```

On Windows, download `vibra-$TAG-x86_64-pc-windows-msvc.exe`, rename it
`vibra.exe`, and put it on `PATH`. To build from source, use a clone of
`https://github.com/nahharris/vibra` with the pinned Rust toolchain:
`cargo build --release --locked -p vibra-cli`, which produces
`target/release/vibra`. Ask the user before installing anything system-wide.

## 2. Create a project

```sh
vibra project init my-app      # DEST must be absent or empty; omit it to init the current dir
```

This creates the following layout. The target name comes from the directory
name.

```text
my-app/
  project.vibon            # the project record
  src/my-app/main.vib      # (defn main () void (do))
  tests/                   # test modules, unit @tests
```

`project.vibon` holds one record. Fields go in this order:

```vibon
(record
  format: @project.v1
  package: (record name: "my-app" version: "0.1.0")
  targets: (array
    (record
      name: @app
      kind: @bin
      root: "src/app"
      entry: @app.main.main
      effects: (array @std.io.stdout))
    (record name: @core kind: @lib root: "src/core"))
  dependencies: (dict))
```

- A **binary** target needs `entry` and `effects`. A **library** target must
  omit both. Target roots must not overlap and must not be `tests/`.
- `entry` is `@TARGET.module.function`, and the function must belong to that
  target. It takes no parameters and returns `void` or `(result void E)`.
- `effects` is the **consent list**: every effect root that the entry may
  perform, transitively. Omitting a root that `main` performs is an error, and
  listing one it never performs is a warning. Standard roots are
  `@std.io.stdin`, `@std.io.stdout`, `@std.io.stderr`, `@std.fs.read`,
  `@std.fs.write`, `@std.fs.metadata`, `@std.env.read`, `@std.time.now`, and
  `@std.random.generate`. A user-defined `deffect` root is named by its full
  path, for example `@app.main.log`.
- `dependencies` must stay `(dict)`. Package dependencies are not implemented
  in the pre-alpha, and only the embedded `@std` library is available.
- Modules map to paths: `src/core/text/wrap.vib` in target `@core` (root
  `src/core`) is `@core.text.wrap`. Every directory and file name must be
  kebab-case. A file `x.vib` and a directory `x/` can't coexist.
- Targets import each other by module path, for example
  `(import wrap @core.text.wrap)`. Put pure logic in a `@lib` target so tests
  and binaries can share it.

## 3. The command surface

```text
vibra [--format human|json] [--workspace PATH] project init [DEST]
vibra [--format human|json] [--workspace PATH] fmt PATH [--write]
vibra [--format human|json] [--workspace PATH] check [TARGET]
vibra [--format human|json] [--workspace PATH] run [TARGET] [-- ARG...]
vibra [--format human|json] [--workspace PATH] test [TEST]
vibra help
```

- Global options go **before** the command: `vibra --format json check`.
- `fmt PATH` takes exactly **one file** (`.vib` or `.vibon`), not a directory.
  Without `--write` it prints the formatted text and changes nothing.
- `check` with no target checks every target. With a target, it checks that
  one: `vibra check @app`.
- `run` runs the only binary target, or the named one. Program arguments go
  after `--`: `vibra run -- input.txt`. The arguments reach the program as
  `env.read.args`.
- `test` runs all tests. To run one, pass its exact selector:
  `vibra test '@tests.count::"an empty line has no words"'`. Quote it so
  the shell keeps the double quotes.
- `--workspace PATH` points at a project from elsewhere. Otherwise the project
  is found by walking up from the current directory to a `project.vibon`.

Exit codes:

| Code | Result | Meaning |
| --- | --- | --- |
| 0 | `@command.ok` | Success |
| 1 | `@command.diagnostics` / `@command.test-failed` | Compile errors or failing tests |
| 2 | `@command.invalid-input` | Bad command line, unknown target, or unknown test selector |
| 3 | `@command.operational-failure` | Host problem, such as running out of memory |
| 4 | `@command.unavailable` | Valid code that the pre-alpha does not support yet (`@tool.unavailable`) |
| 5 | `@command.trap` | Runtime trap |
| 6 | `@command.program-failed` | `main` returned `(result.err ...)`. The error value is printed to stderr |

## 4. The edit–check loop

After each change:

1. `vibra fmt path/to/file.vib --write` for every file you touched, including
   `project.vibon`. Formatting is canonical, so never hand-format.
2. `vibra --format json check` to get the diagnostics. Each one has a `code`,
   a `message`, and a `primarySpan` with `sourceId` (a project-relative file)
   and `startPosition` `{line, column}`, plus `related` spans that point at
   the cause. **Human output has no file or line**, so in an agent loop
   always use JSON. Human-readable copies still go to stderr, so read stdout
   for the JSON (for example, `2>/dev/null`).
3. Fix the first error first. Later errors often cascade from it. Look up
   the code in vibra-language's `references/pitfalls.md`.
4. `vibra test` (or `vibra --format json test`), then `vibra run` when the
   project has a binary.

Report the commands you ran and their exit codes. Never claim that something
works without a passing `check` or `test`.

## 5. Tests

- Every `.vib` file under `tests/` is a test module, `@tests.<path>`, and
  must `(import assert @std.assert)`.
- `(test "name" body...)` with `assert.equal`, `assert.true`, and
  `assert.false`. Tests reach only `visibility: @public` declarations of the
  targets they import.
- Every test runs in a **fresh, empty, deterministic host world**: no files,
  no arguments, no environment variables, empty stdin, and a fixed clock and
  random seed. Its output is captured. An effectful test declares `effects:`,
  and running it is consent for those effects. Test logic is best kept pure.
- Failures print
  `FAIL <selector> @test.assertion-failed` and then
  `assertion @std.assert.equal expected=... actual=... at file:line:col`.
  `expected` is the first operand.

## References

- [references/cli.md](references/cli.md): the JSON envelope and payload of
  every command, the test result atoms, and the behavior of `run` output and
  traps.
