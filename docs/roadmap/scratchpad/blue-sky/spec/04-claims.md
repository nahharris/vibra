# 04 Claims

Status: draft specification; not normative

A claim is everything a cell promises. A witness is admitted against it.
This chapter defines what a claim can say, what must be shown, how well it
was shown, and how the claim itself is judged.

## Clauses

| Clause | Says | Applies to |
| --- | --- | --- |
| `takes`, `returns` | The types of parameters and result | `fn`, `proc`, `lemma`, `op` |
| `requires p` | Callers must establish `p` | `fn`, `proc`, `lemma` |
| `ensures p` | `p` holds of the result. The result is named in `returns` | `fn`, `proc`, `lemma` |
| `means e` | The result equals `e` | `fn` |
| `example call = value` | One concrete input and its result | `fn` |
| `decreases e` | The measure that makes recursion end | `fn`, `lemma` |
| `cost b` | An upper bound on steps | `fn`, handlers |
| `releases l` | The function may remove label `l` | `fn` |
| `uses p, …` | The ports that may be addressed | `proc`, `machine` |
| `invariant p` | `p` holds of the state after every step | `machine` |
| `law` | A property every implementation satisfies | `interface` |

Claim clauses are leaves of type `bool` over the parameters and the named
result. They may call any function and may use `forall` and `exists`.
Inside a claim a called function stands for its own claim: its `means` if
it has one, otherwise its `ensures`.

## Meaning

A function's meaning is what its result must be. There are two ways to
state it, and a claim may use both.

**By reference.** `means` gives the clearest definition anyone can write.
It may be slow. It is total, pure, and executable.

```text
fn fib-fast
  takes n: int
  returns value: int
  requires (int.ge n 0)
  means (fib n)
```

**By property.** `ensures` clauses constrain a named result.

```text
fn split-cents
  takes total: cents, parts: part-count
  returns shares: (list cents)
  ensures (int.eq (list.length shares) parts.value)
  ensures (int.eq (cents.sum shares) total.value)
  ensures (int.le (cents.spread shares) 1)
  ensures (list.descending shares)
  example (split-cents (cents.of 100) (part-count.of 3)) =
      [(cents.of 34), (cents.of 33), (cents.of 33)]
  cost (bound.linear parts.value)
```

- A function with `means` and no `body` is its own witness.
- A function with `means` and a `body` generates the obligation that the
  body's result equals the meaning for every input that satisfies
  `requires`.
