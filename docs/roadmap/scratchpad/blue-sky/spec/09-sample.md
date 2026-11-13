# 09 Sample

Status: draft specification; not normative

One small system, written out in full, to show every shape defined in
[02](02-syntax.md). It moves money between accounts. Each block below is
one `.vib` file.

No tool has parsed or checked this code, because none exists yet. It was
checked by hand against the grammar, the depth limit, and the labeling
rule. Standard library names such as `list.build` are assumed, not
specified.

## Pure code: reference and realization

```text
module sample.math
exports fib, fib-fast

use std.bound
use std.int

// The reference definition. Clear, and exponentially slow.
fn fib
  takes n: int
  returns int
  requires (int.ge n 0)
  decreases n
  means
    if (int.lt n 2)
      return n
    let a: int = (fib (int.sub n 1))
    let b: int = (fib (int.sub n 2))
    return (int.add a b)
end fib

// The fast version. Its claim is the reference. Its witness is linear.
fn fib-fast
  takes n: int
  returns int
  requires (int.ge n 0)
  means (fib n)
  example (fib-fast 10) = 55
  cost (bound.linear n)
body
  return (fib-from step: 0 previous: 0 current: 1 limit: n)
end fib-fast

// What a loop with two variables becomes: a recursive helper. Its
// parameters are the loop variables and its requires are the invariants.
fn fib-from
  takes step: int, previous: int, current: int, limit: int
  returns value: int
  requires (int.le 0 step)
  requires (int.le step limit)
  requires (int.eq previous (fib step))
  requires (int.eq current (fib (int.add step 1)))
  ensures (int.eq value (fib limit))
  decreases (int.sub limit step)
body
  if (int.eq step limit)
    return previous
  let after: int = (int.add step 1)
  let next: int = (int.add previous current)
  return (fib-from step: after previous: current current: next limit: limit)
end fib-from

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

Shapes to notice:

- `fib` has `means` and no `body`. `fib-fast` has both.
- All four parameters of `fib-from` are `int`, so every argument is
  labeled. `int.add` is a built-in scalar function and is positional.
- The tail call in `fib-from` uses no stack.

## Types, claims, transforms, an implementation, a label

```text
module bank.money
exports cents, cents.add, cents.sum, holder-name, masked-name, part-count, split-cents

use std.bound
use std.int
use std.list
use std.order
use std.text

// An amount in the smallest unit of the currency. Never negative.
type cents
  wraps int
  where (int.ge self 0)
end cents

type part-count
  wraps int
  where (int.ge self 1)
end part-count

fn cents.add
  takes left: cents, right: cents
  returns cents
  means (cents.of (int.add left.value right.value))
  example (cents.add left: (cents.of 2) right: (cents.of 3)) = (cents.of 5)
end cents.add

fn cents.sum
  takes items: (list cents)
  returns int
  decreases (list.length items)
  means
    if (list.is-empty items)
      return 0
    let first: cents = (list.first items)
    let rest: (list cents) = (list.rest items)
    return (int.add first.value (cents.sum rest))
  example (cents.sum [(cents.of 1), (cents.of 2)]) = 3
end cents.sum

fn spread
  takes items: (list cents)
  returns int
  requires (int.ge (list.length items) 1)
  means
    let values: (list int) = (list.map items cents.value)
    return (int.sub (int.maximum values) (int.minimum values))
end spread

// Split an amount into parts that differ by at most one unit, largest
// first.
fn split-cents
  takes total: cents, parts: part-count
  returns shares: (list cents)
  ensures (int.eq (list.length shares) parts.value)
  ensures (int.eq (cents.sum shares) total.value)
  ensures (int.le (spread shares) 1)
  ensures (list.is-descending (list.map shares cents.value))
  example (split-cents (cents.of 100) (part-count.of 3)) =
      [(cents.of 34), (cents.of 33), (cents.of 33)]
  example (split-cents (cents.of 0) (part-count.of 2)) = [(cents.of 0), (cents.of 0)]
  cost (bound.linear parts.value)
body
  let base: int = (int.div total.value parts.value)
  let extra: int = (int.rem total.value parts.value)
  fn share-at
    takes index: int
    returns cents
    requires (int.ge index 0)
  body
    let bump: int = (if (int.lt index extra) 1 0)
    return (cents.of (int.add base bump))
  end share-at
  return (list.build parts.value share-at)
end split-cents

impl order.ordered for cents
  fn compare
  body
    return (int.compare left.value right.value)
  end compare
end order.ordered

label personal
end personal

type holder-name
  wraps (personal text)
end holder-name

fn masked-name
  takes name: holder-name
  returns text
  means
    let initial: text = (text.take name.value 1)
    return (text.join [initial, "***"])
  example (masked-name (holder-name.of "Ada")) = "A***"
  releases personal
end masked-name
```

Shapes to notice:

- `(list.first items)` is admitted because the `if` above it returned
  when the list was empty. `(int.div total.value parts.value)` is
  admitted because a `part-count` is at least 1.
- `cents.value` without parentheses is the field reader passed as a
  function. `share-at` is a nested function that reads `base` and
  `extra`.
- The first `example` is longer than 100 columns and breaks after `=`.
- `split-cents` states its meaning by properties. Remove the fourth
  `ensures` and the claim is rejected as indeterminate.

## A port and its model

```text
module bank.accounts
exports account-id, credit-outcome, debit-outcome, ledger, ledger-model, refusal

