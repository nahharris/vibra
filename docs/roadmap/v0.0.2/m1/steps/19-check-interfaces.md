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
  whole (Implementations, Substitution, Associated types, Dispatch,
  Interface values, Overlap), "Bounds", "Widening", "Conversion", "Dict keys,
  equality, and order"; [runtime chapter](../../../../spec/06-runtime.md)
  "Generic instantiation", "Integer operations" (the interface table);
  [source chapter](../../../../spec/01-source-language.md) "`interface`",
  "`impl`".

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-types-v1/src/interfaces.rs` | completeness, overlap by unification, destination dispatch: the algorithms are re-expressed |
| `crates/vibra-types/src/{declare,expr,lower}.rs` | extend |
| `stdlib/src/std/{core,builtin,bool}.vib` | implementations land here |

Proposed: `vibra-types::{interfaces, dispatch, widen}`.

## Ordered tasks

1. Interface declarations: abstract members (no body), default members
   (an empty body is `@type.missingReturn`), associated types as members
   of the interface, `given` bounds on header parameters only, `Self`.
   The receiver slot of a member is its first positional parameter whose
   written type is exactly `Self`. In a written result type `Self` may
   stand only in a position of the `Self` map (the result itself, a
   component of a tuple or record, an argument of `Option`, `Result`, or
   `List`); anywhere else it is `@type.unsupportedSelfPosition` at the
   result type, checked on the declaration whether or not the interface
   is used as a value. A member with no receiver slot whose written
   result does not contain `Self` is `@type.undispatchableContractMember`
   at its declaration.
2. Implementations: in the `type` (target an interface,
   `@name.wrongEntityKind` otherwise) or in the `interface` (target a
   declared or builtin concrete type; an anonymous type, interface, or
   `Never` is `@name.wrongEntityKind`; a same-module type is
   `@type.redundantImplementation`). The key of an implementation is the
   interface, the receiver, and the positional arguments; a label in an
   `impl` target is `@type.typeArgumentMismatch` at the label. The
   conformance diagnostics are the chapter's table: an abstract member or
   an associated type not supplied is `@type.missingAbstractMember` at
   the `impl` head, related to the contract member; a member or choice
   whose name is no member of the interface is `@name.unknownSymbol` at
   the name; a member supplied twice is `@name.memberCollision` at the
   later name; an associated type chosen twice is `@name.redeclaration`
   at the later choice; a default member supplied again is
   `@type.defaultOverride` at the member's name; a signature that differs
   from the contract under the substitution is `@type.mismatch` at the
   differing type. An `impl` block declares no generics. Two keys of one
   interface whose receivers and positional arguments unify overlap: the
   later is `@type.overlappingImplementation`, related to the earlier.
3. Substitution and dispatch, in the five steps of the chapter: the
   receiver constraint (the receiver slot, or the positions of the `Self`
   map in the written expected type; no expected type is
   `@type.ambiguousDestination`; positions that give no receiver or
   different receivers are `@type.mismatch` at the application); the
   positional constraints, with a deferred equality for every written
   associated type; bound evidence when the receiver is a type parameter
   (`@type.unsatisfiedBound` when the interface or the positional
   constraints are not those of the bound; an associated type the bound
   leaves unfixed equals only itself); selection by key (no
   implementation is `@type.unsatisfiedBound` at the receiver operand or
   the expected type, related to the interface; several, which needs an
   undetermined positional argument, are `@type.ambiguousImplementation`
   at the application); checking under the substitution (an argument of
   another type in a `Self` parameter outside the receiver slot is
   `@type.argumentMismatch` at that argument; a deferred equality or a
   result that disagrees is `@type.mismatch`). Member-local type
   parameters are inferred in the last step. A contract member named as a
   value is instantiated from its written expected callable type.
4. Associated types have two names and no projection path: the bare name
   inside the declaring interface and inside an `impl` of it, and a label
   in a type application, `Iter(Item: U8)`. A label that names no
   associated type or is repeated is `@type.typeArgumentMismatch` at the
   label. A choice that mentions an associated type of its own `impl` is
   `@name.unknownSymbol`; an `impl` that brings an associated name into a
   scope where a type parameter has that name is
   `@name.genericRedeclaration` at the `impl` head.
5. Interface values: an interface in type position; a generic interface
   is written applied with every associated type fixed by label, and one
   left open is `@type.typeArgumentMismatch` at the type. A member call
   through a value is checked against the public view, unboxes the held
   value, runs under the stored substitution, and reboxes every `Self`
   position of the result. A member with no receiver slot cannot be
   called through a value; an argument for a second parameter whose type
   is or contains `Self` is `@type.argumentMismatch` at that argument. An
   interface value implements nothing, satisfies only `Any`, and never
   widens again.
6. Widening: concrete to interface and atom singleton to `Atom`, only at
   the five written expected types, never through a container or on an
   interface value; a value that needs widening elsewhere is
   `@type.mismatch`.
7. Bounds: a bound is an interface written as a type expression: bare,
   applied positionally (`From(I16)`), or with labels (`Iter(Item: U8)`);
   a label or positional argument may name another parameter of the same
   header (`given {T: Ordered I: Iter(Item: T)}`). Bounds are resolved
   after ordinary inference: a resolvable bound selects one
   implementation by key, whose choices determine the parameters its
   labels name; a parameter still undetermined when no bound is
   resolvable is `@type.ambiguousInference` at the application; a
   resolvable bound with no implementation is `@type.unsatisfiedBound`;
   two sources of one parameter that disagree are `@type.mismatch` at the
   operand.
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
| `V2-TYPE-INTERFACE-missing-member` | negative | an `impl Printable` without `render` | `@type.missingAbstractMember` at the `impl` head, related to `render` |
| `V2-TYPE-INTERFACE-unknown-member` | negative | an `impl Printable` with a second member `show` | `@name.unknownSymbol` at `show` |
| `V2-TYPE-INTERFACE-member-twice` | negative | an `impl Printable` with `render` twice | `@name.memberCollision` at the later `render` |
| `V2-TYPE-INTERFACE-default-override` | negative | an `impl Ordered` that defines `less` | `@type.defaultOverride` at `less` |
| `V2-TYPE-INTERFACE-signature-mismatch` | negative | `render` returning `U64` | `@type.mismatch` at `U64`, related to the contract member |
| `V2-TYPE-INTERFACE-default-empty-body` | negative | `interface I fn f takes {v: Self} returns r: Str do endfn endinterface` | `@type.missingReturn` at the body |
| `V2-TYPE-INTERFACE-self-position` | negative | `interface Index fn all takes {v: Self} returns r: Dict(Str Self) endfn endinterface` | `@type.unsupportedSelfPosition` at the result type |
| `V2-TYPE-INTERFACE-self-map` | positive | `interface Split fn split takes {v: Self} returns r: Option({Self List(Self)}) endfn endinterface` | accepted: every `Self` is in a position of the `Self` map |
| `V2-TYPE-INTERFACE-impl-target-label` | negative | `type Once of Option(U8) impl Iter(Item: U8) … endimpl endtype` | `@type.typeArgumentMismatch` at the label `Item` |
| `V2-TYPE-INTERFACE-default-member` | positive, interpret | `Ordered.less(1i32 2i32)` | `true` |
| `V2-TYPE-INTERFACE-receiver-dispatch` | positive, interpret | two types implementing `Printable`; `render` called on each | each body |
| `V2-TYPE-INTERFACE-destination-dispatch` | positive, interpret | `let v: Celsius be From.convert(21i32) endlet` with `impl From(I32)` in `Celsius` | the converted value |
| `V2-TYPE-INTERFACE-ambiguous-destination` | negative | `let v be From.convert(21i32) endlet` | `@type.ambiguousDestination` at the application |
| `V2-TYPE-INTERFACE-destination-shape` | negative | `interface Twin fn twin takes {n: U8} returns r: {Self Self} endfn endinterface`; `let t: {U8 U16} be Twin.twin(1u8) endlet` | `@type.mismatch` at the application: the two `Self` positions give different types |
| `V2-TYPE-INTERFACE-no-implementation` | negative | `Printable.render(1i32)` with no `impl` for `I32` | `@type.unsatisfiedBound` at `1i32`, related to `Printable` |
| `V2-TYPE-INTERFACE-ambiguous-implementation` | negative | `Celsius` implementing `From(I16)` and `From(I8)`; `let v: Celsius be From.convert(1) endlet` | `@type.ambiguousImplementation` at the application: the unsuffixed operand leaves the positional argument undetermined |
| `V2-TYPE-INTERFACE-self-parameter-mismatch` | negative | `Ordered.less(1i32 "x")` | `@type.argumentMismatch` at `"x"` |
| `V2-TYPE-INTERFACE-undispatchable` | negative | `interface Maker fn make returns r: I32 endfn endinterface` | `@type.undispatchableContractMember` at `make` |
| `V2-TYPE-INTERFACE-member-as-value` | positive | `let f: Fn({Celsius} Str) be Printable.render endlet` | accepted |
| `V2-TYPE-INTERFACE-member-as-value-ambiguous` | negative | `let f be Printable.render endlet` | `@type.ambiguousInference` |
| `V2-TYPE-INTERFACE-generic-twice` | positive, interpret | `Celsius` implementing `From(I16)` and `From(I8)` | both convert |
| `V2-TYPE-INTERFACE-overlap` | negative | `impl From(T)` and `impl From(I8)` on one receiver | `@type.overlappingImplementation` |
| `V2-TYPE-INTERFACE-assoc-once` | positive, interpret | the `Once` iterator example; `Iter.next(Once(Option.some(value: 1u8)))` | `Option.some(value: {1u8 Once(Option.none())})` |
| `V2-TYPE-INTERFACE-assoc-missing` | negative | `impl Iter` without `type Item of …` | `@type.missingAbstractMember` at the `impl` head |
| `V2-TYPE-INTERFACE-assoc-twice` | negative | `impl Iter` with `type Item of U8 endtype` twice | `@name.redeclaration` at the later choice |
| `V2-TYPE-INTERFACE-assoc-self-reference` | negative | `impl Iter type Item of Option(Item) endtype … endimpl` | `@name.unknownSymbol` at the inner `Item` |
| `V2-TYPE-INTERFACE-assoc-bound-parameter` | positive, interpret | `fn first(T I) takes {it: I} returns r: Option(T) given {I: Iter(Item: T)} do match Iter.next(it) case Option.some(value: {item _}) return Option.some(value: item) case Option.none() return Option.none() endmatch endfn` called with `Once(Option.some(value: 1u8))` | `Option.some(value: 1u8)`: `T` is taken from the choice of the selected implementation |
| `V2-TYPE-INTERFACE-assoc-abstract` | negative | `fn head(I) takes {it: I} returns r: Option(U8) given {I: Iter} do …` returning the item of `Iter.next(it)` | `@type.mismatch` at the returned item: an associated type the bound leaves unfixed equals only itself |
| `V2-TYPE-INTERFACE-assoc-label` | positive | `given {I: Iter(Item: U8)}` and a call with `Once` | accepted |
| `V2-TYPE-INTERFACE-assoc-label-unknown` | negative | `Iter(Thing: U8)` | `@type.typeArgumentMismatch` at the label `Thing` |
| `V2-TYPE-INTERFACE-assoc-label-repeated` | negative | `Iter(Item: U8 Item: U8)` | `@type.typeArgumentMismatch` at the second label |
| `V2-TYPE-INTERFACE-value-widen` | positive, interpret | `let p: Printable be User(…) endlet return Printable.render(p)` | `"Ada"` |
| `V2-TYPE-INTERFACE-value-open-assoc` | negative | `let i: Iter be once endlet` | `@type.typeArgumentMismatch` at the type `Iter` |
| `V2-TYPE-INTERFACE-value-self-result` | positive, interpret | `let i: Iter(Item: U8) be Once(…) endlet Iter.next(i)` | a value of type `Option({U8 Iter(Item: U8)})`: the remainder is reboxed and encodes as `{kind: @interface held: …}` |
| `V2-TYPE-INTERFACE-value-self-input` | negative | `Equatable.equal(e e)` with `e: Equatable` | `@type.argumentMismatch` at the second argument |
| `V2-TYPE-INTERFACE-value-no-widen-again` | negative | `let a: Any be p endlet` with `p: Printable` | `@type.mismatch` |
| `V2-TYPE-INTERFACE-widen-container` | negative | `let l: List(Printable) be [User(…)] endlet` | `@type.mismatch` at the element |
| `V2-TYPE-INTERFACE-widen-inferred-not-written` | negative | `let x be if b then user else celsius endif endlet` | `@type.mismatch` (no common supertype) |
| `V2-TYPE-INTERFACE-widen-atom` | positive, interpret | `fn f takes {a: Atom} returns r: Atom do return a endfn` called with `@ok` | `@ok` |
| `V2-TYPE-INTERFACE-any` | positive | `fn pass takes {a: Any} returns r: Any do return a endfn` called with `1i32` | accepted |
| `V2-TYPE-GENERIC-bound-satisfied` | positive, interpret | `fn show(T) takes {v: T} returns r: Str given {T: Printable} do return Printable.render(v) endfn` with `User` | `"Ada"` |
| `V2-TYPE-GENERIC-bound-unsatisfied` | negative | `show(1i32)` | `@type.unsatisfiedBound` at `1i32`, related to the bound |
| `V2-TYPE-GENERIC-bound-positional` | positive, interpret | `fn widen(T) takes {n: I16} returns r: T given {T: From(I16)} do return From.convert(n) endfn`; `let c: Celsius be widen(2i16) endlet` | the converted value |
| `V2-TYPE-GENERIC-bound-linked` | positive, interpret | `fn largest(T I) takes {items: I} returns r: Option(T) given {T: Ordered I: Iter(Item: T)} …` called with `[3i32 1i32 2i32]` | `Option.some(value: 3i32)` |
| `V2-TYPE-GENERIC-bound-linked-conflict` | negative | the same called as `let r: Option(Str) be largest([3i32 1i32]) endlet` | `@type.mismatch` at the operand |
| `V2-TYPE-GENERIC-bound-undetermined` | negative | `fn make(T S) returns r: T given {T: From(S)} …` called as `let c: Celsius be make() endlet` | `@type.ambiguousInference` at the application: no bound is resolvable and `S` is undetermined |
| `V2-TYPE-GENERIC-bound-not-interface` | negative | `given {T: Fn({I32} I32)}` | `@name.wrongEntityKind` at the bound |
| `V2-TYPE-CONVERT-from-integer` | positive, interpret | `let w: I32 be From.convert(1i8) endlet` | `1i32` |
| `V2-TYPE-CONVERT-tryfrom-integer` | positive, interpret | `let n: Result(I8 ConversionError) be TryFrom.convert(300i32) endlet` | `Result.err(error: ConversionError.outOfRange())` |
| `V2-TYPE-CONVERT-no-from-narrowing` | negative | `let n: I8 be From.convert(1i32) endlet` | `@type.unsatisfiedBound` at the expected type `I8`, related to `From`: no implementation has the key `From`, `I8`, `I32` |
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
`@type.invalidDictKey`, `@type.unsupportedSelfPosition`,
`@type.argumentMismatch`, `@type.missingReturn`, `@name.unknownSymbol`,
`@name.memberCollision`, `@name.redeclaration`,
`@name.genericRedeclaration`. Schema: `@types.v2` `implementations` is
filled with implementation records.

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

- [ ] `conformance/rules.tsv` has one `active` row per case id above.
- [ ] Each row of the implementation diagnostics table and each of the five
      dispatch steps of the type chapter has a case named in the PR.
- [ ] Every integer type has `Equatable`, `Ordered`, and its `From`/`TryFrom`
      rows checked in `stdlib/` (host test enumerates the 8×7 pairs).
- [ ] Validation commands exit 0.