- An exported function MUST state a meaning and at least one example.
- `returns any r: t` declares that several results are acceptable. Without
  `any`, a claim that admits more than one result is rejected
  ([below](#strength-checks)).

Performance work is the replacement of one witness by another under an
unchanged claim. It cannot change behavior, because the claim is the
behavior.

## Obligations

Checking a cell generates **obligations**. Each is a closed statement with
a stable identifier, the cell and position it came from, and the facts in
scope there.

| Source | Obligation |
| --- | --- |
| A call | The callee's `requires` hold |
| `(t.of e)` | The `where` clauses of `t` hold of `e` |
| `(array.at x i)`, `(int.div a b)`, fixed-width arithmetic, `T.from` | The operation is defined |
| `return e` | Every `ensures` holds, and `e` equals `means` |
| A recursive call | The measure decreases and is not negative |
| `assert p` | `p` holds |
| `impossible` | The point is unreachable |
| `cost b` | The inferred bound is within `b` |
| A machine handler | Each `invariant` is preserved. The initial state satisfies it |
| `impl` | Each law of the interface holds |
| An `example` | The witness and the meaning both give the stated value |

Type checking, exhaustiveness, ownership, labels, and `uses` are decided
by the kernel directly and are not obligations.

## Evidence

An evidence record states that one obligation of one cell was discharged
by one method.

```text
(evidence
  (cell 9f2c…e1 41aa…07)
  (obligation ensures-2)
  (grade solved)
  (method solver "z3 4.13" (seconds 0.4))
  (inputs 77b0…c2))
```

| Method | What it does | Best grade |
| --- | --- | --- |
| Kernel | Decides the obligation by computation or by checking a proof certificate | `proved` |
| Solver | Sends a verification condition to a solver that reports it valid | `solved` |
| Enumeration | Checks every input up to a stated size bound | `bounded` |
| Exploration | Checks every reachable machine state up to a stated depth | `bounded` |
| Generation | Checks a stated number of generated inputs, seeded | `tested` |
| Simulation | Checks a stated number of seeded runs with faults | `tested` |
| Replay | Checks recorded journals | `tested` |

- A checker SHOULD search for a counterexample before attempting a proof.
  Testing is cheap and a counterexample is the most useful result.
- A failed obligation MUST be reported with a concrete counterexample when
  one exists, as Vibra values.
- A solver timeout is not a failure. The obligation stays open and may be
  discharged by another method at a lower grade.
- Evidence MUST be reproducible from its record: tool, version, seed, and
  bounds.

## Grades

| Grade | Meaning |
| --- | --- |
| `proved` | Decided by the kernel or by a certificate the kernel checked |
| `solved` | Reported valid by a named solver, without a checked certificate |
| `bounded` | Holds for every case up to a stated bound |
| `tested` | Holds for the stated sample |
| `assumed` | Not checked. Always accompanied by a debt |

- Grades are ordered as listed, highest first.
- A tool MUST NOT report a grade higher than the weakest evidence it
  rests on, including the evidence of every claim used
  ([01](01-programs.md#effective-grades)).
- An obligation with a known counterexample has no grade and blocks
  admission.
- Which grade is enough is not a property of the language. It is set by
  store policy for admission and by target policy for shipping.

## Strength checks

A witness verified against a weak claim is worth little. The store
measures each claim before admitting it.

| Check | Question | Reported on failure |
| --- | --- | --- |
| Satisfiable | Does some input satisfy `requires`? | "No input satisfies the preconditions" |
| Vacuity | Does the claim reject some result for some valid input? | "Every result satisfies this claim" |
| Determinacy | Can two different results satisfy the claim for one input? | The input and both results |
| Examples | Does each example follow from the meaning? | The example and the value the meaning gives |
| Mutation | Does the claim reject altered witnesses? | A surviving mutant and an input where it differs |

Rules:

- A claim with `means` is determinate by construction.
- Determinacy is not checked for `returns any`. The other checks still
  apply.
- Mutants are produced by the checker from the witness: a changed
  constant, a swapped operator or argument, a removed branch, a changed
  boundary. A mutant that provably equals the witness on all inputs is
  discarded. The share of remaining mutants that the claim rejects MUST
  meet the store's threshold.
- Strength checks have grades like any obligation. A determinacy result
  at grade `bounded` says no second result exists up to the bound.

When a check fails because intent is unclear, the tool SHOULD ask with
concrete cases, not prose: "for `(split-cents 1 3)`, is the result
`(1 0 0)` or `(0 0 1)`?"

## Cost

`cost b` bounds the number of **steps** a function takes, as a function of
its inputs.

- One step is one application, construction, field read, or comparison.
  Standard collection operations state their own costs in their claims.
- An update of an owned collection costs a constant. An update of a
  shared collection costs its length ([03](03-types.md#ownership)).
- `b` is a leaf of type `bound`, built from `bound.constant`,
  `(bound.log n)`, `(bound.linear n)`, `(bound.linear-log n)`,
  `(bound.poly n k)`, `(bound.plus a b)`, and `(bound.times a b)`, where
  `n` is an `int` leaf over the parameters.
- The checker infers a bound for the witness and MUST show it is within a
  constant factor of `b`. When inference fails, the obligation may be
  discharged at grade `tested` by measuring steps on generated inputs of
  growing size.
- Every machine handler MUST have a cost bound, stated or inferred. The
  scheduler relies on it ([06](06-runtime.md#scheduling)).

Open: a `space` clause for peak memory, by the same method.

## Lemmas and laws

A **lemma** is a statement with a proof. Its claim is `takes`, `requires`,
and `ensures`. Its witness is a body that the checker verifies.

```text
lemma fib-grows
  takes n: int
  requires (int.ge n 1)
  ensures (int.le (fib n) (fib (int.add n 1)))
  decreases n
body
  if (int.ge n 2)
    apply (fib-grows (int.sub n 1))
  else
    pass
end fib-grows
```

- `apply (l a …)` adds the `ensures` of lemma `l` to the facts in scope,
  after establishing its `requires`. Induction is a lemma applying itself
  under a decreasing measure.
- A lemma body is never executed. It contains `let`, `if`, `match`,
  `assert`, `apply`, and `pass`.
- A **law** is a lemma stated in an interface. Each `impl` proves it.
- There is no tactic language. A proof is a body with assertions, and a
  solver fills the steps between them.

## Debts

A **debt** records that something was not shown.

| Kind | Created by | Records |
| --- | --- | --- |
| `assume` | An `assume p` statement | The assumed fact |
| `diverge` | A recursion admitted without a measure | The cell |
| `extern` | A witness of the form `body extern "symbol"` | The symbol and the claim it is trusted to meet |
| `release` | A `releases l` clause | The label and the function |
| `open` | An obligation left without evidence at admission | The obligation |
| `weak` | A strength check below threshold, accepted knowingly | The check and its result |

- A debt has a kind, the cell and position, a reason in text, and the
  principal that accepted it.
- Only an intent authority may accept a debt. An implementer's
  transaction that needs a new debt is staged and not published until
  one accepts it.
- A debt is inherited by every dependent and MUST appear in the effective
  grade and in every query about any of them.
- An `extern` witness can earn at most grade `tested` for its `ensures`,
  by running it against its meaning.
- A target policy MAY forbid any kind of debt
  ([06](06-runtime.md#targets-and-policy)).

There is no other escape. No annotation silences a check.
