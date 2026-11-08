# Milestone 2 — Native parity

Windows x64, Linux x86-64, and macOS arm64 execute the entire supported Vibra
corpus with the same observable behavior as the reference interpreter.

## Delivery order

1. Expand the slice by executable feature. Cover scalar and checked
   arithmetic, structural and nominal aggregates, pattern matching, generic
   instantiation, closures and indirect/interface dispatch, recursion, module
   initialization, assertions, and the existing host operations. Each
   slice includes interpreter/native comparisons and rejection cases.
2. Exercise all three targets with canonical results, traps and origins, test
   outcomes, CLI entry results, and ordered host audit events. Use fresh
   equivalent deterministic worlds for differential runs and real process
   launches for platform adapter smoke cases. On Windows, exercise paths
   with spaces and non-ASCII characters, argument passing, standard streams,
   filesystem effects, and process exit status through the native launcher.
3. Check execution limits and backend boundaries. Deep non-tail recursion
   consumes runtime-managed frame memory. Forced yields around calls and
   long primitives preserve ordered effects and exactly-once execution
   across resume. Invalid bindings, unsupported targets, and incompatible
   artifacts have structured diagnostics.

## Gates

- **Demo.** Run an effectful, multi-module fixture on all three platforms with
  forced yields, a mapped trap variant, and the same canonical outcomes and
  audit as the interpreter.
- **Exit.** The complete supported executable corpus passes on all three targets.
  An emission failure on an admitted case is a parity failure. Existing Wasm
  coverage and the v0.0.2 agent workflow still pass. Record runtime and
  compiler boundary audits and an unoptimized performance baseline.
