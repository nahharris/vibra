# Vibra WebAssembly ABI v1

Vibra compiles the complete reachable source graph into one deterministic
WebAssembly module. The module exports `main: () -> i32`; zero is success and a
nonzero result asks the host for the recorded structured error.

## Guest code and static linking

The guest owns statement sequencing, expression evaluation order, locals,
user-function calls and returns, `if`, `while`, match-arm selection, mutation,
and reference operations. Reachability starts at `main`. Unreachable user
functions are omitted, and each reachable concrete generic type-argument tuple
gets a deterministically named/indexed function instance. Application,
dependency, and stdlib user functions therefore share one function index
space.

The `vibra.plan.v1` custom section contains the deterministic value-expression,
pattern, seed, and privileged-call descriptors needed by the v1 host. It also
contains only reachable host-function signatures, policy argument declarations,
and interface identities. This makes `program.wasm` executable after all
compiler IR and source state has been dropped. `vibra.program.v1` separately
contains the SHA-256 fingerprint of the complete lowered input.

## Values and memory

Guest locals and parameters are direct `i32` arena addresses. Address zero is
invalid; the host owns the arena and returns nonzero opaque handles. This is the
v1 representation for dynamic values, mutable cells, references, policies, and
capabilities. The guest cannot forge a valid policy or widen one by integer
arithmetic because every handle is checked before use.

Within values, the stable layouts in [`src/wasm_abi.rs`](../src/wasm_abi.rs)
remain normative: scalars are direct, strings/buffers use 32-bit
pointer/length descriptors, aggregate fields are aligned, enums use a tag plus
aligned payload, and mutable/reference values use arena addresses. A future
external static-Wasm ABI may expose those layouts directly without changing the
guest compiler's opaque-handle safety boundary.

## Imports

Language control flow is never a host import. `vibra_v1` exposes narrow value
and runtime primitives:

- seed and constant lookup;
- checked value reads, construction, mutation, and boolean projection;
- nested argument frames and privileged/high-level stdlib calls;
- checked pattern matching and scoped binding lookup; and
- status/non-exhaustive-match reporting.

The removed `vibra_v1.run_program` envelope is rejected. Imports under
`vibra_v1` must be in the exact v1 allowlist. Genuine Preview 1 calls must use
`wasi_snapshot_preview1` and a known WASI function name; unknown WASI and Vibra
symbols are rejected before instantiation.

## Policies and capabilities

The host creates policy and grant handles from `RunConfig` before `main`.
Guest-side narrowing can select a statically validated subset, but all handles
remain opaque and every privileged host call rechecks the approved scopes.
Filesystem handles and allocation limits remain host-owned. Instantiation does
not grant authority by itself.

## Determinism and compatibility

Reachable specializations, types, imports, functions, exports, code,
descriptors, implementation identities, and custom sections are sorted or
emitted in fixed traversal order before indexes are assigned. Maps that do not
have stable iteration order are sorted explicitly. Identical lowered inputs
must produce byte-identical Wasm and therefore byte-identical `.vapp` archives.

The module name `vibra_v1`, the custom-section shapes, function signatures, and
value layouts are a compatibility contract. An incompatible change requires
`vibra_v2`; the runtime must not reinterpret an existing v1 module.
