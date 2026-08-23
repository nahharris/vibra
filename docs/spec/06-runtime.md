# Vibra v1 runtime and WebAssembly

Status: normative target
Implementation status: not started

## Semantic authority

The language semantics are independent of a backend. A small reference
interpreter is the executable oracle for evaluation, types after erasure,
effect events, failures, resource scopes, and budgets. The production compiler
emits WebAssembly. A v1 implementation is conforming only when both produce the
same observable behavior for the conformance corpus.

The interpreter is not a second language implementation: it consumes the same
resolved typed IR as the Wasm backend. Parsing, name resolution, type checking,
effect inference, and host registry validation are shared.

## Evaluation

Evaluation is strict and deterministic:

- call arguments evaluate in resolved parameter order, then variadic order;
- function bodies and `do` forms evaluate from first to last;
- `if` evaluates only the selected branch;
- `match` evaluates its subject once and selects the first matching case;
- collection elements evaluate in source order; and
- `return`, `try`, `break`, and `continue` perform only their specified lexical
  control transfer.

Map iteration order is canonical key order, not hash-table order. String and
byte indexing is bounds-checked. Strings are Unicode scalar sequences at the
language level; byte conversion is explicit UTF-8.

Floating-point operations follow IEEE 754. Serialization and equality
canonicalize all NaN payloads to one quiet NaN per width and normalize negative
zero only where the relevant standard operation explicitly says so.

## Host registry

All host interaction passes through a closed `vibra_v1` registry. Each entry
declares:

- stable operation ID and owning standard effect root;
- exact argument and result types;
- resource-constraint schema;
- whether it creates, borrows, uses, or closes a resource; and
- deterministic budget charges and audit event shape.

Unknown imports and WASI imports are rejected before execution. User source
cannot add registry entries. Composite I/O behavior belongs in Vibra standard
library code over small registry primitives.

Host responses that are ordinary environmental outcomes use typed `result`
errors. ABI mismatch, impossible typed IR, invalid host value IDs, and runtime
invariant violation are traps. A trap has a stable code and source origin but
is not catchable by user code.

## WebAssembly boundary

The emitted module imports only from `vibra_v1`. The guest/host boundary is
scalar-only: values crossing it are numeric scalars or checked opaque indices
into an instance-owned host arena. Guest pointers, shared linear-memory
pointers, references, and resource internals do not cross the boundary.

The host validates every opaque index for instance, kind, liveness, and scope.
Index zero is invalid and IDs are not reused within an instance. The module
exports a versioned entry function and embeds deterministic custom sections for
source/build fingerprint, required registry entries, required effects, and
source-origin mapping.

An incompatible import signature, custom-section shape, value representation,
or entry contract requires `vibra_v2`; a runtime MUST NOT reinterpret it as v1.

## Resources

The runtime owns every host resource created for a program instance.
`with-resource` opens a lexical resource scope. Normal completion, `try`,
written return, budget exhaustion, permission denial after acquisition, and
trap all close resources in reverse creation order.

Close is idempotent inside runtime cleanup but user code has no raw close or
resource duplication primitive. A resource operation validates its lexical
scope and active grant. Borrowed console providers are scoped handles whose
cleanup does not close the embedding process stream.

At instance exit the host reports and closes any resource that escaped due to
an implementation defect. Conformance tests treat such a report as failure.

## Budgets

Every run has finite or host-explicitly-unbounded ceilings for:

- fuel;
- logical memory bytes; and
- simultaneously live host resources.

Project ceilings may be narrowed by CLI or embedding configuration and never
amplified implicitly. V1 fuel charges one unit at function entry, loop header,
match-case test, and host operation admission. The versioned host registry adds
declared charges for data-volume-dependent operations.

Logical memory accounting is deterministic and backend-independent. The v1
accounting table charges canonical serialized sizes for strings, bytes, and
collections plus fixed typed-value overheads published with the conformance
suite. Shared implementation storage MUST NOT reduce the charged logical size.
All memory is reclaimed at instance exit.

Budget exhaustion terminates the active program with a stable runtime result,
runs resource cleanup, and cannot be caught to continue unbounded work. A host
may impose a stricter external limit, but must identify that as host
termination rather than a Vibra semantic result.

## Determinism and observability

Given the same typed program, input values, grants, budgets, and ordered host
responses, a run produces the same:

- return value or failure;
- stdout/stderr bytes;
- ordered host-effect audit events;
- logical budget totals; and
- resource open/close trace.

Wall clock, environment, filesystem, and randomness are host inputs and are
observable only through granted registry operations. Test hosts inject them.
The runtime never reads ambient host state on behalf of pure code.

Compilation is deterministic: identical compiler version, typed program, and
options produce byte-identical Wasm and metadata. Optimization is permitted
only after unoptimized interpreter/Wasm parity exists, and every optimization
must preserve conformance observations. Security hardening, when introduced,
is the final contiguous compiler stage.

## Claims and limits

V1 claims conformance parity and enforced checks at the closed host boundary.
It does not claim verified compilation, attack prevention, constant-time
execution, host-provider correctness, native-code isolation, or safe execution
of arbitrary foreign Wasm.
