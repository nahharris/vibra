# The Vibra Language Specification, v1

This directory is the **only normative description of Vibra**. If code, tests,
tooling, or prose disagree with a document in `spec/`, the specification is
right and the other thing is a bug.

Vibra is being rebuilt spec-first. The pre-reboot implementation, its standard
library, examples, research notes and decision records are preserved on the
`v0-archive` branch and are **not** normative:

```sh
git fetch origin v0-archive && git switch v0-archive
```

## Reading order

| # | Document | What it fixes |
| --- | --- | --- |
| 1 | [Charter](01-charter.md) | What v1 is for, the design rules, and what v1 explicitly does not promise |
| 2 | [Lexical structure](02-lexical.md) | Bytes to tokens: atoms, symbols, labels, comments, canonical form |
| 3 | [Modules and names](03-modules.md) | Files, imports, visibility, resolution, `project.vib` |
| 4 | [Types](04-types.md) | `deftype`, `defint`, generics, ADTs, interfaces, dispatch |
| 5 | [Expressions](05-expressions.md) | Bindings, control flow, `match`, `try`, arithmetic |
| 6 | [Effects](06-effects.md) | `deffect`, boundary declarations, inference, the host boundary |
| 7 | [Standard library](07-stdlib.md) | The v1 kernel module inventory and its effect roots |
| 8 | [Toolchain](08-toolchain.md) | `vibra` commands, machine formats, exit codes |
| 9 | [Diagnostics](09-diagnostics.md) | The stable diagnostic code registry |
| 10 | [Deferred and rejected](10-deferred.md) | Everything outside v1, with the wave that owns it |

The implementation plan lives in [`../ROADMAP.md`](../ROADMAP.md). It is a
schedule, not a contract; it may be reordered without amending the spec.

## Status of this specification

**Draft.** No section is frozen until milestone M0 in the roadmap closes. Until
then, sections may change without a migration path.

Each document carries a status line. A section marked `frozen` may only change
through the amendment process below.

## Conformance

An implementation conforms to Vibra v1 when:

1. It accepts every program the grammar in `spec/` accepts, and rejects every
   program it does not.
2. It emits exactly the diagnostic code that [Diagnostics](09-diagnostics.md)
   assigns to each rejection, with the primary span the spec names.
3. Its formatter is idempotent and produces the canonical form defined in
   [Lexical structure](02-lexical.md) for every accepted program.
4. It passes the conformance suite.

### The conformance suite is generated from this directory

Every fenced code block in `spec/` is executable evidence, and the build fails
when one stops matching the implementation. Tag every block:

| Tag | Checked against | Use for |
| --- | --- | --- |
| `vibra` | Module grammar; must parse, typecheck, and pass effect checking | Complete top-level source |
| `vibra-expr` | Same, wrapped in a `main` body | Statements and expressions alone. Leading `import` forms are hoisted above the synthesized `main`. |
| `vibra-project` | The `project.vib` loader | A complete manifest |
| `vibra-bad` | Must be **rejected**, with the diagnostic code named in the line above the block | Every diagnostic in the registry |
| `ebnf`, `text`, `sh`, `json` | Nothing | Grammar productions and prose |

There is no tag that lets Vibra-looking syntax escape checking. A feature that
is specified but not yet implemented belongs in
[Deferred and rejected](10-deferred.md), not in a block the checker skips.

This rule exists because the previous cycle accumulated normative documents
describing syntax the parser rejected. An agent reading a contract that
disagrees with the compiler writes code that cannot compile, so a stale
normative document is a defect in the language, not a documentation chore.

## Amending the spec

1. Open an issue stating the problem as a program that is currently accepted
   and should not be, or rejected and should not be.
2. Amend the spec section **and** its conformance blocks in the same change.
3. Implementation follows the amendment; it never leads it.

A change to the implementation not preceded by a spec amendment is a bug in the
change, however good the behavior is.
