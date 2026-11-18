## Standard functions

The same in every surface. Types are written in words here; use your
surface's spelling in code. All arguments are evaluated, left to right,
before a call. Only a conditional skips evaluation of the branch not
taken.

| Function | Takes | Returns | Notes |
| --- | --- | --- | --- |
| `int.add`, `int.sub`, `int.mul` | int, int | int | Integers are unbounded |
| `int.div`, `int.rem` | int, int | int | Truncate toward zero. The divisor must not be 0 |
| `int.neg` | int | int | |
| `int.min`, `int.max` | int, int | int | |
| `int.eq`, `int.lt`, `int.le`, `int.gt`, `int.ge` | int, int | bool | `int.lt(a, b)` is "a is less than b" |
| `bool.and`, `bool.or` | bool, bool | bool | Both arguments are evaluated |
| `bool.not` | bool | bool | |
| `text.length` | text | int | |
| `text.concat` | text, text | text | |
| `text.eq` | text, text | bool | |
| `text.starts-with` | text, prefix text | bool | |
| `text.from-int` | int | text | Decimal, `-` for negatives |
| `text.to-int` | text | option of int | Optional `+` or `-`, then one or more digits `0`-`9`, nothing else. Otherwise none |
| `list.length` | list of T | int | |
| `list.is-empty` | list of T | bool | |
| `list.first` | list of T | T | The list must not be empty |
| `list.rest` | list of T | list of T | Everything after the first item. The list must not be empty |
| `list.get` | list of T, index int | option of T | Zero-based. None when out of range |
| `list.append` | list of T, item T | list of T | A new list with the item added at the end |
| `list.concat` | list of T, list of T | list of T | |
| `list.reverse` | list of T | list of T | |
| `list.map` | list of T, function T to U | list of U | |
| `list.filter` | list of T, function T to bool | list of T | |
| `list.fold` | list of T, start A, function (A, T) to A | A | Left to right. The function takes the accumulator first |
| `list.build` | count int, function int to T | list of T | Items for index 0 up to count minus 1. Count must not be negative |
| `option.some` | T | option of T | |
| `option.none` | nothing | option of T | |
| `option.is-some` | option of T | bool | |

An option is a variant with cases `some` (one field, `value`) and
`none`. Read it with `match`, or unwrap it with `try`.

For a wrapped type `w`: `w.of` takes the inner value and returns a `w`
(the type's condition must hold), and `x.value` reads the inner value.

A function is passed to `list.map` and the others by writing its name.
There are no anonymous functions, no loops, no assignment, and no
operators such as `+` or `<`. A name is bound once in a function and
never rebound there. A nested function may reuse a name from the
function around it for its own parameters. Functions may call
themselves and any function declared at the top level, in any order.

Text literals use the escapes `\"`, `\\`, `\n`, and `\t`. A record type
has at least one field.
