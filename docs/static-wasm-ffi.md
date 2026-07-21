# Static WebAssembly FFI

Vibra's first foreign-function boundary is static WebAssembly linking. It does
not load code at runtime and does not define a native ABI. A project dependency
may expose one package-relative `.wasm` artifact:

```yaml
dependencies:
  math:
    path: foreign/math
    wasm: math.wasm
```

Git dependencies use the same `wasm` field alongside `git` and exact `rev`;
the artifact is resolved inside the synced `dep/<alias>` tree. Absolute paths
and traversal are rejected. The dependency alias is the stable module name:

```yaml
sum:
  $function:
    left: $int32
  args:
    right: $int32
  return: $int32
  do:
    - $wasm:
        import:
          module: "@math"
          name: sum
        args: [$args.left, $args.right]
```

The visible `$wasm` node is the explicit unsafe boundary. It must be the
wrapper's only body statement. The wrapper owns conversion of integer status
codes into typed Vibra `result` values; v1 performs no automatic foreign error
translation.

## ABI

The scalar mapping is exact:

| Vibra | WebAssembly |
| --- | --- |
| `bool`, `int8`/`uint8`, `int16`/`uint16`, `int32`/`uint32` | `i32` |
| `int64`/`uint64` | `i64` |
| `float32` | `f32` |
| `float64` | `f64` |
| `void` return | no result |

Only zero or one result is supported. Records, arrays other than a direct
`$array: $uint8` input, nominal wrappers, references, handles, and host
capabilities never cross this boundary.

Bytes and UTF-8 strings use caller-owned linear-memory buffers and an explicit
`(pointer: int32, length: int32)` pair. Both values are unsigned bit patterns:
`pointer + length` must not overflow and must remain within the caller memory.
The callee may read the half-open range `[pointer, pointer + length)` only for
the duration of the call and must never retain the pointer. Empty buffers use
length zero; pointer zero is allowed only with length zero. Strings are valid
UTF-8 before the call. The wrapper allocates, validates, and releases the
call-scoped buffer. A future mutable output buffer contract will add a separate
capacity and explicit number-of-bytes-written result.

The artifact must import a memory named `vibra_ffi.memory` when it accepts
caller-owned pointers. Linking supplies the current instance's memory. It must
not export or require an allocator. Callee-owned pointers, allocator
negotiation, and foreign-owned heap values are outside v1.

`$str` wrapper arguments are automatically lowered to one `(pointer, length)`
pair containing their validated UTF-8 bytes. A direct `$array: $uint8` argument
uses the same pair without UTF-8 interpretation. Non-empty allocations are
8-byte aligned; an empty buffer is exactly `(0, 0)`. The host checks every
length, addition, alignment, wasm32 conversion, memory growth, and write before
calling the export. A fresh memory and module instance are used for each call,
so a foreign function cannot observe or retain a previous call's buffer.

Foreign status codes remain ordinary declared scalar results. A Vibra wrapper
can match or convert that integer into its public typed `result`; the FFI layer
does not assign meaning to status values. Mutable output buffers and copying
foreign writes back into Vibra arrays are deferred; v1's implemented buffer
path is caller-owned, call-scoped input.

## Validation and safety

`vibra check` opens every declared artifact, validates it as WebAssembly, finds
each `@alias` function export, and compares its exact wasm parameter/result
types with the wrapper. This happens before build or execution:

- `E-WASM-005`: undeclared dependency, absent `wasm` field, missing artifact,
  invalid artifact, or unsafe artifact path;
- `E-WASM-006`: missing export or export is not a function; and
- `E-WASM-007`: unsupported ABI type or exact signature mismatch.

The linker must repeat these checks as defense in depth. Static foreign code
receives no Vibra grants and cannot import the privileged `vibra_v1` host ABI.
Its only permitted import in v1 is `vibra_ffi.memory` when using buffers.

For scalar-only libraries, lowering embeds the validated artifact bytes in the
deterministic `program.wasm` execution plan. Source execution and `.vapp`
execution instantiate that embedded module and invoke the named export through
the wrapper's exact scalar signature. The artifact digest contributes to the
program fingerprint; `.vapp` also inventories the original dependency file,
so changing foreign code changes the package bytes and verification result.

## Limitations and roadmap

This milestone deliberately excludes native/C ABIs, runtime plugin discovery,
network-loaded code, rich values, callee-owned memory, allocator negotiation,
and automatic error translation. Mutable output-buffer copy-back remains a
follow-up to the implemented caller-owned input path. Typed runtime plugins are
a separate design;
they must not reuse `@alias` static resolution. Future milestones may add a
multiple named wasm artifacts per package, generated
safe wrappers, and negotiated richer-value ABI versions without weakening the
explicit `$wasm` boundary.
