# 02 Syntax

Status: draft specification; not normative

## Two forms and one law

Every cell has one **canonical tree**, which the store holds and hashes,
and one or more **surfaces**, which are text. The surface agents write is
the **line surface**, stored in `.vib` files.

For every surface, a conforming toolchain MUST satisfy the round-trip law:

- printing a tree and parsing the result returns the same tree;
- parsing canonical text and printing the result returns the same text.

There is one canonical spelling per surface. A parser MAY accept
non-canonical spacing in input. A printer MUST emit only canonical text.

## The canonical tree

A tree is an atom or a parenthesized list of trees. The head of a list
names its node kind. Every keyword of the line surface is the head of the
node it writes, so the two forms share one vocabulary.

| Atom | Examples |
| --- | --- |
| Name | `total`, `split-cents`, `bank.money.cents` |
| Label | `amount:` |
| Integer | `42`, `-7`, `255u8`, `0u32`, `-1i64` |
| Float | `1.5`, `-0.25` |
| Text | `"hello\n"` |
| Boolean | `true`, `false` |
| Hole | `?`, `?fee` |

- A name segment matches `[a-z][a-z0-9]*(-[a-z0-9]+)*`. Segments are
  joined by `.`.
- An integer without a suffix has type `int`. Suffixes are `u8`, `u16`,
  `u32`, `u64`, `i8`, `i16`, `i32`, and `i64`. A literal out of range for
  its suffix is an error.
- Text is UTF-8 between double quotes with the escapes `\\`, `\"`, `\n`,
  `\t`, and `\u{...}`.
- The canonical encoding separates elements by one space and has no other
  whitespace. Its hash is the object's identity. The file form of a tree
  is `.vibon`.

## Design rules of the line surface

| # | Rule | Removes |
| --- | --- | --- |
| 1 | Every line starts with a keyword that names its node | Guessing what a line is |
| 2 | Prefix application, no infix operators, no precedence | Operator-table recall |
| 3 | Expressions nest at most three applications deep. Deeper needs a named step. The kernel enforces this | Closer counting and unnamed intermediates |
| 4 | Every binding states its type | Inference at a distance |
| 5 | Declarations close with `end` and their name. Inner blocks are indentation | Lost closers on long spans, truncated output |
| 6 | A bare name in a body is always a use. Only `let`, `var`, `for`, and parameters introduce names | Binds-or-compares confusion |
| 7 | Field read is `a.b`, indexing is `a[i]`, a call is `(f a)` | Overloaded application |
| 8 | Same-typed arguments are labeled | Swapped arguments |
| 9 | Familiar words where behavior is familiar: `fn`, `let`, `if`, `else`, `match`, `case`, `for`, `while`, `return`, `requires`, `ensures` | Learning cost |
| 10 | New words where behavior is new: `means`, `emit`, `uses`, `port`, `machine` | False friends |

The earlier [syntax note](../syntax.md) sketched this shape with other
words. This chapter replaces `defn` with `fn`, `gives` with `returns`,
`give` with `return`, a `when` chain with `if` and `else if`, and
`otherwise` with `else`.

## Lexical structure

- Source is UTF-8. A line ends with a line feed. Tabs are an error.
- Indentation is two spaces per level and is significant inside a
  declaration.
- A line beginning with `//` is a **note**. It attaches to the node that
  follows it, is kept in the tree, and is excluded from identity.
- A line longer than 100 columns is continued by breaking after `=` or
  between arguments and indenting the continuation four spaces past the
  line's start. A printer MUST break only when the limit requires it.
- One blank line separates top-level declarations. There are no other
  blank lines.

## From lines to trees

A line `k a b` followed by an indented block of lines `L1 … Ln` is the
tree `(k a b L1 … Ln)`. Punctuation is fixed per keyword and carries no
information beyond grouping:

| Surface | Tree |
| --- | --- |
| `name: type` | `(name type)` |
| `let x: t = e` | `(let x t e)` |
| `takes a: t, b: u` | `(takes (a t) (b u))` |
| `x.field` | `(get x field)` |
| `x[i]` | `(at x i)` |
| `if c` … `else if d` … `else` … | `(if (c …) (d …) (else …))` |
| `end name` | Nothing. It closes the list |

