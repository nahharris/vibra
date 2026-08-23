# 8. Toolchain

Status: draft

The toolchain is part of the language. A diagnostic without a stable
machine-readable code, or a command without a schema-described machine format,
is an incomplete feature.

## Commands

v1 ships exactly these. Anything not listed is deferred.

| Command | Purpose |
| --- | --- |
| `vibra check <path>` | Parse, resolve, typecheck and effect-check. No output artifact. |
| `vibra fmt [<path>]` | Check canonical form. `--write` rewrites in place. |
| `vibra lint [<path>]` | Report lint-class diagnostics. `--fix` applies mechanical fixes. |
| `vibra run <path>` | Compile and execute. |
| `vibra build <path>` | Compile to a `.wasm` artifact. |
| `vibra test [<path>]` | Run the in-language test suite. |
| `vibra effects <path>` | Report declared and inferred effect rows. |

`<path>` is a `.vib` file or a directory containing `project.vib`. Omitted, it
means the current directory.

### Global options

| Option | Effect |
| --- | --- |
| `--format <human\|json>` | Output rendering. Default `human` on a terminal, `json` otherwise. |
| `--deny-warnings` | Treat every warning as an error. |
| `--quiet` | Suppress progress output; diagnostics still print. |

### `fmt`

Check-only by default. `--write` is the only way to mutate a file, and it never
runs implicitly as part of another command.

`vibra fmt` exits non-zero when any file is not in canonical form, and prints
the offending paths. `vibra fmt --write` is idempotent: running it twice
produces byte-identical files.

### `test`

Tests are declarations in `.vib` files:

```vibra
(import test "@std/test.vib")

(test.scenario "arithmetic"
  (test.case "addition is checked"
    tags: (@arithmetic)
    (test.assert-eq-int64 (add 1 1) 2)))
```

`test.scenario` takes a name and one or more `test.case` children. `test.case`
takes a name, an optional labeled group, and a body.

Known case labels: `tags:`, `expect-error:`, `profile:`. Profiles and tags
**select** tests; they never grant permissions.

| Option | Effect |
| --- | --- |
| `--filter <substring>` | Run cases whose scenario or case name matches. |
| `--profile <name>` | Run only cases in the named profile. Default `@core`. |
| `--tag <tag>` | Run only cases carrying the tag. Repeatable. |
| `--fail-fast` | Stop at the first failure. |
| `--deny-skips` | A skipped case fails the run. |

A bare `vibra test` runs the `@core` profile, which must be hermetic: no
network, no subprocesses, no real environment variables, no writes outside a
temporary directory the runner owns.

### `build`

v1 emits one artifact kind: a `.wasm` module against the `vibra_v1` host
imports. It is not runnable standalone; it needs a host that provides those
imports, which `vibra run` supplies in-process.

Native executables and OCI artifacts are deferred to wave 4.

Builds are **deterministic**: the same source, the same toolchain version and
the same target produce a byte-identical artifact. Timestamps, absolute paths
and iteration order must not leak into output.

## Diagnostics

Every diagnostic carries:

| Field | Meaning |
| --- | --- |
| `code` | Stable identifier, e.g. `E-SCOPE-001`. Never reused, never renumbered. |
| `severity` | `error`, `warning`, or `note`. |
| `message` | One sentence, present tense, stating what is wrong. |
| `primary` | File, byte range, and line/column of the offending construct. |
| `related` | Zero or more additional spans with their own messages. |
| `fix` | Optional mechanical replacement: a span and its replacement text. |

Rules:

- A code's meaning is stable forever. Retiring one means never issuing it
  again, not reusing the number.
- Every code in [Diagnostics](09-diagnostics.md) has at least one `vibra-bad`
  block in the spec that triggers it.
- Messages state the problem, not the remedy; the remedy goes in `related` or
  `fix`.
- Warnings are deterministic: the same input produces the same warnings in the
  same order.

## Machine formats

`--format json` output is described by a JSON Schema under `schemas/`, and every
schema has a stable `$id`. v1 ships:

| Schema | Describes |
| --- | --- |
| `diagnostic.schema.json` | One diagnostic, and the envelope every command emits |
| `project.schema.json` | The resolved `project.vib` |
| `effects.schema.json` | The `vibra effects` report |
| `test-report.schema.json` | The `vibra test` report |

Rules for every machine format:

- Object keys are sorted; output is byte-deterministic for identical input.
- UTF-8, LF newlines, one trailing newline.
- Adding a field is a minor change; removing or repurposing one requires a new
  `$id`.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success. Warnings may have been printed unless `--deny-warnings`. |
| 1 | The command completed and found problems: type errors, failing tests, unformatted files. |
| 2 | The command could not run: bad arguments, unreadable project, missing entry. |
| 3 | Internal compiler error. Always a bug. |
| 101 | The program under `vibra run` faulted (arithmetic trap, or a runtime host failure). |

A program's own exit status from `sys.exit.with-status` passes through `vibra run`
unchanged, except that it may not collide with 2, 3 or 101; a program returning
one of those is remapped to 1 and a note is printed.

## What is not in the v1 toolchain

`vibra index`, the LSP server, the MCP server, `vibra expand`, `vibra rewrite`,
package publishing and dependency solving. See
[Deferred and rejected](10-deferred.md) for the wave that owns each.

This is a real cost: agent integration was one of the original pillars, and v1
ships without it. The reason is sequencing, not doubt — an index and a decoding
service describe a type system, and building them against a type system that is
still moving is what produced the last cycle's bridge layers. Wave 2 picks them
up immediately after v1 freezes.