use bank.money
use std.int
use std.map
use std.sim
use std.text
use std.unit

type account-id
  wraps text
  where (int.ge (text.length self) 1)
end account-id

type refusal
  case insufficient-funds
  case account-closed
end refusal

type debit-outcome
  case done
  case refused
    field reason: refusal
end debit-outcome

type credit-outcome
  case done
  case closed
end credit-outcome

port ledger
  op debit
    takes account: account-id, amount: money.cents
    returns debit-outcome
    delivery once
  end debit
  op credit
    takes account: account-id, amount: money.cents
    returns credit-outcome
    delivery once
  end credit
  op refund
    takes account: account-id, amount: money.cents
    returns unit
    delivery repeatable
  end refund
  model ledger-model
end ledger

type book
  field balances: (map account-id money.cents)
end book

model ledger-model
  for ledger
  state book
  starts (book {})
  on debit
    let held: (option money.cents) = (map.get state.balances request.account)
    match held
      case none
        let closed: debit-outcome = (debit-outcome.refused (refusal.account-closed))
        return [(sim.answer next: state outcome: closed)]
      case some
        let balance: int = held.value.value
        if (int.lt balance request.amount.value)
          let short: debit-outcome = (debit-outcome.refused (refusal.insufficient-funds))
          return [(sim.answer next: state outcome: short)]
        let left: money.cents = (money.cents.of (int.sub balance request.amount.value))
        let after: book = (book (map.put state.balances request.account left))
        return [(sim.answer next: after outcome: (debit-outcome.done))]
  on credit
    let held: (option money.cents) = (map.get state.balances request.account)
    match held
      case none
        return [(sim.answer next: state outcome: (credit-outcome.closed))]
      case some
        let raised: money.cents = (money.cents.add left: held.value right: request.amount)
        let after: book = (book (map.put state.balances request.account raised))
        let paid: (sim.answer book credit-outcome) =
            (sim.answer next: after outcome: (credit-outcome.done))
        let shut: (sim.answer book credit-outcome) =
            (sim.answer next: state outcome: (credit-outcome.closed))
        return [paid, shut]
  on refund
    let held: (option money.cents) = (map.get state.balances request.account)
    match held
      case none
        return [(sim.answer next: state outcome: (unit.value))]
      case some
        let raised: money.cents = (money.cents.add left: held.value right: request.amount)
        let after: book = (book (map.put state.balances request.account raised))
        return [(sim.answer next: after outcome: (unit.value))]
end ledger-model
```

Shapes to notice:

- `{}` is an empty map because `book` has one field of map type. A
  constructor with one field takes it unlabeled.
- `held.value.value` reads the `cents` inside `some`, then the `int`
  inside the `cents`.
- The `credit` handler returns two answers. A simulation picks one and an
  exploration tries both: an account may be closed at any moment.

## Procedures

```text
module bank.transfer
exports move-money, transfer-request, transfer-result

use bank.accounts
use bank.money

type transfer-request
  field source: accounts.account-id
  field target: accounts.account-id
  field amount: money.cents
end transfer-request

type transfer-result
  case completed
  case refused
    field reason: accounts.refusal
  case refunded
  case needs-review
end transfer-result

proc move-money
  takes request: transfer-request
  returns transfer-result
  uses accounts.ledger
body
  let debit: (uncertain accounts.debit-outcome) =
      await (accounts.ledger.debit request.source request.amount)
  match debit
    case unknown
      return (transfer-result.needs-review)
    case known
      let outcome: accounts.debit-outcome = debit.value
      match outcome
        case refused
          return (transfer-result.refused outcome.reason)
        case done
          return await (credit-or-refund request)
end move-money

proc credit-or-refund
  takes request: transfer-request
  returns transfer-result
  uses accounts.ledger
body
  let credit: (uncertain accounts.credit-outcome) =
      await (accounts.ledger.credit request.target request.amount)
  match credit
    case unknown
      return (transfer-result.needs-review)
    case known
      let outcome: accounts.credit-outcome = credit.value
      match outcome
        case done
          return (transfer-result.completed)
        case closed
          let refund: unit = await (accounts.ledger.refund request.source request.amount)
          return (transfer-result.refunded)
end credit-or-refund
```

Shapes to notice:

- `await` is the whole right-hand side of a `let` or a `return`.
- `debit` and `credit` are `once` operations, so their outcomes are
  `uncertain` and the `unknown` case must be written. `refund` is
  `repeatable` and returns its outcome directly.
- `match` narrows the name it is given. Inside `case refused`,
  `outcome.reason` exists.

## A second port

```text
module bank.records
exports audit, audit-entry, audit-model

use bank.transfer
use std.list
use std.sim
use std.unit

type audit-entry
  case finished
    field result: transfer.transfer-result
  case rejected
    field request: transfer.transfer-request
