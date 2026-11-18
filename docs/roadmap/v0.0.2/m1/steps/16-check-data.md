# Step 16: check-data

Kind: vertical. Slice 3: structural tuples and records, lists and dicts,
declared wrappers, tuples, records, and variant cases with fields,
representation visibility, generics with `given`, recursive types with the
finite-size check, field reads, lookups, and constructor application with
labels.

## Prerequisites

- Step 15 landed.
- Read: [type chapter](../../../../spec/02-type-system.md) "Declared
  types", "Representation visibility", "Structural types", "Recursive
  types", "Callable types", "Dict keys, equality, and order" (key
  admissibility), "Application" (constructor and lookup rows), "Generics";
  [source chapter](../../../../spec/01-source-language.md) "Application",
  "Anonymous products and collections", "Names and paths" (field reads);
  [runtime chapter](../../../../spec/06-runtime.md) "Evaluation"
  (construction, reads, dicts) and "Values" (the encodings the `interpret`
  cases expect).

## Entry points

| Existing | Use |
| --- | --- |
| `crates/vibra-types/src/{declare,infer,expr,lower}.rs` (step 15) | extend |
| `crates/vibra-types-v1/src/{nominal.rs,construct.rs}` | the finite-size and constructor-binding algorithms, re-expressed |
| `stdlib/src/std/builtin.vib` (step 14) | `Dict(K V)` members and `List` members become checkable |

Proposed: `vibra-types::{nominal, generics}`.

## Ordered tasks

1. Declared types: wrapper (`of T`), declared tuple (`of {T U}`), declared
   record (`of {a: T …}`), variant with fields; a repeated field or case
   name is `@name.memberCollision` (step 10 reports it; this step consumes
   the resolved shape); declaration order fixes discriminants and encoding.
2. Constructors: labels exactly when the shape has field names; missing,
   duplicate, unknown label, wrong arity `@type.argumentMismatch`; result
   type is the declared type (the `Result` of `where … else` is 21).
   Applying a tuple, record, wrapper value, atom, or number is
   `@type.notApplicable` at the callee.
3. Field reads: on a record or declared record; `@type.unknownRecordField`;
   no tuple index.
4. Structural types: tuple arity and component types; record field sets in
   any written order with canonical order by UTF-8 bytes; `{}` the empty
   tuple; mixing `@type.mismatch`.
5. Collections: list literal elements of one type; `[]` and `[:]` need an
   expected type (`@type.ambiguousInference`); dict keys `Ordered`
   (`@type.invalidDictKey` for `F64`, `Void`, `List`, `Option`, a declared
   type without `Ordered`; `@type.functionNotEquatable` for a callable
   key); lookups `items(0u64)`, `table(key)`, `text(0u64)`, `bytes(0u64)`
   with exactly one positional argument returning `Option`.
6. Representation visibility: `representation @private|@public`, default
   the declaration's visibility; a hidden constructor, pattern, or field
   from another module is `@name.inaccessibleRepresentation`.
