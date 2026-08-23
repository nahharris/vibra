# Vibra

Vibra is a statically typed, effect-tracked language with an S-expression
surface that compiles to WebAssembly, designed on the assumption that most of
its source code is written, reviewed and rewritten by language models.

**This repository currently contains a specification, not a compiler.**

Vibra was rebuilt spec-first in August 2026. The previous implementation
discovered the right feature set by building it, and ended up with five
subsystems at five levels of maturity, contracts describing syntax the parser
rejected, and a security story that was documented but never enforced. The
conclusion was not that the features were wrong — it was that a language is a
specification, and the specification has to come first.

## Where things are

| | |
| --- | --- |
| **The language** | [`spec/`](spec/README.md) — the only normative documents |
| **The plan** | [`ROADMAP.md`](ROADMAP.md) — milestones M0 through M9 to a usable v1 |
| **The previous implementation** | the `v0-archive` branch |

```sh
git fetch origin v0-archive && git switch v0-archive
```

Nothing on `v0-archive` is normative. It is a source of salvage and of lessons;
[`ROADMAP.md`](ROADMAP.md) lists what is worth taking from it and what is not.

## What v1 is for

Command-line tools and scripts. The day v1 ships, an agent should be able to
write, check, test and run a real CLI program in Vibra without leaving the
language: read arguments and environment, read and write files, run
subprocesses, transform text, handle failure with typed results, and exit with
a status code.

That is the smallest target that exercises reader, types, interfaces, effects,
backend, runtime, stdlib and test runner end to end. Everything else is
assigned to a numbered wave in
[Deferred and rejected](spec/10-deferred.md).

## What the language looks like

```vibra
(import array "@std/array.vib")
(import convert "@std/convert.vib")
(import env "@std/env.vib")
(import fs "@std/fs.vib")
(import io "@std/io.vib")
(import text "@std/text.vib")

(deftype line-count
  (record
    (path str)
    (lines uint64))
  doc: "How many lines one file holds."
  implements: (display)

  (defn display.show ((self self)) str
    (text.concat
      (field self path)
      (text.concat ": " (convert.uint64-to-str (field self lines))))))

(defn count-lines ((path str)) (result line-count fs.error)
  doc: "Count the lines in a file."
  effects: (fs.read)
  (let contents (try (fs.read.to-str path)))
  (result.ok
    (record
      (path path)
      (lines (array.length (text.split-lines contents))))))

(defn main () (result void fs.error)
  (for path (env.read.args)
    (let counted (try (count-lines path)))
    (io.stdout.println (display.show counted)))
  (result.ok unit))
```

Four things in that sample are the whole design in miniature:

- **One canonical spelling per construct**, so the formatter is idempotent and
  a type-directed decoder's search terminates.
- **Explicit signatures everywhere**, including the effect row at every module
  boundary — and inferred inside a module, where restating a derived transitive
  fact buys nothing.
- **Failure is a value**, propagated by `(try ...)`, and dropping one is a
  diagnostic.
- **Implementations live with the type**, so there is exactly one place any
  behavior can be defined and exactly one place to look for it.

## What v1 will not promise

Read this before building on it.

- **v1 is not a sandbox.** Effects are checked statically and then erased.
  `vibra run` grants the program the authority of the process that launched it.
  Do not execute untrusted Vibra source under v1. Enforcement is wave 1.
- **No resource bounds.** No fuel, no memory ceiling, no deadline.
- **No claim that compilation prevents attacks.** Vibra claims semantic
  preservation for behavior its own tests cover, and nothing beyond that.

## Status

Milestone M0 — freezing the specification. No compiler is being built yet, on
purpose: [`ROADMAP.md`](ROADMAP.md) ground rule 1 is that no milestone starts
before the spec sections it implements are frozen.

## License

MIT OR Apache-2.0.
