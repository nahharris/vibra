# 02 Syntax

Status: draft specification; not normative

This chapter defines every shape of the language. The
[sample](09-sample.md) shows all of them in use. The last section,
[Shape decisions](#shape-decisions), lists each choice made here beside
its alternatives, for review.

## Two forms and one law

Every cell has one **canonical tree**, which the store holds and hashes,
and one or more **surfaces**, which are text. The surface agents write is
the **line surface**, stored in `.vib` files.

For every surface, a conforming toolchain MUST satisfy the round-trip law:

- printing a tree and parsing the result returns the same tree;
- parsing canonical text and printing the result returns the same text.

There is one canonical spelling per surface. A parser MAY accept
non-canonical spacing in input. A printer MUST emit only canonical text.

## Design rules

| # | Rule | Removes |
| --- | --- | --- |
| 1 | Every line starts with a keyword that names its node | Guessing what a line is |
| 2 | Prefix application, no infix operators, no precedence | Operator-table recall |
| 3 | Parentheses mean application and nothing else | Grouping parentheses and their miscounting |
| 4 | Expressions nest at most three levels. Deeper needs a named step. The kernel enforces this | Closer counting and unnamed intermediates |
| 5 | Every binding states its type | Inference at a distance |
| 6 | Declarations close with `end` and their name. Inner blocks are indentation | Lost closers on long spans, truncated output |
| 7 | A bare name in a body is always a use. Only `let` and parameters introduce names | Binds-or-compares confusion |
| 8 | A field read is `a.b`, a call is `(f a)`, a collection is `[…]` or `{…}` | One spelling with several meanings |
| 9 | Same-typed arguments are labeled | Swapped arguments |
| 10 | Familiar words where behavior is familiar: `fn`, `let`, `if`, `else`, `match`, `case`, `return`, `requires`, `ensures` | Learning cost |
| 11 | New words where behavior is new: `means`, `emit`, `uses`, `port`, `machine` | False friends |

There is no assignment and no loop form. Iteration is by standard
transforms and recursion ([03](03-types.md#iteration)).

The earlier [syntax note](../syntax.md) sketched this shape with other
words. This chapter replaces `defn` with `fn`, `gives` with `returns`,
`give` with `return`, a `when` chain with `if` and `else if`, and
`otherwise` with `else`.

## Characters and lines

- Source is UTF-8. A line ends with a line feed. Tabs are an error.
- Indentation is two spaces per level and is significant.
- One space separates tokens. No space follows `(`, `[`, or `{`, or
  precedes `)`, `]`, `}`, `,`, or `:`.
- A line beginning with `//` is a **note**. It attaches to the node that
  follows it, is kept in the tree, and is excluded from identity. There
  are no end-of-line notes.
- A line longer than 100 columns is continued by breaking after `=` or
  between arguments and indenting the continuation four spaces past the
  line's start. A printer MUST break only when the limit requires it.
- One blank line separates top-level declarations. There are no other
  blank lines.

## Tokens

| Token | Form | Examples |
| --- | --- | --- |
| Name | `[a-z][a-z0-9]*(-[a-z0-9]+)*` | `total`, `split-cents`, `utf8` |
| Qualified name | Names joined by `.` | `cents.sum`, `bank.money.cents` |
| Integer | Optional `-`, digits, optional suffix | `42`, `-7`, `255u8`, `-1i64` |
| Float | Optional `-`, digits, `.`, digits | `1.5`, `-0.25` |
| Text | Double-quoted, with `\\`, `\"`, `\n`, `\t`, `\u{…}` | `"hello\n"` |
| Boolean | | `true`, `false` |
| Hole | `?` with an optional name | `?`, `?fee` |
| Punctuation | | `(` `)` `[` `]` `{` `}` `,` `:` `=` `->` `.` |

- An integer without a suffix has type `int`. Suffixes are `u8`, `u16`,
  `u32`, `u64`, `i8`, `i16`, `i32`, and `i64`. A literal out of range for
  its suffix is an error.
- Digits may be grouped with `_`, as in `1_000_000`. The canonical form
  groups by three from the right for numbers of five or more digits.
- There are no character literals, no text interpolation, and no
  multi-line text.

**Reserved words.** These cannot be used as names:

```text
accepts any apply as assert assume await body carries case cost decreases
delivery else emit end ensures example exists exports extern false field
fn for forall generic if impl impossible interface invariant label law
lemma let machine match means model module on op own pass port proc
releases requires resource return returns signal starts state takes
target true try type use uses where wraps
```

The words `at`, `bind`, `fault`, `forbid`, `journal`, `require`, `runs`,
`signed`, and `to` are keywords only inside a `target`.

`state`, `event`, `request`, `self`, and `now` are predefined names in
the places [05](05-execution.md) and [03](03-types.md) give them meaning.

## Brackets and punctuation

Each bracket has one meaning.

| Bracket | Means | Used for |
| --- | --- | --- |
| `( )` | Application | Calls, constructors, leaf forms, compound types |
| `[ ]` | Sequence | List and array literals |
| `{ }` | Keyed collection | Set and map literals |

Which shapes take which brackets:

| Shape | Written | Brackets |
| --- | --- | --- |
| Call | `(cents.sum shares)` | Parentheses, always |
| Call with no arguments | `(option.none)` | Parentheses, always |
| Record construction | `(decoded scalar: first width: 1u64)` | Parentheses |
| Leaf form | `(if (int.lt a b) a b)` | Parentheses |
| Compound type | `(list cents)`, `(fn int -> cents)` | Parentheses |
| Simple type | `cents` | None |
| Name, literal | `total`, `42`, `"text"` | None |
| Field read | `request.amount` | None |
| Function as a value | `cents.value` | None |
| List or array literal | `[a, b, c]`, `[]` | Square brackets |
| Set literal | `{a, b}` | Braces |
| Map literal | `{k: v, j: w}`, `{}` | Braces |
| Statement operand | `if (int.lt n 2)`, `if done`, `return total` | None of its own |
| Parameters, fields, bindings | `takes a: t, b: u` | None |
| Block | Indentation | None |

Rules:

- Parentheses never group. `(x)` and `((f a))` are errors. A leaf that is
  not an application has no parentheses.
- An application always has parentheses, including one with no arguments.
  A bare function name is the function as a value.
- A statement keyword adds no brackets. `if (int.lt n 2)` has parentheses
  because its condition is a call, and `if done` has none.
- Inside `( )` arguments are separated by spaces. Inside `[ ]` and `{ }`
  elements are separated by `, `. There is no trailing comma.
- A `[ ]` literal is a list or an array, and a `{ }` literal is a set or
  a map, as its expected type says. All entries of a `{ }` literal have
  the same form.

Punctuation:

| Mark | Means | Where |
| --- | --- | --- |
| `:` | "has type" after a name being declared | `let`, `takes`, `returns`, `field`, quantifiers |
| `:` | "is" after a label or key | Labeled arguments, map entries |
| `,` | Separates items of a line-level list or a bracket literal | `takes`, `generic`, `uses`, `exports`, `on`, `forbid`, `[ ]`, `{ }` |
| `=` | "is defined as" | `let`, `example` |
| `->` | Separates parameter types from the result type | Function types only |
| `.` | Qualifies a name or reads a field | Everywhere |

## Blocks and closers

| Construct | Opens | Closes |
| --- | --- | --- |
| `type`, `label`, `fn`, `lemma`, `interface`, `law`, `port`, `op`, `signal`, `model`, `proc`, `machine`, `target`, and notation blocks | Keyword and name | `end` and the same name |
| `impl i for t` | Keyword | `end i` |
| `body` | The word alone, at the indentation of its declaration | The declaration's `end` |
| `means`, `if`, `else`, `match`, `case`, `on` | The line | Dedent |

A nested `fn` inside a body closes with `end` and its name like any other
declaration.

## Grammar

`NL` is a line end. `INDENT` and `DEDENT` are a change of one indentation
level. `name`, `qname`, `integer`, `float`, and `text` are tokens.

### Files

```text
file        = "module" qname NL exports? use* declaration*
exports     = "exports" qname ("," qname)* NL
use         = "use" qname ("as" name)? NL
declaration = type | label | fn | lemma | interface | impl | port
            | model | proc | machine | target | notation-block
```

### Types and labels

```text
type   = "type" name NL
         INDENT generic? shape ("resource" NL)? DEDENT
         "end" name NL
shape  = "wraps" type-expr NL where* | field+ | case+
where  = "where" leaf NL
field  = "field" name ":" type-expr NL
case   = "case" name NL (INDENT field+ DEDENT)?
label  = "label" name NL "end" name NL
```

### Functions and lemmas

```text
fn      = "fn" qname NL
          INDENT generic? takes? returns clause* DEDENT
          body?
          "end" qname NL
lemma   = "lemma" qname NL
          INDENT generic? takes? clause* DEDENT
          body
          "end" qname NL
generic = "generic" tparam ("," tparam)* NL
tparam  = name (":" qname)?
takes   = "takes" param ("," param)* NL
param   = "own"? name ":" type-expr
returns = "returns" "any"? (name ":")? type-expr NL
clause  = "requires" leaf NL | "ensures" leaf NL | "decreases" leaf NL
        | means | "example" leaf "=" leaf NL | "cost" leaf NL
        | "releases" qname NL
means   = "means" leaf NL | "means" NL block
body    = "body" NL block | "body" "extern" text NL
```

Clauses appear in this order: `requires`, `ensures`, `decreases`, `means`,
`example`, `cost`, `releases`. A function with `means` and no `body` uses
its meaning as its witness. `means` belongs to `fn` only.

### Interfaces and implementations

```text
interface = "interface" name NL
            INDENT generic (fn | law)+ DEDENT
            "end" name NL
law       = "law" name NL INDENT takes? clause* DEDENT "end" name NL
impl      = "impl" qname "for" type-expr NL
            INDENT fn+ DEDENT
            "end" qname NL
```

A member in an `impl` repeats no signature.

### Ports, models, procedures, machines, targets

```text
port    = "port" name NL
          INDENT (op | signal)+ "model" qname NL DEDENT
          "end" name NL
op      = "op" name NL
          INDENT takes? returns "delivery" ("once" | "repeatable") NL DEDENT
          "end" name NL
signal  = "signal" name NL INDENT "carries" type-expr NL DEDENT
          "end" name NL
model   = "model" name NL
          INDENT "for" qname NL "state" type-expr NL "starts" leaf NL
          answer+ DEDENT
          "end" name NL
answer  = "on" name NL block
proc    = "proc" qname NL
          INDENT generic? takes? returns uses? clause* DEDENT
          body
          "end" qname NL
uses    = "uses" qname ("," qname)* NL
machine = "machine" qname NL
          INDENT takes? "state" type-expr NL "starts" leaf NL
          ("accepts" type-expr NL)? uses? invariant* handler+ DEDENT
          "end" qname NL
invariant = "invariant" leaf NL
handler = "on" name "," qname NL block
target  = "target" name NL
          INDENT "runs" qname NL bind+ "journal" "to" text NL policy* DEDENT
          "end" name NL
bind    = "bind" qname "to" text NL
policy  = "require" name "at" name NL | "require" "signed" NL
        | "forbid" name ("," name)* NL | "fault" name NL
```

### Statements

```text
block      = INDENT statement+ DEDENT
statement  = let | if | match | return | assert | assume | apply
           | emit | impossible | pass | fn
let        = "let" name ":" type-expr "=" rhs NL
rhs        = leaf | "try" leaf | "await" leaf
if         = "if" leaf NL block
             ("else" "if" leaf NL block)*
             ("else" NL block)?
match      = "match" subject NL INDENT arm+ DEDENT
subject    = name ("." name)*
arm        = "case" name NL block
return     = "return" rhs NL
assert     = "assert" leaf NL
assume     = "assume" leaf NL
apply      = "apply" leaf NL
emit       = "emit" leaf NL
impossible = "impossible" NL
pass       = "pass" NL
```

### Leaves and types

```text
leaf      = name | qname | literal | path | apply | form | list | keyed
          | hole
literal   = integer | float | text | "true" | "false"
path      = name ("." name)+
apply     = "(" qname arg* ")"
arg       = (name ":")? leaf
form      = "(" "if" leaf leaf leaf ")"
          | "(" "forall" name ":" type-expr leaf ")"
          | "(" "exists" name ":" type-expr leaf ")"
list      = "[" (leaf ("," leaf)*)? "]"
keyed     = "{" (entry ("," entry)*)? "}"
entry     = leaf (":" leaf)?
hole      = "?" name?
type-expr = qname
          | "(" qname type-expr+ ")"
          | "(" "fn" type-expr* "->" type-expr ")"
```

## Statements

| Statement | Meaning | Allowed in |
| --- | --- | --- |
| `let` | Bind an immutable name | Everywhere |
| `if`, `else if`, `else` | Choose a block. The condition is a `bool` | Everywhere |
| `match`, `case` | Choose a block by variant case. Every case is listed | Everywhere |
| `return` | End the function, procedure, or handler with a value | `fn`, `proc`, `means`, handlers |
| `assert` | State a fact for the checker to prove | `body` |
| `assume` | State a fact without proof. Creates a debt | `body` |
| `apply` | Use a lemma | `body` |
| `try` | Unwrap an `option` or `result`, or return its failure | `fn`, `proc` |
| `await` | Request an intent and suspend until its outcome | `proc` |
| `emit` | Request an intent without waiting | Machine handlers |
| `impossible` | State that this point cannot be reached | Everywhere |
| `pass` | Do nothing | Lemmas |
| nested `fn` | Declare a local function. It may read enclosing bindings | `body` |

Rules:

- Every path through a block of a `fn`, `proc`, `means`, or handler MUST
  end in `return` or `impossible`. An `if` without `else` is followed by
  further statements.
- A `match` subject is a name or field path. Inside `case k` the subject
  has the fields of case `k`. There are no patterns and no binders.
- `try` and `await` appear only as the whole right-hand side of `let` or
  `return`. Suspension points are therefore statements.
- A name is bound once. There is no reassignment and no shadowing.

## Leaves

A **leaf** is the expression language. It has no statements and no
blocks.

- **Depth.** A name, literal, or path has depth 0. An application, form,
  or bracket literal has depth one more than its deepest part. A leaf
  MUST NOT exceed depth 3.
- **Construction is application.** A record is built by its type name and
  a variant by its qualified case: `(decoded scalar: s width: w)`,
  `(option.some x)`, `(option.none)`. A constructor with more than one
  field labels every field. A constructor with one field labels none.
- **Labels.** A call MUST label each argument whose parameter shares its
  type with another parameter of the callee, using the parameter name,
  and MUST NOT label any other argument. Functions of the built-in scalar
  modules (`bool`, `int`, the fixed-width integers, and `f64`) are
  positional.
- **Functions as values.** A function is passed by name:
  `(list.map shares cents.value)`. There are no anonymous functions. A
  function needed in one place is a nested `fn`.
- **`if` as a leaf** chooses between two values. Only the chosen branch
  is evaluated, and each branch is checked under its condition.
- **Quantifiers** are allowed only in claim clauses, `invariant`,
  `assert`, and lemmas.
- **Holes.** `?` or `?name` stands for a missing leaf, type, or block. A
  cell with a hole can be staged and evaluated and cannot be admitted.

## The canonical tree

A tree is an atom or a parenthesized list of trees. The head of a list
names its node kind. Every keyword of the line surface is the head of the
node it writes, so the two forms share one vocabulary.

Atoms are the tokens above, plus labels such as `amount:`. The canonical
encoding separates elements by one space and has no other whitespace. Its
hash is the object's identity. The file form of a tree is `.vibon`.

A line `k a b` followed by an indented block of lines `L1 … Ln` is the
tree `(k a b L1 … Ln)`. Punctuation is fixed per keyword and carries no
information beyond grouping:

| Surface | Tree |
| --- | --- |
| `let x: t = e` | `(let x t e)` |
| `takes a: t, b: u` | `(takes (a t) (b u))` |
| `returns shares: (list cents)` | `(returns shares (list cents))` |
| `x.field` | `(get x field)` |
| `[a, b]` | `(seq a b)` |
| `{a, b}`, `{k: v}` | `(keyed a b)`, `(keyed (k v))` |
| `if c` … `else if d` … `else` … | `(if (c …) (d …) (else …))` |
| `on open, submit` | `(on open submit …)` |
| `body` | `(body …)` |
| `end name` | Nothing. It closes the list |

```text
fn cents.add
  takes left: cents, right: cents
  returns cents
  means (cents.of (int.add left.value right.value))
end cents.add
```

is the tree

```text
(fn cents.add (takes (left cents) (right cents)) (returns cents)
  (means (cents.of (int.add (get left value) (get right value)))))
```

shown here on two lines; the stored form has none.

## Tables

A machine's handlers form a grid of state cases against event sources.
The **table surface** shows the same tree as a grid.

```text
machine door
| state  | on           | emit                                | return            |
| ------ | ------------ | ----------------------------------- | ----------------- |
| closed | open         | (audit.record (audit-entry.opened)) | (door-state.open) |
| closed | close        |                                     | state             |
| closed | audit.record |                                     | state             |
| open   | open         |                                     | state             |
| open   | close        | ?                                   | ?                 |
| open   | audit.record |                                     | state             |
end door
```

- The tool writes the first two columns, one row for every pair. The
  author fills the others.
- A `?` is a hole. A table with a hole is not admitted.
- A handler that needs more than `emit` and `return` is shown as a
  reference to its `on` block in the line surface.
- The table surface pads cells to align columns. It obeys the round-trip
  law.

## Notations

A **notation** is a cell that adds a block form for one domain. A
notation in scope adds its own name as a declaration keyword.

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

A notation cell has four parts:

| Part | Requirement |
| --- | --- |
| Grammar | A context-free grammar over lines and tokens, given as data |
| Elaboration | A total function from the parsed block to core declarations |
| Printer | A total function from those declarations back to the block |
| Law | Evidence that printer and elaboration round-trip |

- A notation block opens with the notation's name and a cell name, and
  closes with `end` and the cell name. Between them the notation's own
  grammar applies.
- A notation MUST NOT capture names from its use site, perform effects, or
  depend on anything but its block.
- A notation is identified by hash and imported with `use` like any cell.
- Because the grammar is data, tools can constrain generation to it and
  can generate its documentation.
- A notation SHOULD be added only for a domain that already writes its
  subject that way: byte layouts, grammars, state tables, units, time,
  relational queries.

How a notation cell is itself written is not yet specified.

## Canonical format

- Clauses and statements are printed in grammar order, one per line.
- `exports` and `use` lines are sorted by name.
- The formatter never renames, reorders statements, or changes a tree.
  Formatting is printing.

## Shape decisions

Each row is a choice this chapter makes. The alternatives are the ones
worth weighing. Changing a row changes the printer and parser of the line
surface and nothing else: the tree, the kernel, and every other chapter
are unaffected.

| # | Shape | This draft | Alternatives |
| --- | --- | --- | --- |
| 1 | Call | `(f a b)` | `f(a, b)` |
| 2 | Argument separator | Spaces | Commas |
| 3 | Call with no arguments | `(option.none)` | `option.none` |
| 4 | Grouping parentheses | None exist | Allowed around any leaf |
| 5 | Compound type | `(list cents)` | `list<cents>`, `[cents]` |
| 6 | Function type | `(fn int -> cents)` | `(int) -> cents` |
| 7 | List and array literal | `[a, b]` | `[a b]`, `(list.of a b)` |
| 8 | Set and map literal | `{a, b}`, `{k: v}` | `(set.of a b)`, `(map.of …)`, `#{a, b}` for sets |
| 9 | Separator in literals | `, ` | Spaces, to match calls |
| 10 | Record construction | `(point x: 1 y: 2)` | `point {x: 1, y: 2}`, `{x: 1, y: 2}` typed by context |
| 11 | Labeled argument | `left: a` | `left = a` |
| 12 | Field read | `p.x` | `(point.x p)` only |
| 13 | Indexing | `(array.at items i)` | `items[i]` |
| 14 | Signature | `takes` and `returns` lines | `fn f(a: t, b: u) -> r` on one line |
| 15 | Parameter list | One `takes` line, commas | One line per parameter |
| 16 | Named result | `returns shares: (list cents)` | A separate `result shares` clause |
| 17 | Binding | `let x: t = e` | `x: t = e` |
| 18 | Condition | `if leaf` | `if (leaf)` with required parentheses |
| 19 | Inner blocks | Indentation | `end` on every block, or braces |
| 20 | Declaration closer | `end name` | `end`, or indentation only |
| 21 | Witness marker | `body` | `does`, `is`, `begin` |
| 22 | Conditional value | `(if c a b)` | Statement `if` only |
| 23 | Local function | Nested `fn` with a name | `(fn x: t -> r => leaf)` |
| 24 | Variant test | `match x` then `case k`, narrowing `x` | Patterns with binders |
| 25 | Handler header | `on open, submit` | `on open submit` |
| 26 | Note | `//` line attached to the next node | `#`, `;`, end-of-line notes |
| 27 | Unit value | `(unit.value)` | `()` |
| 28 | Literal suffix | `255u8` | `(u8.of 255)` |

Rows 7, 8, and 13 interact. With `[ ]` used for literals, indexing is a
call, so that `[` never depends on the space before it.
