# Vibra roadmap

| Release | Phase | Scope |
| --- | --- | --- |
| [v0.0.1](v0.0.1.md) | Pre-alpha, shipped | M1–M4 |
| [v0.0.2](v0.0.2.md) | Pre-alpha, provisional | M5–M7, subject to substantial revision |
| Later v0.0.x | Pre-alpha | Scope will be assigned as proposals mature |
| v0.1.0 | First alpha | Goal to be defined during pre-alpha |
| v0.2.0 | First beta | Scope to be defined |
| v1.0.0 | Stable | Scope to be defined |

These versions apply to the active implementation. The existing v0.1.x Git
tags, including `v0.1.0`, belong to the
[archived implementation](../../archive/pre-v1/ARCHIVE.md). Source, data format,
schema, and ABI versions are independent of toolchain release versions.
Conformance profiles identify supported surfaces.

The [scratchpad](../scratchpad/README.md) holds unscheduled research. When a
proposal is ready to schedule, it moves into a specific release roadmap. Use
the [milestone execution model](execution.md) to plan and record delivery.
Milestone step plans live in their respective directories under this one.

## Roadmap rules

Roadmaps schedule work against the active specification.

1. A milestone starts only after the previous exit gate is evidenced.
2. Work lands as vertical, conforming slices. Parser-only, runtime-only, and
   docs-only versions of a promised feature do not count as completion.
3. Main stays coherent. Incomplete features remain on short-lived branches and
   are not advertised by the CLI or schemas.
4. Every rule lands with host-language tests, Vibra conformance cases,
   diagnostics, machine schemas, and canonical documentation as applicable.
5. The active toolchain excludes compatibility bridges for the archived
   implementation, including parser adapters, CLI aliases, schema fields,
   ABI shims, and automatic migrations.
6. When implementation exposes a design gap, specification review happens
   before more code.
7. Milestone numbers are stable identities. Plans, ledgers, and diagnostics
   assign deferred work by number, so a milestone is never renumbered or
   inserted; a large milestone is delivered in ordered stages instead, each
   with its own demo and corpus sub-gate, and its exit gate stays the only
   merge gate.

## Architecture boundary

The Rust workspace follows this dependency direction:

```text
syntax -> resolved AST -> typed IR -> interpreter
                              |          |
                              |          +-> external registries/adapters
                              +-> Wasm backend

workspace service -> syntax/resolution/types/effects
CLI and MCP -> workspace service and execution entrypoints
```

There is one lossless syntax tree, one resolved typed IR, one diagnostic model,
one effect-bearing function-call graph, and one workspace snapshot service.
The interpreter and Wasm backend consume the same typed IR. CLI and MCP are
adapters, not alternate implementations.

Crate/module boundaries may evolve, but dependency arrows may not point from
language semantics into CLI, MCP, filesystem UI, or a backend.
