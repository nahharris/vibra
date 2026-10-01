# Step 10 — freeze Stage 3B contracts

Prerequisite: Step 9 merged. Specification prerequisite: it claims no
behavior.

## Scope

Close the Stage 3B gaps and ledger rows deferred from Step 1, and write the
guides for Steps 11–16.

- **G7 / X1 → D18.1.** The `iter` default-member table in the type chapter
  lists parameters, result, and member generics in separate columns. `map`
  declares its own `where: (out any)` and returns `(iter out)`, so it can
  change the element type, and `mapped-iter` is generic in `item` and `out`.
- **G10 / X2 and the Step 10 half of G2 → D18.2.** A map is ordered by its
  keys, so `compare` alone decides where a key goes and whether two keys are
  one. Map keys therefore require only `ordered`. `@std.core` declares
  `equatable` and `ordered`, with an explicit total-order and agreement rule,
  and v1 declares no `hashable`. A generic key type needs one bound,
  `ordered`, which also resolves the one-bound-per-parameter conflict.
- **G9 / X3 → D18.3.** The tooling chapter defines the `@index.v1` projection
  of `@workspace`. It has three kinds of records:
  - declaration records carrying identity, kind, module, owner, visibility,
    source span, checked signature, effects, error types, applications, and
    formatter-normalized text;
  - implementation records keyed by receiver and applied interface, whose
    members are keyed by the contract, target, and receiver triple;
  - reference records.

  All are canonical VIBON in fixed sort orders, byte-identical for an
  identical snapshot.

## Delivery notes

- These are recommendations adopted without a separate review round, under
  the standing instruction to continue autonomously; each is a single ledger
  row and a single spec paragraph, so it can be revisited before its
  implementing step.
- The guides are [11](11-interfaces.md), [12](12-interface-values.md),
  [13](13-conversion.md), [14](14a-iter-contract.md), [15](15a-index-records.md), and
  [16](16-exit.md).
