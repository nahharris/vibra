# Vibra roadmap

| Release | Phase | Outcome | Exit evidence |
| --- | --- | --- | --- |
| [v0.0.1](v0.0.1/README.md) | Pre-alpha, shipped | Checked language in the earlier S-expression surface, interpreter, effects, and Wasm conformance | [M4 exit evidence](v0.0.1/m4/exit-evidence.md) |
| [v0.0.2](v0.0.2/README.md) | Pre-alpha, planned | The language of the active specification on the interpreter, grants, an agent repair loop, and a first native slice | [Repair a seeded defect and run one native fixture](v0.0.2/README.md#release-gate) |
| [v0.0.3](v0.0.3/README.md) | Pre-alpha, planned | Native executables on Windows, Linux, and macOS; `vibra build`; Wasm removed | [Interpreter parity and reproducible artifacts on all three targets](v0.0.3/README.md#release-gate) |
| [v0.0.4](v0.0.4/README.md) | Pre-alpha, planned | Dependencies, tool adapters, and bounded local concurrency | [Use and upgrade a real dependency; exercise two workers](v0.0.4/README.md#release-gate) |
| [v0.0.5](v0.0.5/README.md) | Pre-alpha, planned | Service behavior and selected recovery profiles | [The pending-save service case under failure injection](v0.0.5/README.md#release-gate) |
| Later v0.0.x | Pre-alpha | Scope assigned as proposals mature | |
| v0.1.0 | First alpha | Goal to be defined during pre-alpha | |
| v1.0.0 | Stable | Scope to be defined | |

Only v0.0.1 has shipped. While milestone 1 of v0.0.2 is in progress, the
v0.0.1 toolchain is the `vibra-v1` binary. The plans for v0.0.2 and later were
rewritten in place on 2026-11-16 against the rewritten
[specification](../spec/00-charter.md); earlier plans for those releases are
not kept. The existing v0.1.x Git tags belong to the
[archived implementation](../../archive/pre-v1/ARCHIVE.md).

The [scratchpad](scratchpad/README.md) holds unscheduled research and the
record of the redesign debate; it is not normative. Use the
[milestone execution model](execution.md) to plan and record delivery. The
[Nexus plan](nexus/README.md) holds the design and work packages of the
native compiler and runtime, which span v0.0.2 M4 through v0.0.4.

## Roadmap rules

1. A milestone's exit gate is evidenced before the next milestone's gate is
   claimed. Work that has no dependency on an open gate may overlap it when
   the release plan names that overlap.
2. Work lands as vertical, conforming slices. A parser-only, runtime-only,
   or documentation-only version of a feature does not count.
3. Main stays coherent. Incomplete features are not advertised by the CLI or
   the schemas.
4. Every rule lands with host-language tests, Vibra conformance cases,
   diagnostics, machine schemas, and canonical documentation, as applicable.
5. There is no compatibility bridge to an earlier surface: no parser
   adapter, CLI alias, schema field, ABI shim, or automatic migration.
6. When implementation exposes a design gap, the specification is corrected
   before more code is written.
7. Prefer a compile-time rejection or an error value over a trap, in the
   language and in the toolchain's own contracts.
8. No dependency cycles: among milestones, work packages, crates, and build
   inputs.
9. A statement that something is implemented, tested, or measured names its
   evidence. A model-writing pilot is evidence about a surface on stated
   models and tasks; it is never a release gate.

## Architecture boundary

```text
source -> syntax tree -> resolved tree -> checked IR -+-> interpreter
                                                      |
                                                      +-> lowering -> verified execution IR -+-> reference stepper
                                                                                             +-> code generation -> object
                                                                                                 + runtime archive -> linker -> executable

workspace service -> syntax, resolution, checking, contracts, effects
CLI and adapters   -> workspace service and execution entry points
```

There is one lossless syntax tree, one checked IR, one diagnostic model, one
call graph that carries effect rows, and one workspace snapshot service. The
interpreter is permanent: it is the semantic reference and the compile-time
evaluator. Lowering, the stepper, code generation, and the runtime consume
checked IR and never the reverse. Dependency arrows do not point from
language semantics into the CLI, an adapter, or a backend. The crate graph is
acyclic and is checked.
