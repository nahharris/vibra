# Experiment X1: frozen task statements

Status: frozen once both reviewers accept; then hashed

Each section between `<!-- task tN -->` markers is the exact text a
participant receives after the card and the standard-function table.
The same text is used for every surface. Tasks 9 and 10 insert a
program in the participant's surface at `{{PROGRAM}}`, taken from the
reference solutions, which the harness has checked to be the same tree
in all four surfaces.

Every statement ends with the same instruction: "Output the complete
program in one code block and nothing else."

<!-- task t1 -->
## Task

Declare a function `clamp` that takes `x`, `lo`, and `hi` (each an int)
and returns an int. It requires that `lo` is less than or equal to
`hi`. It returns `lo` when `x` is below `lo`, `hi` when `x` is above
`hi`, and `x` otherwise.

Give it one `requires` clause, one `ensures` clause saying the result
is at least `lo`, and two `example` clauses: `clamp` of 5, 0, 3 is 3,
and `clamp` of -2, 0, 3 is 0.

Output the complete program in one code block and nothing else.
<!-- end -->

<!-- task t2 -->
## Task

Declare a variant type `grade` with four cases that have no fields:
`fail`, `pass`, `merit`, `distinction`.

Declare a function `grade-of` that takes `score` (an int) and returns
an option of `grade`. A score below 0 or above 100 gives none. 0 to 49
gives `fail`, 50 to 64 gives `pass`, 65 to 79 gives `merit`, and 80 to
100 gives `distinction`, each wrapped in some.

Output the complete program in one code block and nothing else.
<!-- end -->

<!-- task t3 -->
## Task

Declare a variant type `shape` with three cases: `circle` with a field
`radius` (int), `rect` with fields `width` and `height` (int), and
`square` with a field `side` (int).

Declare a function `area` that takes `s` (a `shape`) and returns an
int: 3 times radius times radius for a circle, width times height for
a rect, and side times side for a square.

Output the complete program in one code block and nothing else.
<!-- end -->

<!-- task t4 -->
## Task

Declare a function `sum-positive` that takes `xs` (a list of int) and
returns an int: the sum of the items that are greater than 0. An empty
list gives 0.

Write it by recursion on the list, using `list.is-empty`,
`list.first`, and `list.rest`. Do not use `list.fold`, `list.filter`,
or `list.map`.

Output the complete program in one code block and nothing else.
<!-- end -->

<!-- task t5 -->
## Task

Declare a wrapped type `cents`: an int that is at least 0.

Declare a record type `line` with fields `quantity` (int) and `price`
(`cents`).

Declare a function `total-due` that takes `lines` (a list of `line`)
and returns `cents`: the sum over all lines of quantity times price.
It requires nothing; you may assume every quantity is at least 0.

Compute it with `list.map` and `list.fold`, passing functions you
declare by name.

Output the complete program in one code block and nothing else.
<!-- end -->

<!-- task t6 -->
## Task

Declare a wrapped type `cents`: an int that is at least 0.

Declare a function `cents-total` that takes `items` (a list of
`cents`) and returns an int: the sum of their inner values.

Declare a function `split-cents` that takes `total` (`cents`) and
`parts` (int) and returns a list of `cents`. It requires `parts` to be
at least 1. It splits `total` into exactly `parts` shares that add up
to `total`, where no two shares differ by more than 1 and larger shares
come first. For example, 100 split into 3 parts is 34, 33, 33.

Give `split-cents` one `requires` clause, an `ensures` clause saying
the result has `parts` items, an `ensures` clause saying
`cents-total` of the result equals the inner value of `total`, and one
`example` clause for 100 split into 3.

Output the complete program in one code block and nothing else.
<!-- end -->

<!-- task t7 -->
## Task

Declare a function `third-item-doubled` that takes `xs` (a list whose
items are each an option of int) and returns an option of int.

Take the item at index 2 (the third item). If the list has fewer than
three items, the result is none. If that item is none, the result is
none. Otherwise the result is some of twice its value.

Use `try` for both steps.

Output the complete program in one code block and nothing else.
<!-- end -->

<!-- task t8 -->
## Task

Write a command-line argument parser.

Declare these types:

- A record `settings` with fields `verbose` (bool), `output` (option
  of text), and `inputs` (list of text).
- A variant `arg-error` with three cases, each with fields `index`
  (int) and `token` (text): `unknown-flag`, `missing-value`, and
  `duplicate`.
- A variant `parse-result` with two cases: `ok` with a field
  `settings` (`settings`), and `error` with a field `error`
  (`arg-error`).

Declare a function `parse-args` that takes `args` (a list of text) and
returns a `parse-result`. Read the tokens left to right, by position
starting at index 0:

1. The token `--` ends option reading. Every token after it is an
   input, whatever it looks like. The `--` itself is not an input.
2. The token `--verbose` sets `verbose` to true. Repeating it is fine.
3. The token `--output` takes the next token as its value, whatever
   that token looks like, and sets `output` to some of it.
   - If `output` was already set, stop with `duplicate`, giving the
     index of this `--output` token and the token `--output`. Check
     this before looking for the value.
   - If there is no next token, stop with `missing-value`, giving the
     index of the `--output` token and the token `--output`.
4. Any other token that starts with `-` stops with `unknown-flag`,
   giving its index and the token.
5. Any other token is an input, added to `inputs` in order.

The first error found, reading left to right, is the result. With no
error the result is `ok` with the settings. Before any token is read,
`verbose` is false, `output` is none, and `inputs` is empty.

Output the complete program in one code block and nothing else.
<!-- end -->

<!-- task t9 -->
## Task

Here is a working command-line argument parser:

```text
{{PROGRAM}}
```

Change it to add an option `--limit`:

- Add a field `limit` (option of int) to `settings`. It starts as
  none.
- Add a case `bad-limit` to `arg-error`, with fields `index` (int) and
  `token` (text).
- The token `--limit` takes the next token as its value, whatever that
  token looks like.
  - If `limit` was already set, stop with `duplicate`, giving the
    index of this `--limit` token and the token `--limit`. Check this
    first.
  - If there is no next token, stop with `missing-value`, giving the
    index of the `--limit` token and the token `--limit`.
  - Convert the value with `text.to-int`. If that gives none, or the
    number is below 0, stop with `bad-limit`, giving the index of the
    value token and the value token.
  - Otherwise set `limit` to some of the number.

Everything else behaves as before.

Output the complete changed program in one code block and nothing else.
<!-- end -->

<!-- task t10 -->
## Task

The program below is a command-line argument parser that was cut off
partway through. The rules it implements are:

1. The token `--` ends option reading. Every token after it is an
   input. The `--` itself is not an input.
2. The token `--verbose` sets `verbose` to true. Repeating it is fine.
3. The token `--output` takes the next token as its value and sets
   `output` to some of it. If `output` was already set, stop with
   `duplicate` (index and token of this `--output`), checked first. If
   there is no next token, stop with `missing-value` (index and token
   of the `--output`).
4. Any other token that starts with `-` stops with `unknown-flag`
   (its index and the token).
5. Any other token is an input, added to `inputs` in order.

```text
{{PROGRAM}}
```

Continue the program from exactly where it stops, so that the text
above followed by your text is one complete, correct program. Do not
repeat any of the text above.

Output only your continuation in one code block and nothing else.
<!-- end -->
