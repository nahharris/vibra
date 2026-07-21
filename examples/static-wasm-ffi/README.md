# Static wasm FFI fixture

Compile `foreign/math.wat` to `foreign/math.wasm` with a standard WAT tool,
then run `vibra check .`. The check resolves `@math`, opens the declared static
artifact, and verifies that `sum` has the wrapper's `(i32, i32) -> i32`
signature before any execution occurs.

Execution/linking of external artifacts is a follow-up milestone; the current
fixture demonstrates the implemented manifest and pre-execution validation
slice. The complete ABI and safety contract is in
[`docs/static-wasm-ffi.md`](../../docs/static-wasm-ffi.md).