`body` is written at the indentation of the declaration keyword and opens
the witness block.

## File structure

```text
file        = "module" qname NL
              ("exports" name ("," name)* NL)?
              ("use" qname ("as" name)? NL)*
              declaration*
declaration = type | label | fn | lemma | interface | impl | port
            | proc | machine | notation | protocol | target
label       = "label" name NL "end" name
```

A notation in scope adds its own name as a declaration keyword
([below](#notations)).

## Declarations

### Types

```text
type cents
  wraps int
  where (int.ge self 0)
end cents

type transfer-request
  field source: account-id
  field target: account-id
  field amount: cents
end transfer-request

type debit-outcome
  case done
  case refused
    field reason: text
end debit-outcome
```

```text
type   = "type" name NL INDENT generic? (wrap | field+ | case+)
         ("resource" NL)? DEDENT "end" name
wrap   = "wraps" type-expr NL ("where" leaf NL)*
field  = "field" name ":" type-expr NL
case   = "case" name NL (INDENT field+ DEDENT)?
```

In a `where` clause, `self` is the wrapped value.

### Functions and lemmas

```text
fn       = "fn" qname NL INDENT signature claim* means? DEDENT
           witness? "end" qname
signature = generic? takes? returns
generic  = "generic" name (":" name)? ("," name (":" name)?)* NL
takes    = "takes" param ("," param)* NL
param    = "own"? name ":" type-expr
returns  = "returns" "any"? (name ":")? type-expr NL
claim    = "requires" leaf NL | "ensures" leaf NL
         | "decreases" leaf NL | "cost" leaf NL
         | "example" leaf "=" leaf NL | "releases" name NL
means    = "means" leaf NL | "means" NL block
witness  = "body" NL block | "body" "extern" text NL
lemma    = "lemma" qname NL INDENT generic? takes? claim* DEDENT
           witness "end" qname
```

Clauses appear in the order of the grammar. A function with `means` and no
`body` uses its meaning as its witness.

```text
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
```

### Interfaces and implementations

```text
interface ordered
  generic t
  fn compare
    takes left: t, right: t
    returns ordering
  end compare
  law reflexive
    takes a: t
    ensures (ordering.is-equal (ordered.compare left: a right: a))
  end reflexive
end ordered

impl ordered for cents
  fn compare
  body
    return (int.compare left.value right.value)
  end compare
end ordered
```

```text
interface = "interface" name NL INDENT generic (fn | law)+ DEDENT
            "end" name
law       = "law" name NL INDENT takes? claim* DEDENT "end" name
impl      = "impl" qname "for" type-expr NL INDENT fn+ DEDENT
            "end" qname
```

A member in an `impl` repeats no signature. Members are called through the
interface name, as in `(ordered.compare left: a right: b)`.

### Ports, procedures, and machines

```text
port ledger
  op debit
    takes account: account-id, amount: cents
    returns debit-outcome
    delivery once
  end debit
  model ledger-model
end ledger

proc move-money
  takes request: transfer-request
  returns transfer-result
  uses ledger
body
  let debit: (uncertain debit-outcome) = await (ledger.debit request.source request.amount)
  match debit
    case unknown
      return (transfer-result.needs-review)
    case known
      return (settle request debit.value)
end move-money

machine door
  state door-state
  starts (door-state.closed)
  accepts door-command
  uses audit
  on closed, open
    emit (audit.record (audit-entry.opened))
    set state = (door-state.open)
  on closed, close
    pass
  on closed, audit.record
    pass
  on open, open
    pass
  on open, close
    set state = (door-state.closed)
  on open, audit.record
    pass
end door
```

```text
port    = "port" name NL INDENT (op | signal)+ "model" qname NL
          DEDENT "end" name
op      = "op" name NL INDENT takes? returns
          "delivery" ("once" | "repeatable") NL DEDENT "end" name
signal  = "signal" name NL INDENT "carries" type-expr NL DEDENT
          "end" name
proc    = "proc" qname NL INDENT signature uses? claim* DEDENT
          witness "end" qname
uses    = "uses" qname ("," qname)* NL
machine = "machine" qname NL INDENT takes?
          "state" type-expr NL "starts" leaf NL
          ("accepts" type-expr NL)? uses? ("invariant" leaf NL)*
          handler+ DEDENT "end" qname
handler = "on" name "," source NL block
source  = name | qname
```

[05](05-execution.md) gives their meaning. `target`, `notation`, and
`protocol` are specified where they are used
([06](06-runtime.md#targets-and-policy), [below](#notations),
[05](05-execution.md#protocols)).

## Statements

```text
block     = INDENT statement+ DEDENT
statement = let | var | set | if | match | for | while | return
          | assert | assume | apply | emit | impossible | pass | fn
let       = "let" name ":" type-expr "=" rhs NL
var       = "var" name ":" type-expr "=" rhs NL
set       = "set" name "=" rhs NL
rhs       = leaf | "try" leaf | "await" leaf
if        = "if" leaf NL block ("else" "if" leaf NL block)*
            ("else" NL block)?
match     = "match" path NL INDENT ("case" name NL block)+ DEDENT
for       = "for" name "in" leaf NL INDENT invariant* statement+ DEDENT
while     = "while" leaf NL INDENT invariant* decreases statement+ DEDENT
invariant = "invariant" leaf NL
return    = "return" rhs NL
assert    = "assert" leaf NL
assume    = "assume" leaf NL
apply     = "apply" leaf NL
emit      = "emit" leaf NL
impossible = "impossible" NL
pass      = "pass" NL
```

| Statement | Meaning | Allowed in |
| --- | --- | --- |
| `let` | Bind an immutable name | Everywhere |
| `var`, `set` | Declare and reassign a local variable | `body` |
| `if`, `else` | Choose a block. The condition is a `bool` | Everywhere |
| `match`, `case` | Choose a block by variant case. All cases are listed | Everywhere |
| `for … in` | Run a block for each element of a finite collection | `body` |
| `while` | Run a block while a condition holds, with a measure | `body` |
| `return` | End the function with a value | `fn`, `proc`, `means` |
| `assert` | State a fact for the checker to prove | `body` |
| `assume` | State a fact without proof. Creates a debt | `body` |
| `apply` | Use a lemma | `body` |
| `try` | Unwrap an `option` or `result`, or return its failure | `fn`, `proc` |
| `await` | Perform an intent and suspend until its outcome | `proc` |
| `emit` | Send an intent without waiting | `machine` |
| `impossible` | Assert that this point cannot be reached | Everywhere |
| `pass` | Do nothing | Handlers and lemmas |
| nested `fn` | Declare a local function. It may read enclosing `let` bindings | `body` |

Rules:

- Every path through a `fn` or `proc` body MUST end in `return`.
- A `match` subject is a name or field path. Inside `case k` the subject
  has the fields of case `k`. There are no patterns and no binders.
- `try` and `await` appear only as the whole right-hand side of `let`,
  `var`, `set`, or `return`. Suspension points are therefore statements.
- `var` and `set` MUST NOT appear in a `means` block.

## Leaves

A **leaf** is the expression language. It has no statements and no
blocks.

```text
leaf  = atom | path | index | apply | form
path  = name ("." name)+
index = (name | path) "[" leaf "]"
apply = "(" qname arg* ")"
arg   = leaf | label leaf
form  = "(" "if" leaf leaf leaf ")"
      | "(" "forall" name ":" type-expr leaf ")"
      | "(" "exists" name ":" type-expr leaf ")"
```

- **Depth.** An atom, path, or index of atoms has depth 0. An application
  or form has depth one more than its deepest argument. A leaf MUST NOT
  exceed depth 3.
- **Construction is application.** A record is built by its type name and
  a variant by its qualified case: `(decoded scalar: s width: w)`,
  `(option.some x)`, `(option.none)`. A constructor with more than one
  field labels every field.
- **Labels.** A call MUST label each argument whose parameter shares its
  type with another parameter of the callee, using the parameter name,
  and MUST NOT label any other argument. Primitive operators such as
  `int.sub` are positional.
- **Functions as values.** A function is passed by name:
  `(list.map shares cents.value)`. There are no anonymous functions.
- **`if` as a leaf** chooses between two values. Only the chosen branch
  is evaluated, and each branch is checked under its condition.
- **Quantifiers** are allowed only in claim clauses, `invariant`,
  `assert`, and lemmas.
- **Holes.** `?` or `?name` stands for a missing leaf, type, or block. A
  cell with a hole can be staged and evaluated and cannot be admitted.

Type expressions:

```text
type-expr = qname | "(" qname type-expr+ ")"
          | "(" "fn" type-expr* "->" type-expr ")"
```

## A longer example

The UTF-8 lead-byte decoder from the syntax note, in the final surface:

```text
fn sequence
  takes items: (array u8), index: u64, first: u32
  returns (option decoded)
body
  if (within value: first low: 0u32 high: 127u32)
    return (option.some (decoded scalar: first width: 1u64))
  else if (within value: first low: 194u32 high: 223u32)
    let second: u32 = try (continuation items index low: 128u32 high: 191u32)
    let high: u32 = (u32.mul (u32.sub first 192u32) 64u32)
    return (option.some (decoded scalar: (u32.add high second) width: 2u64))
  else if (within value: first low: 224u32 high: 239u32)
    return (three items index first)
  else if (within value: first low: 240u32 high: 244u32)
    return (four items index first)
  else
    return (option.none)
end sequence
```

`(u32.sub first 192u32)` is admitted because the guard proves `first` is
at least 194. Without the guard it is an unproved obligation, not a
run-time wraparound ([03](03-types.md#numbers)).

## Tables

A machine's handlers form a grid of state cases against event sources.
The **table surface** shows the same tree as a grid:

```text
machine transfer
| state     | on           | to        | emit                        |
| --------- | ------------ | --------- | --------------------------- |
| idle      | start        | debiting  | (ledger.debit source amount) |
| idle      | ledger.debit | idle      | impossible                  |
| debiting  | start        | debiting  | (reply.busy)                |
| debiting  | ledger.debit | ?         | ?                           |
end transfer
```

- The tool writes the first two columns, one row for every pair. The
  author fills the others.
- A `?` is a hole. A table with a hole is not admitted.
- A handler too large for a row is shown as a reference to its `on`
  block in the line surface.

The table surface is a projection and obeys the round-trip law.

## Notations

A **notation** is a cell that adds a block form for one domain.

```text
layout utf8-scalar
  returns char
  0xxxxxxx
  110xxxxx 10xxxxxx
  1110xxxx 10xxxxxx 10xxxxxx
  11110xxx 10xxxxxx 10xxxxxx 10xxxxxx
  where shortest-form, scalar-value
end utf8-scalar
```

Here `layout` is a notation from the standard library. From the block it
derives a decoder, an encoder, and the law that they are inverses.

A `notation` cell has four parts:

| Part | Requirement |
| --- | --- |
| Grammar | A context-free grammar over lines and tokens, given as data |
| Elaboration | A total function from the parsed block to core declarations |
| Printer | A total function from those declarations back to the block |
| Law | Evidence that printer and elaboration round-trip |

- A notation MUST NOT capture names from its use site, perform effects, or
  depend on anything but its block.
- A notation is identified by hash and imported with `use` like any cell.
- Because the grammar is data, tools can constrain generation to it and
  can generate its documentation.
- A notation SHOULD be added only for a domain that already writes its
  subject that way: byte layouts, grammars, state tables, units, time,
  relational queries.

## Canonical format

- Clauses and statements are printed in grammar order, one per line.
- One space separates tokens. There is no alignment padding outside the
  table surface.
- `exports` and `use` lines are sorted by name.
- The formatter never renames, reorders statements, or changes a tree.
  Formatting is printing.
