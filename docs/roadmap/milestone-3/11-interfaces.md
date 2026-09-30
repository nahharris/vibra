# Step 11 — interfaces, `impl`, and the library map

Prerequisite: Step 10 merged. Stage 3B behavior step.

## Read before editing

- [Types](../../spec/02-type-system.md): **Interfaces and methods**,
  **Overlap and non-unifiability**, **Generics** (interface bounds),
  **Nominal declarations** (map keys and the key contracts).
- [Source](../../spec/01-source-language.md): the `defint` and `impl` grammar.
- [Runtime](../../spec/06-runtime.md): **Evaluation** (static dispatch).
- [Decision ledger](decision-ledger.md) rows D17.1, D18.1–D18.3.

## Scope

1. **Interfaces.** `defint` with abstract and default contract members,
   generic interfaces keyed by applied type, and `any` as the predeclared empty
   interface in bound position (type position is Step 12).
2. **Implementations.** Nested `impl` blocks in both placements (inside the
   `deftype`, targeting an interface; inside the `defint`, targeting a
   concrete type), with the entity-kind rule for each target slot, same-module
   `defint` targets rejected with `@type.redundant-implementation`,
   completeness per block identity (`@type.missing-abstract-member`, extra
   members), default-override rejection (`@type.default-override`), pairwise
   non-unifiable applied targets (`@type.overlapping-implementation`), and
   anonymous targets rejected.
3. **Static dispatch.** A contract member called through its interface path,
   such as `(ordered.compare a b)`, selects the implementation from the
   receiver's static type. The member's dispatch class is fixed at
   declaration: receiver-dispatched, destination-dispatched (checked here,
   called in Step 13), or `@type.undispatchable-contract-member`.
4. **Interface-bounded generics.** `where: (k ordered)`: a bounded parameter's
   contract members are callable on its values, and an instantiation must
   satisfy the bound.
5. **Key contracts and the library map** (D17.1, D18.2). `@std.core` declares
   `equatable` and `ordered`. The closed key conformances become ordinary
   implementations in the standard library. `map` moves into the standard
   library as a `deftype` over a sorted array of entries that claims `@map`,
   with `map.of` and lookup as Vibra with native implementations. A user
   `deftype` key needs its own `ordered` implementation.

## Test matrix

- Positive: an interface with abstract and default members implemented in each
  placement; a generic interface implemented at two non-overlapping targets; a
  bounded generic calling a contract member; a user record keyed in a map
  through its own `ordered`; `map.of` and lookup matching their natives.
- Negative: a missing abstract member, an extra member, and a default override;
  a same-module `defint` target; a `deftype` target naming a type and a
  `defint` target naming an interface (`@name.wrong-entity-kind`); an
  anonymous target; overlapping targets `(from t)` and `(from i32)`; an
  undispatchable member; a bound the instantiation does not satisfy; a user
  key without `ordered`.

## Done

Inventory rows `Declaration::Defint`, `TypeMember::Implementation`, and the
interface-bound clause of `Attribute::Where` reference cases, and the body/native
harness covers the map rows. Validation passes.
