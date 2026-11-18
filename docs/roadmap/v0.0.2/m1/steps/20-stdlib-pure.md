# Step 20: stdlib-pure

Kind: vertical. Slice 6: the pure standard library completed. `Str` and
`Bytes` with their members and conversions, `Dict` completed, `Iter` with
`fold` and the lazy adapters, the nested `Iter` implementations for `List`,
`Dict`, `Str`, `Bytes`, `Option`, `Ordered` defaults with one comparison and
left-biased ties, `Bool` operations, `Char` classifiers, `Str.trim`, the
floating-point operations, and `Str`'s `From`/`TryFrom` with every number
type. No operation traps.

## Prerequisites

- Step 19 landed.
- Read: [type chapter](../../../../spec/02-type-system.md) "Language core
  and standard library", "Dict keys, equality, and order" (canonical
  order), "Iteration" (members, laws, implementations), "Conversion";
  [runtime chapter](../../../../spec/06-runtime.md) "Integer operations"
  (the interface table), "Floating-point operations", "Character, sequence
  and dictionary operations", "Native implementations";
  [diagnostics chapter](../../../../spec/07-diagnostics-and-conformance.md)
  "Required coverage" item 8.
- Open questions: Q15 (empty `native` list) answered; Q17 (`Str.trim` and
  the `Char` classifier member names are not listed by the chapter).

## Entry points

| Existing | Use |
| --- | --- |
| `stdlib-v1/src/std/{str,bytes,iter,builtin}.vib` | the member inventory to re-express; iter.vib shows the adapter structure |
| `crates/vibra-interp/src/intrinsics.rs` (step 13) | every intrinsic the members bind |
| `crates/vibra-types/src/stdlib.rs` (step 14) | the loader admits the new modules |

Proposed: `stdlib/src/std/{str,bytes}.vib`; `@std.iter` completed;
`@std.builtin` completed.

## Ordered tasks

1. `Str`: `type Str of List(Char) role @str representation @private …` with
   `length`, `concat`, `slice`, `toUtf8`, `fromUtf8`, `trim`, `Equatable`,
   `Ordered` (through the shared lexicographic helper over two `Iter`
   values with `Ordered` items), `Iter` (`Item` is `Char`), `From(N)` for
   every number type, and the constructor and pattern `Str(scalars)`
   accessible inside the module only.
2. `Bytes`: `of List(U8)`, `length`, `concat`, `slice`, `Equatable`,
   `Ordered`, `Iter` (`Item` is `U8`).
3. Numbers: `TryFrom(Str)` on every integer and float type (decimal
   grammar; `invalidFormat`, `outOfRange`); `From(F32)`/`From(F64)` on
   `Str`; the float static members `add`, `sub`, `mul`, `div`, `neg`,
   `equal`, `compareTotal`; floats implement neither `Equatable` nor
   `Ordered`.
4. `Char`: `toU32`, `fromU32`, the ASCII classifiers (Q17 names), using
   `Ordered.inRange`.
5. `Bool`: `not`, `and`, `or`, `xor` eager (already in 14; cases here).
6. `Iter`: `fold(Acc)` strict left; `map`, `filter`, `skip`, `take` lazy
   through the adapter types; `collect`; implementations for `List`,
   `Dict` (items `{K V}` in key order through the dict iteration
   primitive), `Str`, `Bytes`, `Option`; `Result` has none.
7. `Dict`: `entries`, `From(List({K V}))`.
8. Loader: the manifest lists eight modules; `native: []` stays empty.
9. Cases: each law of the iteration section as an `interpret` case with a
   small finite iterator and, for laziness, an endless one.

## Test matrix