end audit-entry

type audit-log
  field entries: (list audit-entry)
end audit-log

port audit
  op record
    takes entry: audit-entry
    returns unit
    delivery repeatable
  end record
  model audit-model
end audit

model audit-model
  for audit
  state audit-log
  starts (audit-log [])
  on record
    let entries: (list audit-entry) = (list.append state.entries request.entry)
    return [(sim.answer next: (audit-log entries) outcome: (unit.value))]
end audit-model
```

## A machine

```text
module bank.desk
exports desk, desk-command

use bank.accounts
use bank.records
use bank.transfer
use std.bool
use std.int

type desk-command
  case submit
    field request: transfer.transfer-request
  case shut-down
end desk-command

type desk-state
  case open
    field active: int
  case draining
    field active: int
  case shut
end desk-state

fn active-count
  takes current: desk-state
  returns int
  means
    match current
      case open
        return current.active
      case draining
        return current.active
      case shut
        return 0
end active-count

// Accepts transfers until told to shut down, then finishes the ones in
// flight.
machine desk
  state desk-state
  starts (desk-state.open 0)
  accepts desk-command
  uses accounts.ledger, records.audit
  invariant (int.eq (active-count state) (pending transfer.move-money))
  invariant (bool.implies (desk-state.is-draining state) (int.ge (active-count state) 1))
  on open, submit
    emit (transfer.move-money event.request)
    return (desk-state.open (int.add state.active 1))
  on open, shut-down
    if (int.eq state.active 0)
      return (desk-state.shut)
    return (desk-state.draining state.active)
  on open, transfer.move-money
    emit (records.audit.record (records.audit-entry.finished event.outcome))
    return (desk-state.open (int.sub state.active 1))
  on open, records.audit.record
    return state
  on draining, submit
    emit (records.audit.record (records.audit-entry.rejected event.request))
    return state
  on draining, shut-down
    return state
  on draining, transfer.move-money
    emit (records.audit.record (records.audit-entry.finished event.outcome))
    if (int.eq state.active 1)
      return (desk-state.shut)
    return (desk-state.draining (int.sub state.active 1))
  on draining, records.audit.record
    return state
  on shut, submit
    emit (records.audit.record (records.audit-entry.rejected event.request))
    return state
  on shut, shut-down
    return state
  on shut, transfer.move-money
    impossible
  on shut, records.audit.record
    return state
end desk
```

Shapes to notice:

- Three state cases and four event sources give twelve handlers, and all
  twelve are written. The sources are the two cases of `desk-command`,
  the procedure the machine emits, and the port operation it emits.
- A handler returns the next state. `return state` keeps the current
  one.
- `on shut, transfer.move-money` is `impossible`. The first invariant
  proves it: a shut desk has no transfer pending.

The same machine in the table surface:

```text
machine desk
| state    | on                   | emit                     | return                   |
| -------- | -------------------- | ------------------------ | ------------------------ |
| open     | submit               | (transfer.move-money …)  | (desk-state.open …)      |
| open     | shut-down            |                          | see `on open, shut-down` |
| open     | transfer.move-money  | (records.audit.record …) | (desk-state.open …)      |
| open     | records.audit.record |                          | state                    |
| draining | submit               | (records.audit.record …) | state                    |
| draining | shut-down            |                          | state                    |
| draining | transfer.move-money  | (records.audit.record …) | see `on draining, …`     |
| draining | records.audit.record |                          | state                    |
| shut     | submit               | (records.audit.record …) | state                    |
| shut     | shut-down            |                          | state                    |
| shut     | transfer.move-money  |                          | impossible               |
| shut     | records.audit.record |                          | state                    |
end desk
```

Leaves are shortened with `…` here to fit the page. The real table
surface prints them whole.

## A target

```text
module bank.deploy

use bank.accounts
use bank.desk
use bank.records

target production
  runs desk.desk
  bind accounts.ledger to "postgres-ledger"
  bind records.audit to "append-only-file"
  journal to "replicated-log"
  require ensures at solved
  require invariant at bounded
  require signed
  forbid assume, extern, open, weak
  fault restart
end production
```

## A draft with holes

A cell can be staged before it is finished. This fragment can be
evaluated on the paths that avoid its holes and cannot be admitted.

```text
fn fee
  takes amount: money.cents, tier: fee-tier
  returns charged: money.cents
  ensures (int.le charged.value amount.value)
body
  match tier
    case standard
      return (money.cents.of ?rate)
    case waived
      return ?
end fee
```

## One cell as a tree

The stored form of `fib-from`, wrapped here for the page:

```text
(fn fib-from
  (takes (step int) (previous int) (current int) (limit int))
  (returns value int)
  (requires (int.le 0 step))
  (requires (int.le step limit))
  (requires (int.eq previous (fib step)))
  (requires (int.eq current (fib (int.add step 1))))
  (ensures (int.eq value (fib limit)))
  (decreases (int.sub limit step))
  (body
    (if ((int.eq step limit) (return previous)))
    (let after int (int.add step 1))
    (let next int (int.add previous current))
    (return (fib-from step: after previous: current current: next limit: limit))))
```
