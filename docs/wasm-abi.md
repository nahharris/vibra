# Vibra WebAssembly ABI v1

Vibra source execution crosses a deterministic WebAssembly boundary. The compiler lowers the complete reachable source graph first, sorts symbol-bearing maps for its program fingerprint, and emits a module with:

- one exported `main: () -> i32` function;
- one versioned import, `vibra_v1.run_program: () -> i32`;
- a `vibra.program.v1` custom section containing the SHA-256 fingerprint of the complete lowered program; and
- no ambient or unvalidated imports.

Status `0` means success. A nonzero status means the host operation failed and the runtime returns the captured structured error to the caller.

## Values and memory

The stable value layout is defined in [`src/wasm_abi.rs`](../src/wasm_abi.rs): direct scalars, 32-bit pointer/length descriptors, aligned aggregate fields, tagged enum payloads, and 32-bit arena addresses for mutable cells and references. Those layouts are reserved for finer-grained `vibra_v1` calls and external static Wasm interoperability; changing an existing layout requires a new ABI module version.

## Capabilities

Policies and grants remain host-owned. The module cannot manufacture handles or broaden the `RunConfig` approved before instantiation. Privileged stdlib operations execute only through the installed `vibra_v1` host environment, which applies the same scope checks as source execution.

## Determinism and compatibility

Types, imports, functions, exports, code, and custom sections are emitted in a fixed order. Program maps are sorted before hashing, so identical lowered inputs produce byte-identical modules across repeated builds. The runtime rejects every import outside the known ABI allowlist before instantiation.

The current v1 envelope executes the complete lowered program at the versioned host boundary, retaining parity with the language interpreter while the reserved scalar and aggregate calls are progressively moved guest-side. Packages bind to the exact ABI string `vibra-v1`; incompatible changes require `vibra_v2` rather than mutating v1.
