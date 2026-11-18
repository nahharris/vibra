# Step 19: check-interfaces

Kind: vertical. The interface half of slice 5: interfaces with abstract and
default members, nested `impl` in the type and in the interface, the orphan
rule, completeness and overlap, receiver and destination dispatch,
associated types, interface values, the two widenings, `Any`, bounds, and
conversion through `From` and `TryFrom`, with the integer implementations of
`Equatable`, `Ordered`, `From`, and `TryFrom` in the standard library.

## Prerequisites

- Step 18 landed.
- Read: [type chapter](../../../../spec/02-type-system.md) "Interfaces"
  whole (Implementations, Associated types, Dispatch, Interface values,
  Overlap), "Generics" (bounds), "Widening", "Conversion", "Dict keys,
  equality, and order"; [runtime chapter](../../../../spec/06-runtime.md)
  "Generic instantiation", "Integer operations" (the interface table);
  [source chapter](../../../../spec/01-source-language.md) "`interface`",
  "`impl`".
- Open questions: Q12 (prelude membership of `From`/`TryFrom`), Q14
  (`I.Item` on an unbounded parameter). Both must be answered before this
  step starts.

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-types-v1/src/interfaces.rs` | completeness, overlap by unification, destination dispatch: the algorithms are re-expressed |
| `crates/vibra-types/src/{declare,expr,lower}.rs` | extend |
| `stdlib/src/std/{core,builtin,bool}.vib` | implementations land here |

Proposed: `vibra-types::{interfaces, dispatch, widen}`.

## Ordered tasks

1. Interface declarations: abstract members (no body), default members,
   associated types, `given` bounds on parameters and associated types,
   `Self`. A member selectable by neither receiver nor destination is
   `@type.undispatchableContractMember` at its declaration.
2. Implementations: in the `type` (target an interface,
   `@name.wrongEntityKind` otherwise) or in the `interface` (target a
   declared or builtin concrete type; an anonymous type, interface, or
   `Never` is `@name.wrongEntityKind`; a same-module type is
   `@type.redundantImplementation`); every abstract member exactly once
   after `Self` substitution (`@type.missingAbstractMember`), no default
   redeclared (`@type.defaultOverride`), no extra member
   (`@name.memberCollision` is resolution's; the checker reports
   `@type.mismatch` at the extra member against the contract), signatures
   equal to the contract's; an `impl` block declares no generics; every
   associated type chosen exactly once (`@type.missingAbstractMember`);
   applied targets of one receiver pairwise non-unifiable
   (`@type.overlappingImplementation`).
3. Dispatch: receiver dispatch on a `Self` positional parameter;
   destination dispatch from the written expected type
   (`@type.ambiguousDestination` without one); two candidates
   `@type.ambiguousImplementation`; a contract member named as a value is
   instantiated from its written expected callable type.
4. Associated types: `Once.Item`, `I.Item` with `given {I: Iter}` (Q14 for
   an unbounded `I`), labels in type expressions `Iter(Item: U8)`
   (`@type.typeArgumentMismatch` for an unknown label), bounds
   `given {I: Iter(Item: U8)}`.
5. Interface values: an interface in type position; generic interfaces
   written applied with every associated type fixed
   (`@type.typeArgumentMismatch`); member calls through a value; `Self` in
   the result replaced by the interface value type in the supported
   positions; a member taking `Self` elsewhere is `@type.mismatch` through
   a value; an interface value implements nothing and never widens again;
   `Any`.
6. Widening: concrete to interface and atom singleton to `Atom`, only at
   the five written expected types, never through a container or on an
   interface value; a value that needs widening elsewhere is
   `@type.mismatch`.
7. Bounds: `@type.unsatisfiedBound` with the bound related.
8. Conversion: `From(Source)` and `TryFrom(Source)` destination dispatched;
   `@type.redundantConversion` when one receiver's `From` and `TryFrom`
   sources unify.
9. Standard library: `Equatable` and `Ordered` for the integer types,
   `Char`, `Bool`; `From` and `TryFrom` among integer types per the width
   rule; structural `Equatable`/`Ordered` for atoms and admissible tuples
   and records (synthesized by the checker, executed by the interpreter's
   canonical comparison). Dict keys use them.
10. Cases.

## Test matrix

| Case id | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-TYPE-INTERFACE-impl-in-type` | positive, interpret | the `Printable`/`User` example; `Printable.render(User(name: "Ada" id: UserId(1u64)))` | `"Ada"` |
| `V2-TYPE-INTERFACE-impl-in-interface` | positive, interpret | `interface Printable … impl Celsius fn render … endfn endimpl endinterface` with `Celsius` in another module of the package | accepted; dispatch selects it |
| `V2-TYPE-INTERFACE-impl-same-module-redundant` | negative | `impl Celsius` inside `Printable` when `Celsius` is declared in the same module | `@type.redundantImplementation` at the `impl` |
| `V2-TYPE-INTERFACE-impl-target-interface` | negative | `type A of U8 impl Printable2 endimpl endtype` where `Printable2` is a type | `@name.wrongEntityKind` |
| `V2-TYPE-INTERFACE-impl-anonymous` | negative | `impl {I32 I32}` inside an interface | `@name.wrongEntityKind` at the `impl` |
| `V2-TYPE-INTERFACE-impl-never` | negative | `impl Never` inside an interface | `@name.wrongEntityKind` |
| `V2-TYPE-INTERFACE-missing-member` | negative | an `impl Printable` without `render` | `@type.missingAbstractMember` at the `impl` |
| `V2-TYPE-INTERFACE-default-override` | negative | an `impl Ordered` that defines `less` | `@type.defaultOverride` at `less` |
| `V2-TYPE-INTERFACE-signature-mismatch` | negative | `render` returning `U64` | `@type.mismatch` at the member |
| `V2-TYPE-INTERFACE-default-member` | positive, interpret | `Ordered.less(1i32 2i32)` | `true` |
| `V2-TYPE-INTERFACE-receiver-dispatch` | positive, interpret | two types implementing `Printable`; `render` called on each | each body |
| `V2-TYPE-INTERFACE-destination-dispatch` | positive, interpret | `let v: Celsius be From.convert(21i32) endlet` with `impl From(I32)` in `Celsius` | the converted value |
| `V2-TYPE-INTERFACE-ambiguous-destination` | negative | `let v be From.convert(21i32) endlet` | `@type.ambiguousDestination` at the application |
| `V2-TYPE-INTERFACE-undispatchable` | negative | `interface Maker fn make returns r: I32 endfn endinterface` | `@type.undispatchableContractMember` at `make` |
| `V2-TYPE-INTERFACE-member-as-value` | positive | `let f: Fn({Celsius} Str) be Printable.render endlet` | accepted |
| `V2-TYPE-INTERFACE-member-as-value-ambiguous` | negative | `let f be Printable.render endlet` | `@type.ambiguousInference` |
| `V2-TYPE-INTERFACE-generic-twice` | positive, interpret | `Celsius` implementing `From(I16)` and `From(I8)` | both convert |
| `V2-TYPE-INTERFACE-overlap` | negative | `impl From(T)` and `impl From(I8)` on one receiver | `@type.overlappingImplementation` |
| `V2-TYPE-INTERFACE-assoc-once` | positive, interpret | the `Once` iterator example; `Iter.next(Once(Option.some(value: 1u8)))` | `Option.some(value: {1u8 Once(Option.none())})` |
| `V2-TYPE-INTERFACE-assoc-missing` | negative | `impl Iter` without `type Item of …` | `@type.missingAbstractMember` |
| `V2-TYPE-INTERFACE-assoc-path` | positive | `fn f takes {x: Once.Item} returns r: U8 do return x endfn` | accepted; `Once.Item` is `U8` |
| `V2-TYPE-INTERFACE-assoc-param-path` | positive | `fn first(I) takes {it: I} returns r: Option(I.Item) given {I: Iter} …` | accepted |
| `V2-TYPE-INTERFACE-assoc-unbounded` | negative | the same without `given` | the Q14 code at `I.Item` |
| `V2-TYPE-INTERFACE-assoc-label` | positive | `given {I: Iter(Item: U8)}` and a call with `Once` | accepted |
| `V2-TYPE-INTERFACE-assoc-label-unknown` | negative | `Iter(Thing: U8)` | `@type.typeArgumentMismatch` |
| `V2-TYPE-INTERFACE-value-widen` | positive, interpret | `let p: Printable be User(…) endlet return Printable.render(p)` | `"Ada"` |
| `V2-TYPE-INTERFACE-value-open-assoc` | negative | `let i: Iter be once endlet` | `@type.typeArgumentMismatch` |
| `V2-TYPE-INTERFACE-value-self-result` | positive, interpret | `let i: Iter(Item: U8) be Once(…) endlet Iter.next(i)` | `Option({U8 Iter(Item: U8)})` encoding per the chapter |
| `V2-TYPE-INTERFACE-value-self-input` | negative | `Equatable.equal(e e)` with `e: Equatable` | `@type.mismatch` |
| `V2-TYPE-INTERFACE-value-no-widen-again` | negative | `let a: Any be p endlet` with `p: Printable` | `@type.mismatch` |
| `V2-TYPE-INTERFACE-widen-container` | negative | `let l: List(Printable) be [User(…)] endlet` | `@type.mismatch` at the element |
| `V2-TYPE-INTERFACE-widen-inferred-not-written` | negative | `let x be if b then user else celsius endif endlet` | `@type.mismatch` (no common supertype) |
| `V2-TYPE-INTERFACE-widen-atom` | positive, interpret | `fn f takes {a: Atom} returns r: Atom do return a endfn` called with `@ok` | `@ok` |
| `V2-TYPE-INTERFACE-any` | positive | `fn pass takes {a: Any} returns r: Any do return a endfn` called with `1i32` | accepted |
| `V2-TYPE-GENERIC-bound-satisfied` | positive, interpret | `fn show(T) takes {v: T} returns r: Str given {T: Printable} do return Printable.render(v) endfn` with `User` | `"Ada"` |
| `V2-TYPE-GENERIC-bound-unsatisfied` | negative | `show(1i32)` | `@type.unsatisfiedBound` at `1i32`, related to the bound |
| `V2-TYPE-CONVERT-from-integer` | positive, interpret | `let w: I32 be From.convert(1i8) endlet` | `1i32` |
| `V2-TYPE-CONVERT-tryfrom-integer` | positive, interpret | `let n: Result(I8 ConversionError) be TryFrom.convert(300i32) endlet` | `Result.err(error: ConversionError.outOfRange())` |
| `V2-TYPE-CONVERT-no-from-narrowing` | negative | `let n: I8 be From.convert(1i32) endlet` | `@type.ambiguousDestination` or `@type.mismatch`: no implementation exists, so `@type.mismatch` at the application |
| `V2-TYPE-CONVERT-redundant` | negative | one receiver with `impl From(I8)` and `impl TryFrom(I8)` | `@type.redundantConversion` |
| `V2-TYPE-NOMINAL-dict-user-key` | positive, interpret | a declared type with `impl Ordered`; a dict keyed by it | entries in that order |
| `V2-TYPE-NOMINAL-dict-key-no-ordered` | negative | a declared type without `Ordered` as a key | `@type.invalidDictKey` |
| `V2-TYPE-INTERFACE-equatable-tuple` | positive, interpret | `Equatable.equal({1i32 "a"} {1i32 "a"})` | `true` |

## Diagnostics and schema changes

Codes: `@type.redundantImplementation`, `@name.wrongEntityKind`,
`@type.missingAbstractMember`, `@type.defaultOverride`, `@type.mismatch`,
`@type.ambiguousDestination`, `@type.undispatchableContractMember`,
`@type.ambiguousInference`, `@type.overlappingImplementation`,
`@type.ambiguousImplementation`, `@type.typeArgumentMismatch`,
`@type.unsatisfiedBound`, `@type.redundantConversion`,
`@type.invalidDictKey`, and the Q14 code. No schema change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-interp -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: all cases pass; a host test shows overlap is decided without
consulting implementations elsewhere (adding an `impl` in another module
does not change the verdict).

## Excluded

`Str`, `Bytes`, the `Iter` adapters and `fold`, `Str`'s conversions (20);
guards on contract members (22); `proc` members (M2).

## Completion evidence

- [ ] `conformance/rules.tsv` has `V2-TYPE-INTERFACE`, `V2-TYPE-CONVERT` active.
- [ ] Every integer type has `Equatable`, `Ordered`, and its `From`/`TryFrom`
      rows checked in `stdlib/` (host test enumerates the 8×7 pairs).
- [ ] Validation commands exit 0.
