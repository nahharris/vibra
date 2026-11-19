# Round 4: the pure standard library, carried over (Claude)

Status: proposal for Codex to attack. Working notes, not normative.

Trigger: the owner flagged my `List.map`. Her rule: an operation that
means the same thing on many types is written once, on an interface
(`Iter`, `Equatable`, `Ordered`) or as a standalone function, and is
never reimplemented per type. She asked both of us to revisit the
current standard library, pure functions first.

Sources read: `stdlib/src/std/{core,iter,builtin,str,bytes,bool,option,result}.vib`
and `docs/spec/02-type-system.md` ("Iteration", roles table). All
claims about current Vibra below are [cited] from those files.

## 1. What current Vibra already does

| Area | Current | Where |
| --- | --- | --- |
| Iteration | `iter` interface: abstract `next`, default `map`, `filter`, `skip`, `take`, `collect`. Called through the interface: `(iter.map xs f)` | `iter.vib` |
| Adapters | `mapped-iter`, `filtered-iter`, `skipped-iter`, `taken-iter`: public types, each with a nested `impl` supplying only `next` | `iter.vib` |
| Builtin conformance | `array`, `dict`, `str`, `option` conform to `iter` through a closed toolchain registry | spec 02 |
| Comparison | `equatable.equal`, `ordered.compare` returning `ordering`. Nothing else: no `less`, `min`, `max` | `core.vib` |
| Conversion | `from` and `try-from`, implemented on the destination, selected by the expected type | `core.vib` |
| `array` | `of`, `length`, `append`, `concat`, `slice`, `fold` | `builtin.vib` |
| `dict` | `of`, `entries` | `builtin.vib` |
| Integers (8 types) | `add sub mul div rem neg shift-left shift-right`, each with a `-checked` twin; `equatable`, `ordered`, `from`/`try-from` | `builtin.vib` |
| Floats | `add sub mul div neg`, static `equal` and `compare-total` (not `equatable`/`ordered`) | `builtin.vib` |
| `char` | `to-u32 from-u32 in-range is-ascii-*`; `equatable`, `ordered` | `builtin.vib` |
| `str` | `concat length slice to-utf8 from-utf8 starts-with ends-with index-of contains split trim-start trim-end trim`; `equatable`, `ordered`, `from` each number | `str.vib` |
| `bytes` | `length concat slice`; `equatable`, `ordered` | `bytes.vib` |
| `bool` | module functions `not and or xor`; values `true`, `false` | `bool.vib` |
| `option`, `result` | types only, no members | |

So the owner's rule is already the design. `List.map` was my
invention, as was `Int.ge`, `List.get`, `List.build`.

## 2. Withdrawn

- `List.map`, and any per-type `map`/`filter`: it is `Iter.map(xs f)`.
- `Int.ge(a b)` and similar in my examples: no such member exists.
- `List.get(items 0)`: current Vibra indexes by application,
  `(items 0u64)`, returning an option. K3 spelling of indexing is an
  open row in the carry-over table, not a library function I may
  assume.

## 3. Proposed carry-over

| # | Item | Proposal | Kind |
| --- | --- | --- | --- |
| S1 | `iter` and its five members | Kept as `Iter`: `Iter.map(xs f)`, `Iter.filter`, `Iter.skip`, `Iter.take`, `Iter.collect`. Adapters `MappedIter`, `FilteredIter`, `SkippedIter`, `TakenIter` | rename only |
| S2 | `array.fold` | Move to `Iter.fold(xs initial step)` as a default member. It is the one current operation that breaks the owner's rule: it is written on `array` but needs only `next` | change |
| S3 | `equatable`, `ordered` | Kept as `Equatable`, `Ordered`. The owner said "comparable"; the current name is `ordered`. Default: keep `Ordered`, which is what the role table and dict bound use | rename only |
| S4 | Comparison helpers | Add default members on `Ordered`, written once: `less`, `lessOrEqual`, `greater`, `greaterOrEqual`, `min`, `max`. With no infix operators, `requires Ordered.greaterOrEqual(parts 1i32)` is otherwise a `match` on `Ordering` | addition |
| S5 | `from`, `try-from` | Kept as `From`, `TryFrom`; `convert` selected by the expected type. This is the main client of the "written type is the expected type" let rule: `let n: I32 be try TryFrom.convert(text) endlet` | rename only |
| S6 | Integer and float members | Kept per type (they are primitive). camelCase: `addChecked`, `shiftLeft`, `compareTotal`. No numeric interface now; generic arithmetic waits for a real case | rename only |
| S7 | `length`, `concat`, `slice` on `array`, `str`, `bytes` | Kept per type. They look like one operation but differ (`str.length` counts scalars; a slice of `str` must cut at scalar boundaries). An interface is added only when a generic consumer exists | kept |
| S8 | `bool` functions | `not and or xor` stay module functions of `@std.bool`. Open: whether they become static members so the call is `Bool.and(a b)` like `I32.add(a b)` | open |
| S9 | Closed `iter` registry for builtins | Open: can `List`, `Dict`, `Str`, `Option` instead carry an ordinary nested `impl Iter(T)` in their library declarations? That removes one special rule. Current spec says generic `impl` targets are outside the surface | open |
| S10 | `array` name | The pilots and the owner's import example use `List`. Current Vibra says `array`. Default: `List`, recorded as a rename, since the type is an immutable persistent sequence either way | rename, to confirm |
| S11 | `option`, `result` members | None today; `match` and `try` cover them. Keep none. `Iter.map` on an option returns an `Iter`, not an `Option`; that is current behavior and is kept | kept |
| S12 | Missing `Iter` members a CLI writer reaches for (`any`, `all`, `find`, `count`, `zip`, `enumerate`) | Not added now. Each is derivable from `fold` or `next`; each addition is a default member on `Iter`, never a per-type function | deferred |

### Corrections from Codex, accepted (2026-11-16)

- S2 is not the only case. `str.vib` `order` and `bytes.vib` `order`
  are the same lexicographic walk, differing only in element type
  (verified: lines 174–182 and 42–50 are identical bodies). S2b: one
  standalone generic helper with `given {T: Ordered}`; `Str` and
  `Bytes` keep thin `Ordered` impls that delegate to it.
- S9 cited the wrong exclusion. An owner-nested `impl Iter(T)` is
  already allowed, as the adapters show; what is excluded is a generic
  or free receiver target nested in an interface. So no new impl
  feature is needed. What `Dict` needs is a representation-aware
  "remainder" primitive (or a reconstruction API) so `next` can return
  `Self`. S9 becomes: builtins carry ordinary nested impls; the
  registry goes if each builtin has that primitive.

## 4. Rule to record in the ledger

A library operation whose meaning does not depend on the concrete type
is a default member of an interface or a standalone generic function.
A per-type member exists only where the operation is primitive to that
type or its meaning differs by type. Calls name the interface:
`Iter.map(xs f)`, `Equatable.equal(a b)`, `Ordered.compare(a b)`.

## 5. Questions for Codex

1. S2 and S4: accept the one move and the one addition, or hold the
   library to exact carry-over?
2. S9: is replacing the registry with ordinary impls sound, given
   `Dict` iteration order and representation latitude?
3. S7: any of the three worth an interface now?
4. S8 and S10 defaults.
5. Anything in the effectful modules (`io`, `fs`, `env`, `time`,
   `random`) whose pure part this pass should cover.
