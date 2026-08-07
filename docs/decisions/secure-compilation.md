---
title: Secure compilation boundary
category: decisions
status: accepted
updated: 2026-08-07
issue: 252
---

# Secure compilation boundary

This is the accepted contract for the first Vibra compilation boundary. It
adopts the constraints needed for later security reasoning without claiming a
mechanized proof or redesigning the ABI.

## `vibra_v1` is scalar-only on the compartment boundary

The `vibra_v1` compartment boundary is the emitted Vibra guest module and the
host implementation that services its imports. Every guest import is numeric:
the current guest module has no linear-memory section, and its imports use
`i32` values. Dynamic values are represented by nonzero indices into a
host-owned arena. The index is checked and resolved on the owning side; it is
not a guest pointer and does not grant access to host memory.

The names `str`, `bytes`, `array`, `record`, and the handle/result shapes in
the host registry describe host-arena values, not pointer layouts on the
guest-facing Wasm ABI. Mutable cells, references, function values, shared
references, and pointers do not cross this boundary. The lowering check
rejects those shapes directly and when nested in aliases or aggregate values.

Host-backed handles are opaque indices minted and resolved by the host. Copying
an index does not copy or expose the underlying resource, and no host handle is
represented as a shared reference or pointer.

The registry audit for this decision found the existing `vibra_v1` entries
already satisfy this model. The audit is kept executable by the registry test,
the emitted-import signature test, and the ABI rejection test.

## Static Wasm FFI is a separate, weaker boundary

Static dependency Wasm FFI is an explicit unsafe boundary from the host into
dependency-provided code. It is not part of the `vibra_v1` guest-to-host
isolation claim. Buffer-bearing FFI calls pass pointers and lengths through a
fresh host-owned `vibra_ffi.memory`; this is intentionally documented as a
weaker host-to-foreign-module boundary in
[`static-wasm-ffi.md`](../reference/static-wasm-ffi.md).

Its current mitigations are exact import allowlisting, a fresh Store, fresh
Memory, and fresh Instance for each call, checked wasm32 ranges and writes,
and call-scoped buffers that the callee must not retain. These mitigations are
not a shared-reference guarantee and do not turn static FFI into `vibra_v1`.

## Hardening is a terminal compiler stage

Compilation stages that establish reachability and emit Wasm precede the
hardening stage. Hardening passes form the final contiguous suffix of the
pipeline; a later layout-changing or semantic pass is a contract violation.
The ordering is represented in the backend and tested. The current pipeline
has no concrete hardening transform yet, so this invariant is a sequencing
constraint and not a proof of a security property.

## Claims and limits

Semantic preservation and attack prevention are separate claims. Vibra may
describe semantic preservation only for the behavior covered by its compiler,
runtime, and regression tests. This decision does not claim attack prevention,
memory safety, an isolation theorem, or that a future hardening pass stops a
particular attack. The static FFI exception remains outside the `vibra_v1`
claim for the same reason.

This contract is informed by the secure-compilation research notes and the
issue #252 ABI audit. It does not attempt mechanized proof, verified compiler
construction, or unrelated ABI redesign.
