# Vibra nominal effect system

Date: 2026-08-07
Status: accepted implementation contract for issue #240 (amended by #249; parent issue #151)
Migration: intentionally breaking; only nominal roots and `deffect` operations are supported

## Decision

Vibra effects are compiler-native, nominal roots declared with `deffect`.
Each operation owns exactly one root and may list additional roots in its
`effects:` annotation. The owner is implicit; the annotation is additive.
Exported functions, interface methods, and deffect operations declare their
complete effect ceiling. Module-private functions infer their performed row
from their body. `main` also infers; program authority is a project-level
contract and is not repeated in the function signature.

Inference builds the lowered module call graph and computes the least fixed
point of the effect-set join operation. Calls contribute the callee's inferred
row, so private call chains and recursive components are handled without
requiring private annotations. Interface dispatch is already collapsed to a
concrete callee key before this pass. An inferred row always reports leaf
operations; it never expands to a declaration root.

At a declaration boundary, the declared row must cover the inferred row.
Under-declaration is `E-EFFECT-001`. Over-declaration is accepted with a
deterministic `W-EFFECT-001` warning. A bare declared root such as `fs` covers
`fs.read`, `fs.write`, and `fs.metadata` for this comparison only; it does not
change the stored declaration or inferred output.

```text
(deffect read
  (defn open (path path) (result reader fs-error)
    (intrinsic @fs-open-read path)
    effects: ())
  (defn file (path path) (result str fs-error)
    ; ordinary Vibra composition over read/open and stream operations
    ...
    effects: (stream.read stream.manage)))
```

Inside the declaration, `file` is still the module-level symbol `fs.file`.
The operation is addressed as `read.file`. A root may not collide with any
other top-level name, and operation names are unique within their root.
Roots are resolved by the canonical defining module and root name, never by an
import alias.

Effects are static and erased. They describe observable host actions and the
ambient authority a function can reach; they do not sandbox an embedder.
Implementations must remain below the interface method's ceiling, even when
their concrete performed rows differ from another implementation.

## Native boundary

`intrinsic` is a closed, compiler-known operation. Its kebab-case atom is
looked up in the versioned `vibra_v1` registry, with exact argument arity and
ABI value types checked during lowering. Pure value intrinsics may be used by
ordinary functions; an intrinsic that crosses the ambient-authority boundary
must be owned by a `deffect` operation. The runtime dispatches the registry
entry and may mint a host-backed endpoint only for the validated operation
result.

Raw `wasm` is reserved for dependency-provided/custom Wasm and is owned by a
nominal effect. It is not a substitute for a built-in intrinsic. Composite
host conveniences (read-all, write-all, copy, process-run, and formatting
helpers) belong in Vibra code above the primitive boundary.

## Nominal endpoints and streams

Host-backed endpoints are declared with `(newtype (handle @access))` and are
unforgeable. Source casts to, from, or between host-backed newtypes are
rejected. A validated endpoint may widen only to the generic handle capability
required by a trusted stream operation:

| endpoint capability | accepted generic capability |
| --- | --- |
| `@read` | `@read` |
| `@write` | `@write` |
| `@read-write` | `@read`, `@write`, `@read-write` |
| `@process` | `@process` |

The reverse widening is never allowed. Files, standard streams, TCP/UDP
resources, and process pipes keep distinct nominal identities while sharing the
interfaces `stream.readable`, `stream.writable`, `stream.flushable`, and
`stream.closeable` where their semantics permit.

The shared data/lifecycle error is `stream.error`; acquisition operations keep
provider-specific errors such as `fs-error` and `net-error`.

## Root inventory

The approved roots are:

| module | roots |
| --- | --- |
| `stream` | `read`, `write`, `manage` |
| `fs` | `read`, `write`, `metadata` |
| `io` | `stdin`, `stdout`, `stderr` |
| `net` | `connect`, `bind` |
| `process` | `spawn`, `wait`, `signal` |
| `env` | `read`, `write` |
| `time` | `now`, `sleep` |
| `random` | `generate` |
| `sys` | `info` |

The full operation inventory and endpoint names are maintained with the
stdlib migration. Multi-root operations list every constituent root explicitly;
for example `fs.write.copy` declares `fs.read`, `fs.write`, `stream.read`,
`stream.write`, and `stream.manage` when its implementation uses those
operations.

## Reporting and diagnostics

`vibra effects` exposes `surface.declared` and `surface.performed`, per-function
declared/performed rows, operation owner/additive rows, call edges, and primitive
witnesses containing input/result capability shapes and required roots. The
`performed` rows are the same least-fixed-point inference used for validation;
the report does not maintain a second call-graph definition. There are no
legacy effect fields or structural aliases in the report.

As a corpus migration check, the four tracked example entrypoints no longer
repeat their transitive effect unions on `main`: 15 effect labels were removed
from those annotations (15 to 0). Boundary declarations on ordinary exported
functions, interface methods, deffect operations, and foreign operations remain
explicit.

`E-EFFECT-001..007` retain their existing meanings where applicable. Native
surface diagnostics additionally cover malformed/duplicate `deffect`
declarations, intrinsic placement/registry/arity/type failures, and attempts
to forge or cast host-backed endpoint types.

## Non-goals

Effects are not handlers, parameterized rows, effect-polymorphic function
types, or runtime permissions. A generic combinator therefore keeps a fixed
declared ceiling; effect sets as type-level values belong with #151. Host
handles remain copyable for this migration; affine ownership and borrowing are
a follow-up design.
