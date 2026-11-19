# 01 Programs

Status: draft specification; not normative

## The store

A Vibra program lives in a **store**: an append-only, content-addressed
collection of trees. Every object in the store is a canonical tree
([02](02-syntax.md#the-canonical-tree)) identified by the hash of its
canonical encoding.

- The store MUST never change or delete an object that a revision can
  reach.
- Two stores that hold the same hash MUST hold the same bytes.
- Text files are produced from the store and staged into it
  ([08](08-tooling.md#staging)). A text file is never the authority.

## Cells

The unit of a program is a **cell**. Every declaration is a cell.

| Part | Content | Written by | Seen by dependents |
| --- | --- | --- | --- |
| Claim | Signature and everything the cell promises | Intent authority | Yes |
| Witness | An implementation that keeps the promise | Implementer | No |
| Evidence | A record per obligation, with a grade | Kernel and checkers | Its grades only |

Cell kinds and what their two parts are:

| Kind | Claim | Witness |
| --- | --- | --- |
| `type` | Shape, refinement, resource marker | None |
| `label` | A name for restricted data | None |
| `fn` | Parameters, result, `requires`, `ensures`, `means`, examples, cost | `body` |
| `lemma` | A statement over cells | `body`, a proof |
| `interface` | Member signatures and laws | None |
| `impl` | The interface and the type it is for | Member bodies |
| `port` | Operations, signals, outcomes, delivery, model | None in the language; a driver outside it |
| `model` | The port it plays and its state | One handler per operation |
| `proc` | Parameters, result, `uses`, `ensures` over outcomes | `body` |
| `machine` | State, accepted events, `uses`, invariants | `on` handlers |
| `notation` | Grammar and the type it produces | Elaboration and printer |
| `protocol` | Roles and the global exchange | Projections, derived |
| `target` | Bindings and policy | None |

### Opacity

- The checker of a cell MUST use only the claims of the cells it
  references. It MUST NOT read their witnesses.
- A `means` clause is part of the claim. It is visible to dependents and
  may be unfolded by a checker.
- Replacing a witness MUST NOT require re-checking any other cell.

### Check closure

The **check closure** of a cell is the cell plus the claims it references
directly. The store MUST reject a cell whose closure exceeds the store's
budget. The budget is a store setting counted in tree nodes. The draft
default is 2,000 nodes; the right value is an empirical question.

A cell over budget is split. This rule is what keeps every check within a
bounded context.

## Identity

- A **claim id** is the hash of the claim tree after normalization.
- A **witness id** is the hash of the witness tree after normalization.
- A **cell id** is the pair of the two.

Normalization:

1. replaces every reference to another cell by that cell's claim id;
2. replaces a cell's reference to itself by a self marker;
3. removes notes;
4. keeps parameter, field, and case names, because calls and matches use
   them.

Cells that refer to each other in a cycle are hashed as one group.

Consequences that conforming tools MUST preserve:

- A dependent refers to a claim id. Swapping a witness changes no
  dependent's hash.
- Changing a claim creates a new claim id. Existing dependents still refer
  to the old claim until they are staged again
  ([08](08-tooling.md#the-worklist)).
- A cell's name is not part of its identity. Renaming changes a namespace
  and no cell.
- Evidence is keyed by cell id and obligation. The same cell in two stores
  has the same evidence.

## Names and modules

A **namespace** maps qualified names to cell ids. A **revision** is an
immutable namespace plus the store policy, identified by hash. Publishing
creates a new revision.

A **module** is a prefix of the namespace, such as `bank.money`. In the
text projection a module is one file.

```text
module bank.money
exports cents, part-count, split-cents

use std.int
use std.list
use bank.accounts as accounts
```

- `use a.b.c` makes the module available under its last segment. `as`
  gives another alias. There are no wildcard imports and no re-exports.
- A declaration is private to its module unless the `exports` line names
  it.
- A name MUST resolve to exactly one cell. A local binding MUST NOT have
  the name of a visible alias, type, or other local. There is no
  shadowing.
- A function may be declared in the namespace of a type from the same
  module, as in `fn cents.sum`.

A **package** is a namespace root with a manifest, `vibra.vibon`, that
names its dependencies by revision hash. Resolving a package MUST NOT
depend on anything but the manifest and the stores it names.

## Roles

Every change to a store is a transaction made by a principal holding a
role over part of the namespace.

| Role | May | MUST NOT |
| --- | --- | --- |
| Intent authority | Create, change, and sign claims. Accept debts | Bypass strength checks |
| Implementer | Add and replace witnesses. Add private helper cells. Decline a cell | Change or remove a claim it did not create in the same transaction. Accept a debt |
| Checker | Attach evidence | Change claims or witnesses |
| Adversary | Attach counterexamples and surviving mutants | Admit anything |
| Operator | Create targets, bind drivers, run and replay | Change cells |

- The store MUST reject a transaction that exceeds its principal's role.
- One principal MAY hold several roles. A store policy MAY require that
  the intent authority and the implementer of a cell differ.
- An exported claim is **signed** when an intent authority has approved
  its claim id. A target policy MAY require signed claims.

### Declining

An implementer that cannot satisfy a claim MUST be able to say so. A
**decline** is a record attached to a claim id that carries the obligation
that could not be met and the best counterexample or partial evidence.
Declining is always permitted and is never an error of the implementer.

## Admission

A cell is a **draft** while staged and **admitted** once published. The
store MUST admit a cell only if all of the following hold.

1. Its tree is well formed and round-trips through the line surface.
2. It elaborates to the core language and the kernel accepts it
   ([07](07-compiler.md#the-kernel)).
3. It contains no holes.
4. Its check closure is within budget.
5. Every obligation it generates has evidence, or a debt accepted by an
   intent authority ([04](04-claims.md#debts)).
6. Its claim passes the strength checks
   ([04](04-claims.md#strength-checks)).
7. Its claim does not duplicate an existing one: no other admitted cell in
   the revision has a `means` that normalizes to the same tree with the
   same signature.
8. Every cell it references is admitted in the same revision.

A rejected transaction MUST leave the store unchanged and MUST report each
failed rule with the facts needed to fix it
([08](08-tooling.md#diagnostics)).

## Effective grades

Each obligation of a cell has a grade ([04](04-claims.md#grades)). The
**effective grade** of a cell for a property is the lowest grade among its
own obligations for that property and the effective grades of the claims
it references. A debt anywhere below a cell appears in that cell's
effective grade. Tools MUST report effective grades, not local ones.

## Five gates

| Gate | A wrong program is | Where |
| --- | --- | --- |
| 1 | Unsampleable | Holes offer only well-typed moves ([08](08-tooling.md#holes-and-moves)) |
| 2 | Unrepresentable | No form exists for it ([02](02-syntax.md), [03](03-types.md)) |
| 3 | Unacceptable | Admission, above |
| 4 | Unshippable | Target policy ([06](06-runtime.md#targets-and-policy)) |
| 5 | Unhideable | Debts and effective grades |
