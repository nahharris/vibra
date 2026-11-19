# Vibra v0.0.3

**Planned pre-alpha release.** Vibra compiles the whole supported language to
native executables on Windows, Linux, and macOS, `vibra build` ships, and the
WebAssembly backend is removed. The interpreter stays, as the semantic
reference and as the engine of `run`, `test`, and `eval`.

| Order | Milestone | Evidence |
| --- | --- | --- |
| M1 | [The whole language on native code](m1/README.md) | Every executable case agrees across the interpreter, the stepper, and a linked Windows executable. |
| M2 | [Three platforms](m2/README.md) | Real processes on Windows x64, Linux x86-64, and macOS arm64 match the interpreter, with no pending case. |
| M3 | [Build, distribution, and cutover](m3/README.md) | `vibra build` with a manifest, byte-identical rebuilds, a clean-machine install, and the Wasm backend removed. |

## Starting point

[v0.0.2](../v0.0.2/README.md) supplies the language on the interpreter, with
its contracts frozen in M0, and one native slice in M4: a guard with a
consequence, a plan, a scoped file operation through a grant, startup, and
the `ExitCode` entry, linked on Windows. This release widens that slice to
the whole language and to three platforms.

The [Nexus plan](../nexus/README.md) holds the design and the work packages.
It cannot override the [specification](../../spec/00-charter.md).

Native crates are already in the workspace, built before the language was
redesigned. Their presence is not evidence: every behavior is qualified
through the current corpus.

Source files and immutable snapshots stay the authority. The portable
identity of a build is computed from a closed snapshot with its pinned
dependencies and its claim, witness, and policy identities. No published
store or code database is involved.

## Supported targets

| Platform | Triple | Object and link |
| --- | --- | --- |
| Windows x64 | `x86_64-pc-windows-msvc` | PE/COFF; MSVC linker and Windows SDK, discovered explicitly and pinned |
| Linux x86-64 | `x86_64-unknown-linux-gnu` | ELF; a pinned compiler driver and link environment |
| macOS arm64 | `aarch64-apple-darwin` | Mach-O; a pinned Apple SDK and compiler driver |

Windows comes first in engineering order. A target is qualified only by
running on it: emitting an object for another platform is not evidence. An
unsupported target or toolchain gives one of the `@tool.*` diagnostics, never
a silently chosen host target.

## Release gate

**Parity.** For the complete supported executable profile, the interpreter,
the stepper, and native code agree on canonical results, typed errors, test
outcomes, source origins, startup records, and the order of completed audit
events. Expectations are written from the specification, independently of
any executor's output. Every native comparison runs twice, with an
unrestricted scheduling quota and with a quota of one; yielding must neither
repeat a host action nor change an observation.

**A real program.** The release program of v0.0.2 is created, inspected,
repaired, checked, tested, built, distributed, and run on a clean Windows
machine, with its arguments, streams, scoped files, required and optional
grants, typed failures, and exit code. A run in a development interpreter is
not distribution evidence.

**No gaps.** M2 and M3 need zero pending cases on all three targets, both
suites, schema validation, and the repair and approval regressions of
v0.0.2. A case that is unsupported or was not run is a gap.

**Wasm.** Until cutover the Wasm backend runs only the v0.0.1 corpus, under
the fail-closed [transition profile](../../spec/06-runtime.md#wasm-transition).
A program that needs grants, plans, or the new entry is unsupported there. No
second Wasm ABI is built for a backend that is being retired. At the
qualified cutover the Wasm contracts, profile, emitter, runner,
dependencies, and packaging leave together.

If a platform cannot qualify, the release stays incomplete until the support
plan is changed on review. Readiness decides when native compilation ships
and when Wasm goes; it does not decide whether.

## Not in this release

A GPU or remote backend, a foreign function interface, a public
native-library ABI, cross-compilation as a product, a native debugger, an
optimizer beyond what parity needs, and any guarantee about service
recovery. Concurrency is [v0.0.4](../v0.0.4/README.md) and services are
[v0.0.5](../v0.0.5/README.md). Safety here is an obligation of a trusted
toolchain and runtime; it is not operating-system confinement of foreign
machine code.

Each milestone needs an approved contract and a step plan under the
[execution model](../execution.md) before its implementation begins. No gate
of this release is evidenced yet.
