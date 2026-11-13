# Vibra v0.0.3

**Planned pre-alpha release.** Vibra builds native executables backed by
Nexus. The reference interpreter remains the semantic oracle and serves the
interactive tooling from [v0.0.2](../v0.0.2/README.md).

| Order | Milestone | Evidence |
| --- | --- | --- |
| M1 | [Native execution slice](m1/README.md) | Link and run the fixture on Windows x64 first, then on Linux x86-64 and macOS arm64. |
| M2 | [Native parity](m2/README.md) | All three platforms match the interpreter across the supported executable corpus. |
| M3 | [Reproducible builds and Wasm cutover](m3/README.md) | Each platform produces reproducible executable and manifest pairs and passes the release audit before Wasm leaves the active toolchain. |

## Scope and priorities

Start after the [v0.0.2 handoff](../v0.0.2/m5/README.md) covers build inputs,
provenance, source origins, and conformance.
The first compiled product is a binary project target over the admitted
project surface. Each native target ships with a pinned link environment and
is qualified on its actual platform.

| Release platform | Native target | First execution slice |
| --- | --- | --- |
| Windows x64 | `x86_64-pc-windows-msvc` | PE/COFF and MSVC tooling, with a Windows-native CLI and `.exe` launch. [Rust Windows MSVC target](https://doc.rust-lang.org/stable/rustc/platform-support/windows-msvc.html) |
| Linux x86-64 | `x86_64-unknown-linux-gnu` | ELF and the pinned GNU link environment. |
| macOS arm64 | `aarch64-apple-darwin` | Mach-O and the pinned Apple SDK and link environment. |

Keep source resolution, type and effect checking, queries, and edit planning
shared. Lower checked IR into a verified execution IR, emit CPU objects,
and link them with a launcher and Nexus. Nexus owns activations, value
storage, execution checkpoints, traps, and host dispatch for this release.
Host providers and representation-neutral primitives remain shared.

This release targets AOT executables and cooperative execution checkpoints.
The [v0.0.4 handoff](../v0.0.4/README.md) carries forward runtime-owned
frames, opaque context-local handles, and source-linked host events.
v0.0.4 owns local actor scheduling and the first public capability surface.
GPU and remote execution, native libraries, native debugger integration,
and cross-compilation are outside this release.
See the [Nexus scratchpad](../scratchpad/execution-targets/nexus.md) for the
proposal and follow-on work. [GitHub #408](https://github.com/nahharris/vibra/issues/408)

## Specification transition and release gate

Land the native contract and its transition profile before the first native
implementation slice. Keep the current Wasm contract and differential tests
in force while qualifying the replacement. Change the charter, package/build,
tooling, runtime, diagnostics, and conformance contracts together at cutover.
Review source/effect chapters for backend references and preserve the shared
language semantics.

The specification now carries the native contract beside the Wasm one, marked
as specified for this release and not implemented: the
[Native execution](../../spec/06-runtime.md#native-execution) section, the
`native-v1` profile and the native parity inventory in
[Native transition](../../spec/07-diagnostics-and-conformance.md#native-transition),
the [native build command](../../spec/05-tooling.md#native-build-command) and
[native build products](../../spec/04-programs-and-packages.md#build-products),
and the diagnostics for an unsupported target, a missing link tool, a link
failure, and an incompatible artifact. It states that execution may be
suspended and resumed at checkpoints without observable effect and defines no
quota. Until the maintainer approves it, the runtime ABI and the parity
inventory it describes are provisional.

Require full interpreter parity for every executable case in the supported
release profile and repeatable native artifacts on all three platforms. An
unavailable native case counts as a gap for that profile, and a `pending` cell
of the native parity inventory fails the M2 gate. Run the host suite
and the independent Vibra corpus. Require v0.0.2 tooling regressions to pass
and keep run, test, and interactive evaluation sessions interpreter-backed.
Retire the active Wasm path only after all three native targets pass. If one
platform slips, keep Wasm coverage and treat the native slice as preview
evidence pending a revised release decision.
