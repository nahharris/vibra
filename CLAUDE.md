# AGENTS.md

Guidance for AI agents and contributors working in this repository.

## Read this first

**Vibra is currently a specification, not a compiler.** The repository was
rebooted spec-first in August 2026. There is no `src/`, no `Cargo.toml`, and no
standard library here yet, and that is deliberate — not a broken checkout.

| | |
| --- | --- |
| The language | [`spec/`](spec/README.md) — the only normative documents |
| The plan | [`ROADMAP.md`](ROADMAP.md) |
| The previous implementation | the `v0-archive` branch |

```sh
git fetch origin v0-archive && git switch v0-archive
```

Nothing on `v0-archive` is normative. Read it for salvage and for lessons; do
not treat its decision documents, its grammar, or its README as current. A
sizable part of why the reboot happened is that accepted contracts there
described syntax the parser rejected, so an agent following them wrote code that
could not compile.

## The one rule that matters

**The spec leads; the implementation follows.**

A change to behavior that is not preceded by a spec amendment is a bug in the
change, however good the behavior is. If you find yourself writing code that
does something `spec/` does not describe, stop and amend the spec first — or
conclude that the behavior should not exist.

Corollaries:

- No milestone in `ROADMAP.md` starts before the spec sections it implements
  are frozen.
- No bridge or adapter layers. The previous cycle's most expensive artifact was
  a temporary adapter between a typed frontend and a legacy lowering path,
  which became permanent because it worked. If a milestone needs a shim to hit
  its gate, split the milestone.
- A feature whose rejections have no stable diagnostic code is not finished.

## Working on the spec

1. State the problem as a program that is currently accepted and should not be,
   or rejected and should not be.
2. Amend the section **and** its conformance code blocks in the same change.
3. Tag every fenced block. `vibra` and `vibra-expr` blocks must eventually
   parse and typecheck; `vibra-bad` blocks must be rejected with the code named
   above them. There is no tag that exempts Vibra-looking syntax from checking.
4. Keep cross-references live. Every diagnostic code in
   [`spec/09-diagnostics.md`](spec/09-diagnostics.md) needs a block that
   triggers it.

Speculative or unscheduled ideas go in
[`spec/10-deferred.md`](spec/10-deferred.md), assigned to a wave or recorded as
rejected with a reason. Nothing is allowed to be merely absent.

## Working on the implementation

Once M1 opens, this section gains build and test commands. Until then there is
nothing to build, and a change that adds a compiler before M0 closes is out of
order.

Conventions that carry forward regardless:

- Symbols and test names are kebab-case.
- Both suites — the host-language tests and the in-language `vibra test` suite —
  pass before any commit that closes a milestone.
- Machine output is JSON with sorted keys and deterministic ordering, described
  by a schema with a stable `$id`.

## Style

- Prose in this repository states what is true and what is not. Where v1 does
  not promise something — and it does not promise sandboxing, resource bounds,
  or attack prevention — say so plainly rather than leaving a reader to infer a
  guarantee.
- Keep semantic preservation and attack prevention as separate claims. Vibra
  makes the first, about behavior its own tests cover, and never the second.