7. Generics: `type-params`, `given` bounds, `Any` default, invariant
   arguments, exact argument count (`@type.typeArgumentMismatch`),
   `@type.unsatisfiedBound` (bounds are nominal interfaces; `Any` always
   holds; other bounds are checked when 19 adds implementations, so this
   step's bound cases use `Any`), inference of arguments from operands and
   expected types, no call-site type arguments.
8. Recursive types: symbolic references; `@type.infiniteSize` at the first
   repeating declaration; recursion through `List`, `Dict`, or a callable
   is finite.
9. Interpreter and encoder: compound values, lookups, dict order.
10. Stdlib: `Dict(K V)` members (`entries`), `List` members checked.
11. Cases.

## Test matrix

| Case id | Kind | Input | Expected |
| --- | --- | --- | --- |
| `V2-TYPE-NOMINAL-record-construct` | positive, interpret | `type User of {name: Str id: U64} endtype\nfn main returns r: User do return User(id: 1u64 name: "Ada") endfn` | `{kind: @record typeInfo: {kind: @declared path: @app.main.User seed: … arguments: []} fields: {name: "Ada" id: 1u64}}`, fields in declaration order |
| `V2-TYPE-NOMINAL-record-missing-field` | negative | `User(name: "Ada")` | `@type.argumentMismatch` at the application |
| `V2-TYPE-NOMINAL-record-unknown-field` | negative | `User(name: "Ada" id: 1u64 age: 3u8)` | `@type.argumentMismatch` |
| `V2-TYPE-NOMINAL-record-duplicate-field` | negative | `User(name: "Ada" name: "Bob" id: 1u64)` | `@type.argumentMismatch` |
| `V2-TYPE-NOMINAL-record-positional` | negative | `User("Ada" 1u64)` | `@type.argumentMismatch` |
| `V2-TYPE-NOMINAL-wrapper` | positive, interpret | `type Celsius of F64 endtype … return Celsius(21.5f64)` | `{kind: @wrapper typeInfo: … value: 21.5f64}` |
| `V2-TYPE-NOMINAL-wrapper-no-delegation` | negative | `type Celsius of F64 endtype`; `F64.add(Celsius(1.0f64) 2.0f64)` | `@type.mismatch` at the `Celsius` value |
| `V2-TYPE-NOMINAL-declared-tuple` | positive, interpret | `type Pair of {I32 Str} endtype … Pair(1i32 "x")` | `{kind: @tuple typeInfo: … values: [1i32 "x"]}` |
| `V2-TYPE-NOMINAL-variant-fields` | positive, interpret | `type Shape case circle {radius: F64} case empty endtype … Shape.circle(radius: 1.0f64)` | `{kind: @enum typeInfo: … variant: @circle payload: {radius: 1.0f64}}`; `Shape.empty()` has no `payload` field |
| `V2-TYPE-NOMINAL-variant-fieldless-args` | negative | `Shape.empty(1i32)` | `@type.argumentMismatch` |
| `V2-TYPE-NOMINAL-same-shape-different-type` | negative | `type A of {x: I32} endtype\ntype B of {x: I32} endtype`; a `B` where an `A` is expected | `@type.mismatch` |
| `V2-TYPE-NOMINAL-not-applicable` | negative | `let t be {1i32 2i32} endlet t(0u64)` | `@type.notApplicable` at the callee `t` |
| `V2-TYPE-NOMINAL-field-read` | positive, interpret | `return user.name` with `user: User` | `"Ada"` |
| `V2-TYPE-NOMINAL-unknown-record-field` | negative | `user.age` | `@type.unknownRecordField` at `age` |
| `V2-TYPE-NOMINAL-structural-record-order` | positive, interpret | `fn f takes {r: {a: I32 b: Str}} returns s: Str do return r.b endfn` called with `{b: "x" a: 1i32}` | `"x"` |
| `V2-TYPE-NOMINAL-tuple-arity` | negative | `let t: {I32 I32} be {1i32 2i32 3i32} endlet` | `@type.mismatch` |
| `V2-TYPE-NOMINAL-empty-tuple` | positive, interpret | `fn main returns r: {} do return {} endfn` | `{kind: @tuple values: []}` |
| `V2-TYPE-NOMINAL-list` | positive, interpret | `return [1i32 2i32 3i32]` | list of three |
| `V2-TYPE-NOMINAL-list-mixed` | negative | `[1i32 "x"]` | `@type.mismatch` at `"x"` |
| `V2-TYPE-INFER-empty-list-ambiguous` | negative | `let l be [] endlet` | `@type.ambiguousInference` |
| `V2-TYPE-INFER-empty-dict-expected` | positive, interpret | `let d: Dict(Str I32) be [:] endlet return d` | an empty dict |
| `V2-TYPE-NOMINAL-dict-order-and-duplicate` | positive, interpret | `["b": 1i32 "a": 2i32 "a": 3i32]` | two entries, `"a": 3i32` first |
| `V2-TYPE-NOMINAL-dict-key-float` | negative | `[1.0f64: "x"]` | `@type.invalidDictKey` at the key type |
| `V2-TYPE-NOMINAL-dict-key-function` | negative | `[inc: "x"]` | `@type.functionNotEquatable` |
| `V2-TYPE-NOMINAL-dict-key-tuple` | positive, interpret | `[{1i32 "a"}: true]` | one entry: an anonymous tuple of `Ordered` components is `Ordered` structurally |
| `V2-TYPE-NOMINAL-dict-key-atom` | positive, interpret | `[@b: 1i32 @a: 2i32]` | two entries, `@a` first: atoms are `Ordered` structurally |
| `V2-SRC-CALLS-lookup-list` | positive, interpret | `return [7i32](0u64)` and `return [7i32](1u64)` | `Option.some(value: 7i32)` and `Option.none()` |
| `V2-SRC-CALLS-lookup-dict` | positive, interpret | `["a": 1i32]("a")` | `Option.some(value: 1i32)` |
| `V2-SRC-CALLS-lookup-arity` | negative | `[7i32](0u64 1u64)` | `@type.argumentMismatch` |
| `V2-SRC-CALLS-lookup-key-type` | negative | `[7i32]("x")` | `@type.mismatch` |
| `V2-TYPE-NOMINAL-representation-private` | negative, workspaceCheck | `model.vib`: `type Token of U64 visibility @public representation @private endtype`; `main.vib`: `model.Token(1u64)` | `@name.inaccessibleRepresentation` at the constructor |
| `V2-TYPE-NOMINAL-representation-default` | negative, workspaceCheck | a private type used from another module | `@name.privateAccess` |
| `V2-TYPE-NOMINAL-representation-public` | positive, workspaceCheck | `representation @public` on a public type; constructed from another module | accepted |
| `V2-TYPE-GENERIC-pair` | positive, interpret | `type Box(T) of {item: T} endtype\nfn main returns r: Box(I32) do return Box(item: 1i32) endfn` | a `Box` with argument `I32` |
| `V2-TYPE-GENERIC-argument-count` | negative | `Box(I32 Str)` in a type | `@type.typeArgumentMismatch` |
| `V2-TYPE-GENERIC-invariant` | negative | `fn f takes {b: Box(Any)} …` called with a `Box(I32)` | `@type.mismatch` |
| `V2-TYPE-GENERIC-inferred-argument` | positive, interpret | `fn first(T) takes {items: List(T)} returns head: Option(T) do return items(0u64) endfn` called with `[1i32]` | `Option.some(value: 1i32)` |
| `V2-TYPE-GENERIC-uninferable` | negative | `fn make(T) returns r: Option(T) do return Option.none() endfn` called as `make()` with no expected type | `@type.ambiguousInference` at the application |
| `V2-TYPE-NOMINAL-recursive-list` | positive, interpret | `type Tree case leaf case node {kids: List(Tree)} endtype` | accepted; a value built and returned |
| `V2-TYPE-NOMINAL-infinite-size` | negative | `type A of {next: A} endtype` | `@type.infiniteSize` at `A` |
| `V2-TYPE-NOMINAL-infinite-size-mutual` | negative | `type A of {b: B} endtype\ntype B of {a: A} endtype` | `@type.infiniteSize` at the first in declaration order whose expansion repeats |
| `V2-SRC-CALLS-evaluation-order` | positive, interpret | a record constructed with `{b: trace(2i32) a: trace(1i32)}` where `trace` is pure, so order is shown by a host test on the interpreter | written order |

## Diagnostics and schema changes

Codes: `@type.argumentMismatch`, `@type.mismatch`, `@type.notApplicable`,
`@type.unknownRecordField`, `@type.ambiguousInference`,
`@type.invalidDictKey`, `@type.functionNotEquatable`,
`@name.inaccessibleRepresentation`, `@name.privateAccess`,
`@type.typeArgumentMismatch`, `@type.infiniteSize`. No schema change
(`@types.v2` gains declared types inside its existing fields).

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-interp -p vibra-conformance
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
cargo test --locked --offline --workspace --all-targets --all-features
```

Read: all cases pass; the `@value.v2` snapshots of the declared types carry
the declaration identity of the type chapter in `seed`.

## Excluded

Patterns over products (17); `where` invariants (21); interface bounds
beyond `Any` (19); `Str` and `Bytes` members (20).

## Completion evidence

- [ ] `conformance/rules.tsv` has one `active` row per case id above.
- [ ] `@std.builtin`'s `Dict` and `List` members check clean (host test).
- [ ] Validation commands exit 0.
