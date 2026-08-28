# Pre-v1 archive record

Archived: 2026-08-22
Repository snapshot: `2d3cabf256ce74d930a3c4c2f1071e16fab68cdc`
Original version: `0.1.4`
Status: unsupported, non-normative

## Why this tree was archived

The implementation grew while the language was still being discovered. It
contains valuable work, but its surfaces reached different maturity levels:

- a typed frontend and a legacy value-based lowering path coexisted behind an
  adapter;
- accepted documents mixed intended design, implemented behavior, migration
  history, and future plans;
- runtime capability and async prototypes existed without a complete stable
  source contract;
- syntax, type, effect, tooling, project, schema, and runtime changes were
  planned and delivered independently; and
- the presence of a CLI command or schema could be mistaken for a supported
  language commitment.

Continuing that tree would let implementation accidents define v1. The soft
reboot therefore keeps the snapshot for archaeology and restarts from the
active specification.

## Contents

The directory preserves the former top-level layout, including:

- `src/`, `Cargo.toml`, and `Cargo.lock`;
- Rust and Vibra tests;
- former docs, schemas, plans, research, and examples;
- project, distribution, container, workflow, migration, and skill files; and
- the former standard-library submodule location.

The in-tree archive is for inspection. Some files retain their original
root-relative assumptions and are not maintained or guaranteed to build from
the nested location.

## Exact restoration

For an exact runnable historical checkout, use Git rather than changing the
archive:

```sh
git worktree add ../vibra-pre-v1 2d3cabf256ce74d930a3c4c2f1071e16fab68cdc
git -C ../vibra-pre-v1 submodule update --init --recursive
```

The original `stdlib` gitlink points to
`656ec4fb42100b2cb73a5569160441bae41b432d` from
`https://github.com/nahharris/vibra-stdlib.git`. The submodule was not populated
in the soft-reboot worktree when it was archived, so the Git restoration above
is the canonical way to recover its content.

## Use policy

- Do not edit this tree as the new compiler.
- Do not restore a feature because it once existed.
- Do not preserve pre-v1 compatibility in v1.
- When archaeology reveals a sound rule, add it first to the active spec and
  conformance corpus, then implement it anew.
