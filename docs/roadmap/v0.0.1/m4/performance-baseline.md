# Performance baseline (Step 21)

The numbers are in [`performance-baseline.vibon`](performance-baseline.vibon), an
`@perf-baseline.v1` document that `perf-baseline --write` generates and nobody edits. v0.0.2 M3
compares its optimizations against it. This note says what the numbers are and how to
reproduce them. It claims no language behavior.

## Regenerating and checking

```text
cargo run --release --locked --offline -p vibra-conformance --bin perf-baseline -- --write
cargo run --locked --offline -p vibra-conformance --bin perf-baseline -- --check
```

`--write` measures all ten programs first and writes only if every one completed.
`--check` measures the exact metrics and exits 1 with one line per difference. CI runs
the same comparison as the host test `performance_baseline_m4_step21`, so a change that
moves an exact metric fails until the file is regenerated in the same pull request.
Medians never take part in any comparison.

## The programs

| Name | Source | Run as |
| --- | --- | --- |
| `deep-tail-walk` | `examples/m3-catalog`, the test "a deep walk over a line of stock" (10^5 rounds) | one test of the demo's module |
| `m3-demo-tests` | `examples/m3-catalog`, all eight tests, the walk among them | eight fresh instances |
| `non-tail-depth` | corpus case `V1-RUNTIME-activation-depth` (depth 10^5) | entry |
| `tail-loop` | `examples/m4-baseline/programs/tail-loop.vib` (10^6 iterations) | entry |
| `array-fold` | `.../array-fold.vib` (1,000 elements) | entry |
| `dict-insert` | `.../dict-insert.vib` (`dict.of` with 200 out-of-order keys) | entry |
| `text-concat` | `.../text-concat.vib` (2,000 `text.concat` calls) | entry |
| `float-to-str` | `.../float-to-str.vib` (1,000 `f64.to-str` calls) | entry |
| `nested-value-5000` | `examples/m4-baseline/nested` (build, compare, release) | one test |
| `effectful-walk` | corpus case `V1-RUNTIME-io-effectful-iter-walk`, default host world | entry |

Both backends run under the conformance runner's limit of 64 MiB.

## Exact metrics and where they come from

| Backend | Metric | Source |
| --- | --- | --- |
| Wasm | module bytes | `vibra_wasm::emit`, a pure function of the program |
| Wasm | fuel | Wasmtime fuel, from a runner built by `Runner::with_fuel`; the conformance runner meters none |
| Wasm | allocations, peak live bytes | two state fields (addresses 112 and 120) that only a module from `emit_measured` writes; `emit` never emits those instructions, so a normal module's bytes are unchanged |
| interpreter | allocations, reductions | counters in the machine: values the accounting counts, and expressions reduced |
| interpreter | peak bytes | the machine's existing `peak_memory_bytes` |

Fuel and module bytes are taken from the normal module; allocations and the peak from
the measured module, which computes the same result (a host test checks this).
For a test program the Wasm numbers sum over the selected tests, and the peaks are the
largest of them; module bytes are those of the whole test module.

All of these are integers and none depends on a path, a hash order, or a time. They
are profile-independent: a debug and a release build agree. Wasmtime fuel counts
WebAssembly operators and the module bytes come from the pure emitter, so the Cranelift
target does not enter. The interpreter's byte figures come from `size_of` of its own
types, which is the same on the three 64-bit CI targets. No metric is per-platform. This
rests on those arguments and on a Windows run; the first CI run on Ubuntu and macOS is
the check, and a disagreeing metric would move to the recorded set.

Two limits of the interpreter figures, recorded for v0.0.2 M3: its peak counts values only at
its measurement points (a measurement is taken when enough has been allocated), so for a
program of values it is a lower bound and small (1,176 bytes for the 5,000-deep value,
where the module's peak is 2.5 MB); and `dict.of`, a native, is one reduction.

## Recorded, not checked

The medians are over 11 runs (microseconds) and are taken in the build that wrote the
file (`release`), with the CPU, OS, Rust, and Wasmtime versions beside them. The
interpreter's time is one run of the checked program. The Wasm time is validation,
compilation, instantiation, and run of the emitted module, which includes Cranelift
compilation, so it is not a measure of generated-code speed alone. A comparison across
machines is excluded.

## The nested value (ledger D14.2)

Reproduced first. On the pre-M4 interpreter (`2e7d257`, debug), building a value nested
`n` deep took 2.1 s, 11 s, and 56 s for `n` = 1,250, 2,500, and 5,000, about five times
more per doubling, so the reported seven minutes with a comparison follows. The cause is
the one the ledger names: every read of a binding copied the whole structure. Step 3
shares the parts of compound values, and the same build now takes 79 ms at 5,000 and
322 ms at 20,000, linear in `n` (3 allocations and 15 reductions per level). It is not
a defect in the current tree, so there is nothing to fix; the program is in the
baseline and a host test bounds its debug-build time.

## Known costs recorded for v0.0.2 M3

- `array.fold`'s library body slices at every step, so a module's fold is quadratic in
  copies (ledger D10.5). The 1,000-element fold costs 12.5 M fuel and 4,008
  allocations. Not rewritten here.
- Every checked arithmetic call allocates its `result` in a module: the tail loop of
  10^6 iterations makes 3 million allocations (3 per iteration) while its live arena
  stays at 16,512 bytes. An optimization for unshared or unboxed results is v0.0.2 M3's, to be
  measured against this row.
- Canonical equality is a type-directed walk (ledger D8.4); the type-free fast path is
  v0.0.2 M3's.