| Case id | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-TYPE-INTERFACE-str-members` | positive, interpret | `{Str.length("héllo") Str.concat("a" "b") Str.slice("hello" 1u64 3u64) Str.slice("hello" 3u64 1u64)}` | `{5u64 "ab" Option.some(value: "el") Option.none()}` |
| `V2-TYPE-INTERFACE-str-utf8` | positive, interpret | `Str.fromUtf8(Str.toUtf8("é"))` and `Str.fromUtf8(Bytes([255u8]))` | `Result.ok(value: "é")`, `Result.err(error: ConversionError.invalidFormat())` |
| `V2-TYPE-INTERFACE-str-trim` | positive, interpret | `Str.trim("  a b\t")` | `"a b"` |
| `V2-TYPE-INTERFACE-str-order` | positive, interpret | `Ordered.compare("ab" "b")`, `Equatable.equal("a" "a")` | `Ordering.less()`, `true` |
| `V2-TYPE-INTERFACE-str-representation-private` | negative | `Str(['a'])` outside `@std.str` | `@name.inaccessibleRepresentation` |
| `V2-TYPE-INTERFACE-str-lookup` | positive, interpret | `"héllo"(1u64)` | `Option.some(value: 'é')` |
| `V2-TYPE-INTERFACE-bytes-members` | positive, interpret | `Bytes.length(Str.toUtf8("é"))`, `Bytes.slice(b 0u64 1u64)` | `2u64`, `Option.some(value: …)` |
| `V2-TYPE-CONVERT-parse-integer` | positive, interpret | `let a: Result(I8 ConversionError) be TryFrom.convert("-12") endlet` and `"300"`, `"1_0"`, `"+1"` | `ok -12i8`, `outOfRange`, `invalidFormat`, `invalidFormat` |
| `V2-TYPE-CONVERT-parse-unsigned-minus` | positive, interpret | `TryFrom.convert("-1")` at `U8` | `invalidFormat` |
| `V2-TYPE-CONVERT-render-integer` | positive, interpret | `let s: Str be From.convert(-7i32) endlet` | `"-7"` |
| `V2-TYPE-CONVERT-float-text` | positive, interpret | `From.convert(100.0f64)`, `From.convert(0.0025f64)`, `From.convert(1e20f64)` at `Str` | `"100.0"`, `"0.0025"`, `"1e20"` |
| `V2-TYPE-CONVERT-float-parse` | positive, interpret | `TryFrom.convert("1.5")`, `"1e400"`, `"nan"` at `F64` | `ok 1.5f64`, `outOfRange`, `invalidFormat` |
| `V2-TYPE-INTERFACE-float-not-ordered` | negative | `Ordered.less(1.0f64 2.0f64)` | `@type.mismatch` (no implementation) |
| `V2-RUNTIME-float-ops` | positive, interpret | `F64.div(1.0f64 0.0f64)`, `F64.equal(F64.div(0.0f64 0.0f64) F64.div(0.0f64 0.0f64))` | `inf`, `false` |
| `V2-RUNTIME-saturating` | positive, interpret | `{I8.add(127i8 1i8) U8.sub(0u8 1u8) I32.shiftLeft(1i32 40u32) I8.shiftRight(-1i8 9u32)}` | `{127i8 0u8 2147483647i32 -1i8}` |
| `V2-RUNTIME-checked` | positive, interpret | `{I32.divChecked(1i32 0i32) I32.divChecked(-2147483648i32 -1i32) U8.shiftLeftChecked(1u8 8u32)}` | `{err divisionByZero err overflow err invalidShift}` |
| `V2-TYPE-INTERFACE-char` | positive, interpret | `{Char.toU32('A') Char.fromU32(55296u32)}` | `{65u32 Option.none()}` |
| `V2-TYPE-INTERFACE-bool-eager` | positive, interpret | `Bool.and(false trace())` where `trace` returns `true` through a visible module value count is not possible purely, so: a host test on the interpreter trace; the case asserts `Bool.xor(true true)` is `false` | `false` |
| `V2-TYPE-INTERFACE-ordered-defaults` | positive, interpret | `{Ordered.min(1i32 1i32) Ordered.max(2i32 1i32) Ordered.inRange(5i32 1i32 5i32) Ordered.lessOrEqual(2i32 1i32)}` | `{1i32 2i32 true false}`; host test: one `compare` per pair |
| `V2-TYPE-INTERFACE-iter-collect` | positive, interpret | `Iter.collect([1i32 2i32])` | `[1i32 2i32]` |
| `V2-TYPE-INTERFACE-iter-fold` | positive, interpret | `Iter.fold([1i32 2i32 3i32] "" fn takes {acc: Str x: I32} returns r: Str do return Str.concat(acc From.convert(x)) endfn)` | `"123"` (left, in order) |
| `V2-TYPE-INTERFACE-iter-fold-empty` | positive, interpret | `Iter.fold([] 7i32 …)` with `[]` typed `List(I32)` | `7i32` |
| `V2-TYPE-INTERFACE-iter-map-filter` | positive, interpret | `Iter.collect(Iter.filter(Iter.map([1i32 2i32 3i32] double) isEven))` | `[2i32 4i32 6i32]` |
| `V2-TYPE-INTERFACE-iter-take-skip-split` | positive, interpret | `{Iter.collect(Iter.take(xs 2u64)) Iter.collect(Iter.skip(xs 2u64))}` for `[1i32 2i32 3i32]` | `{[1i32 2i32] [3i32]}` |
| `V2-TYPE-INTERFACE-iter-lazy` | positive, interpret | an endless user iterator `Naturals`; `Iter.collect(Iter.take(Naturals(0u64) 3u64))` | `[0u64 1u64 2u64]`; terminates |
| `V2-TYPE-INTERFACE-iter-next-pure` | positive, interpret | `Iter.next(xs)` twice on one value | equal results |
| `V2-TYPE-INTERFACE-iter-dict` | positive, interpret | `Iter.collect(["b": 1i32 "a": 2i32])` | `[{"a" 2i32} {"b" 1i32}]` |
| `V2-TYPE-INTERFACE-iter-str-option` | positive, interpret | `Iter.collect("ab")`, `Iter.collect(Option.some(value: 1i32))`, `Iter.collect(Option.none())` typed | `['a' 'b']`, `[1i32]`, `[]` |
| `V2-TYPE-INTERFACE-iter-result-none` | negative | `Iter.collect(Result.ok(value: 1i32))` | `@type.mismatch` (no implementation) |
| `V2-TYPE-INTERFACE-iter-remainder-type` | positive | `let step: Option({U8 Once}) be Iter.next(once) endlet` | accepted: the remainder has the receiver's own type |
| `V2-TYPE-INTERFACE-dict-entries` | positive, interpret | `Dict.entries(["b": 1i32 "a": 2i32])` | `[{"a" 2i32} {"b" 1i32}]` |
| `V2-TYPE-CONVERT-dict-from-list` | positive, interpret | `let d: Dict(Str I32) be From.convert([{"a" 1i32} {"a" 2i32}]) endlet` | one entry `"a": 2i32` |

## Diagnostics and schema changes

Codes: `@type.mismatch`, `@name.inaccessibleRepresentation`. No schema
change. The manifest gains `@std.str` and `@std.bytes`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-interp -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: all cases pass; a host test runs the intrinsic sample vectors of
`vibra-ir` through the interpreter.

## Excluded

Native acceleration (v0.0.3); `@std.assert` (23); host modules (M2).

## Completion evidence

- [ ] Every member in "Character, sequence and dictionary operations" has a case
      or a host test named after it.
- [ ] Every law of "Iteration" has a case.
- [ ] `stdlib/manifest.vibon` lists eight modules with `native: []`.
- [ ] Validation commands exit 0.
