# Milestone 3 — complete nominal static core

Execution plan: [M3 steps and implementation guides](execution.md).
Integration branch: `m3`, created from `main` after the M2 merge.

Outcome: authors can build reusable, strongly typed pure libraries.

Deliverables:

- declared and anonymous records, enums, unions, and tuples, declared
  wrapper types, arrays, flat dicts, and atoms;
- a library-first core: the compiler owns the scalar types, `(array t)`, `fn`
  types, and the structural type constructors, while `bool`, `str`, `bytes`,
  `dict`, `option`, `result`, and their operations are standard-library Vibra
  whose `deftype`s claim the closed language roles; single-declaration
  imports; and native implementations that keep their Vibra reference body,
  checked against it by a body/native differential harness;
- closed constructor/projection/lookup application for products and
  collections, including declared-type constructors, the anonymous `tupleof`,
  `recordof`, and `enumof` forms, the `array.of` and `dict.of` static methods,
  and optional bounds/presence lookup;
- flat `where:` generics with `deftype`-inherited names in nested methods,
  closed `impl` targets, complete `types:` call-site argument lists taken from
  the addressed entity, the predeclared empty interface `any` and interface
  values in type position, interface default methods that cannot be overridden,
  `fn` types, `lambda`, `impl` blocks nested in the owning `deftype` or
  `defint`, static dispatch, and ownership/conflict rules;
- unqualified declaration names in every nesting position, path-based method
  and contract references at use sites, one flat addressable-member namespace
  per owner, same-module `impl` rejection, and type-keyed implementation
  identity spelling an `impl` block as applied-target/receiver-type and a member
  as contract-member/applied-target/receiver-type;
- exhaustive flat-arm `match`, `option`, `result`, `try`, and unhandled-failure
  checks;
- declared and anonymous unions over a closed, pairwise non-unifiable member
  set, with no lifted members and no flattening;
- structural `tuple`, `record`, `enum`, and `union` type expressions in every
  type position, with order-insensitive identity and a canonical formatter
  order, wrapper types for any other `deftype` body, `intrinsic-type`
  declarations for the builtin types and their static methods, and a
  `type-name` applied-type head excluding every reserved type head;
- bound-agnostic unification behind every overlap rule, so overlap is decided at
  the declaration and never changes with a distant `impl`;
- all three widening relations — to an interface, to a union, and from an atom
  singleton to `atom` — fired only from written expected types, with no
  computed least upper bound, plus `as` ascription and `as` union narrowing in
  `match`;
- destination-dispatched contract members covering conversion and factory
  shapes alike, the `from` and `try-from` interfaces, the `conversion-error`
  standard-library enum, and one receiver implementing a generic interface at
  several pairwise non-unifiable applied targets;
- destructuring patterns in `let`, fixed positional parameters, lambdas, and
  `match`, with shared exhaustiveness-based irrefutability and no `(bind ...)`
  compatibility form;
- the standard `iter` interface with abstract `next`, canonical default methods,
  closed registry conformance for `(array t)`, `(dict k v)`, `str`, and
  `(option t)`, and stdlib adapter types `mapped-iter`, `filtered-iter`,
  `skipped-iter`, and `taken-iter` with explicit `impl (iter item)`;
- function types with closed effect rows, initially empty for the pure slice;
- full resolved symbol/reference/index records;
- a core/text/bytes/collection/option/result standard-library foundation,
  including the checked integer operations such as `i32.add-checked`
  whose nominal `result` contracts are unavailable in M2; and
- written algebraic laws for the standard `equatable` and `ordered`
  interfaces (dict keys need only `ordered`, per M3 ledger D18.2) and the `iter` default methods, stated in their
  documentation and exercised by conformance examples, so that a later
  law-checking track cannot retroactively invalidate existing implementations.

Delivery order: the milestone lands in two stages on `m3`.

1. **Nominal data and typed failure** — records, enums, wrapper types, unions,
   tuples, arrays, dicts, atoms, their constructors, projections, lookups, and
   variadic operands; destructuring and constructor patterns; exhaustive
   `match`; `option`, `result`, `try`, and unhandled-failure checks; union
   widening, `as` ascription, and `as` narrowing; generics whose every `where:`
   bound is `any`, with applied types, inference, and `types:`, because
   `option`, `result`, lookups, and the collection library are generic; and
   the checked integer and core collection library. Stage demo: a pure module
   parses a small line-oriented format into a nominal record/enum model and
   reports failures through a nominal error union with `try`, with no
   interfaces.
2. **Interfaces, interface-bounded generics, conversion, and iteration** —
   everything else above, including dict operations over a generic key type.
   The stage opens with a specification review of the `iter` default
   `map` signature `(fn (item) item)`, which cannot change the element type
   even though a nested method may declare its own generic names; the review
   either records why that is intended or amends the type chapter to a
   `where: (out any)` result of `(iter out)` before implementation.

Demo gate: implement and test a generic collection-processing library with a
nominal error type, an `iter` implementation, and an interface with a default
method, using no compiler-private escape hatch.

Exit gate:

- the entire type-system chapter has focused positive and negative cases;
- ambiguous inference, illegal shadowing, non-exhaustive matches, invalid
  implementation placement, and silently ignored fallible values produce
  stable atom diagnostics;
- interpreter behavior is independent of dict/hash iteration;
- index and context output use canonical resolved identities; and
- every M2 decision-ledger row and supported-surface entry deferred to M3 is
  implemented or explicitly reassigned to a named later milestone, and no
  M3-owned form still reports `@tool.unavailable`.
