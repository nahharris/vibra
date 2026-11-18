# v0.0.2 M1: pure language

**Planned.** The pure part of the language, checked and run on the reference
interpreter.

## Scope, in slices

1. **Reader and formatter.** Lexical structure, the full grammar, recovery,
   the lossless tree, and the canonical format, with golden trees for every
   production and an idempotence corpus.
2. **Names and modules.** Imports with derived names, the flat module scope,
   paths against field reads, acyclic imports, case rules for module paths.
3. **Types and construction.** Scalars, structural tuples and records,
   declared wrappers, tuples, records, and variant types, representation
   visibility, generics with `given`, recursive types with the finite-size
   check.
4. **Bodies.** `let` with patterns, optional type, and `else`; `if`,
   `match`, and `do` as statements and expressions; explicit `return`;
   `try` with its expected type; written evaluation order; coverage.
5. **Callables and interfaces.** Named and anonymous `fn`, labelled
   defaults, interfaces, nested `impl`, dispatch, interface values, the two
   widenings.
6. **Standard library.** The pure modules: `Iter` with `fold` and the
   adapters, `Equatable`, `Ordered` with its defaults, `From` and `TryFrom`,
   saturating and checked arithmetic with no trapping operation, nested
   `Iter` implementations for the builtin collections.
7. **Guards.** The certified fragment; bare `requires`, `ensures`, and
   `where` discharged by the seven rules or rejected; the `else` forms;
   fallible construction; conversion of guarded callables.
8. **Module values and tests.** Lazy, acyclic module values; `test`
   declarations without grants; assertions.
9. **Compile-time evaluation.** The interpreter evaluates constant
   predicates and constant expressions for the checker, under tool limits
   that report "not established", never "true".

Each slice lands with its diagnostics, schemas, conformance cases, and
formatter support.

## Exit evidence

- Positive, negative, recovery, and format cases for every form of the
  slices above, in both suites.
- Contract cases: discharged, refuted, and unproved obligations with their
  diagnostics; guard consequence values; a handled constant
  construction error; no run-time check emitted for a bare guard.
- Deep tail and non-tail recursion; invariant checks on every ingress path.
- A list of every gate-program construction the contract rules rejected,
  with the repair used.
- The reference solutions and controls of the model-writing pilot's pure tasks pass.

## Not in this milestone

`proc`, plans, `await`, grants, host operations, and the entry contract.
