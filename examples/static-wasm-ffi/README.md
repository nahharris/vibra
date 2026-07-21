# Static wasm FFI fixture

Compile `foreign/math.wat` to `foreign/math.wasm` with a standard WAT tool,
then run `vibra check .` followed by `vibra run src/main.vibra`. The check
resolves `@math`, opens the declared static artifact, and verifies that `sum`
has the wrapper's `(i32, i32) -> i32` signature before execution. Run embeds
the validated bytes in the compiled program, instantiates the module, and calls
`sum(20, 22)` through the typed wrapper. The complete ABI and safety contract is
in
[`docs/static-wasm-ffi.md`](../../docs/static-wasm-ffi.md).
