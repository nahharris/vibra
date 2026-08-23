# Static wasm FFI fixture

Compile `foreign/math.wat` to `foreign/math.wasm` with a standard WAT tool,
then run `vibra check .` followed by `vibra run src/main.vib`. The check
resolves `@math`, opens the declared static artifact, and verifies that `sum`
has the wrapper's `(i32, i32) -> i32` signature before execution. Run embeds
the validated bytes in the compiled program, instantiates the module, and calls
`sum(20, 22)` through the typed wrapper. The complete ABI and safety contract is
in
[`docs/reference/static-wasm-ffi.md`](../../docs/reference/static-wasm-ffi.md).

For caller-owned string and byte buffers, the foreign artifact imports
`vibra_ffi.memory` and declares two `i32` parameters for each `$str` or direct
`$array: $uint8` wrapper argument. The project CLI integration fixture exercises
this contract with multibyte UTF-8 in both source and packaged execution.
