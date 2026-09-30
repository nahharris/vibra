# Step 11 — interfaces and `impl`

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
4. **Interface-bounded generic functions.** `where: (t shape)` on a `defn`: a
   bounded parameter's contract members are callable on its values, and an
   instantiation must satisfy the bound (`@type.unsatisfied-bound`, which also
   reports a contract call whose receiver type has no implementation).

The key contracts, bounds on `deftype` and `lambda` parameters, and the library
`map` are [Step 11b](11b-key-contracts.md).

## Test matrix

- Positive: an interface with abstract and default members implemented in each
  placement; a generic interface implemented at two non-overlapping targets; a
  bounded generic calling a contract member, in one module and across an
  import.
- Negative: a missing abstract member, an extra member, and a default override;
  a same-module `defint` target; a `deftype` target naming a type and a
  `defint` target naming an interface (`@name.wrong-entity-kind`); an
  anonymous target; overlapping targets `(convert t)` and `(convert i32)`; an
  undispatchable member; a bound the instantiation does not satisfy; a contract
  call on a receiver with no implementation; a written member whose signature
  differs from the contract.

## Done

Inventory rows `Declaration::Defint`, `TypeMember::Implementation`, and the
interface-bound clause of `Attribute::Where` reference cases, and validation
passes.
