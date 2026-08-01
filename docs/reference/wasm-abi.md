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
contains only reachable host-function signatures and interface identities.
This makes `program.wasm` executable after all compiler IR and source state
has been dropped. `vibra.program.v1` separately contains the SHA-256
fingerprint of the complete lowered input.

## Values and memory

Guest locals and parameters are direct `i32` arena addresses. Address zero is
invalid; the host owns the arena and returns nonzero opaque handles. This is the
v1 representation for dynamic values, mutable cells, and references. The guest
cannot forge a valid handle or widen it by integer arithmetic because every
handle is checked before use.

Within values, the stable layouts in [`src/wasm_abi.rs`](../../src/wasm_abi.rs)
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

The removed `vibra_v1.run_program` envelope is rejected. Imports must be in the
exact v1 registry; arbitrary Preview 1 imports and unknown Vibra symbols are
rejected before instantiation.

## Host authority

Every host operation (filesystem, network, process, clock, random,
environment, stdin) is unconditionally available to guest code -- there is no
capability, policy, or `--allow-*` authorization layer. Resource handles
remain opaque and host-owned; filesystem handles and allocation limits remain
host-owned regardless.

## Resource lifecycle and limits

Opaque host handles are valid only in the program instance that minted them;
copying the value does not duplicate the underlying resource or its authority.
Dynamic resources have one explicit close transition. The first close releases
the resource immediately. Duplicate close and every operation through an alias
after close return `fs-error.resource-closed`; a never-minted handle returns
`fs-error.invalid-handle`. Standard streams are borrowed from the process:
close is a no-op and cannot revoke them. Stdin is a singleton per instance, so
repeated access cannot grow the handle table.

All remaining live resources are closed when the program instance ends,
including error exits. `RunConfig.max_open_files` / `--max-open-files` bounds
all live owned handles (currently files; borrowed standard streams are
excluded); the default is 1024 and zero explicitly disables the bound.
Exhaustion is the typed
`fs-error.too-many-open-files` result and does not mint a handle. Future socket,
child-process, and timer handles must use this same table and contract.

`RunConfig.max_alloc_len` bounds each program-controlled raw buffer allocation
(64 MiB by default). Strings, arrays, maps, enum payloads, function arguments,
and assignments are value copies in v1; aggregate copy cost is proportional to
the copied value, and collection growth may allocate and copy the collection.
The host arena itself lasts for the instance and is reclaimed wholesale at
exit. Compact `bytes` values are the safe buffer strategy at the host boundary;
APIs must reject a requested buffer above `max_alloc_len` before allocation.

## Determinism and compatibility

Reachable specializations, types, imports, functions, exports, code,
descriptors, implementation identities, and custom sections are sorted or
emitted in fixed traversal order before indexes are assigned. Maps that do not
have stable iteration order are sorted explicitly. Identical lowered inputs
must produce byte-identical Wasm and therefore byte-identical `.vapp` archives.

The module name `vibra_v1`, the custom-section shapes, function signatures, and
value layouts are a compatibility contract. An incompatible change requires
`vibra_v2`; the runtime must not reinterpret an existing v1 module.
