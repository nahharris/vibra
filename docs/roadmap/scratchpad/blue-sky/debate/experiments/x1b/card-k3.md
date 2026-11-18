# Language card (surface K3)

You are writing a small typed functional language. Follow this card
exactly. Output only code.

## Shape

Every construct is an opening keyword, then sections that each start
with a word, then a closing keyword: `fn … endfn`, `type … endtype`,
`if … endif`, `match … endmatch`. Section words are never followed by a
colon. A colon appears only in a pair: `name: type` or `name: value`.
There are no commas: items in a list of any kind are separated by
spaces or line breaks. Indentation does not matter. Comments start with `//` and
run to the end of the line. Names use lowercase letters, digits, and
hyphens, such as `total-count`.

- **Types:** `int`, `bool`, `text`, `list(T)`, `option(T)`, and the
  names you declare.
- **Braces:** braces declare a shape. They hold a list of `name: type`
  pairs in two places: a function's parameters
  `takes {a: int b: text}` and a type's fields `fields {name: text}`
  or `case low {remaining: int}`. A pair list has at least one pair.
  Braces are never used in an expression.
- **Calls:** `name(arg arg)`. A call with no arguments is `name()`.
  A function call passes its arguments by position. A record or a
  variant case with fields is built by a call with every field
  labeled, `item(name: "x" count: c)`, in any order.
- **Literals:** `42`, `-7`, `"text"`, `true`, `false`, and lists
  `[a b c]` or `[]`. Successive items always have a space or a line
  break between them: `f(-1)` and `[-1 -2]` are fine, `f(a -1)` has
  two arguments, and `a-1` is one name.
- **Field read:** `value.field`.
- **Statements:** `let`, `if`, `match`, `return`, and a nested `fn`.
  Every path through a function ends in `return`. Each section that
  holds statements (`do`, `then`, `else`, a `case`) holds one or more
  of them, and `match` or `if` may appear inside them. A closing
  keyword must match the nearest construct that is still open:
  constructs nest, they never cross.

## Example

```text
// A wrapped type: an int that is never negative.
type quantity
  wraps int
  where int.ge(self 0)
endtype

// A record.
type item
  fields {name: text count: quantity}
endtype

// A variant. A case may have fields or none.
type stock-level
  case empty
  case low {remaining: int}
  case plenty
endtype

fn level
  takes {count: quantity threshold: int}
  returns stock-level
  requires int.ge(threshold 1)
  example level(quantity.of(0) 5) = stock-level.empty()
  do
    if int.eq(count.value 0) then
      return stock-level.empty()
    else if int.lt(count.value threshold) then
      return stock-level.low(remaining: count.value)
    else
      return stock-level.plenty()
    endif
endfn

// match lists every case. Inside a case, the matched name has that
// case's fields.
fn describe
  takes {current: stock-level}
  returns text
  do
    match current
      case empty
        return "none"
      case low
        return text.concat("only " text.from-int(current.remaining))
      case plenty
        return "plenty"
    endmatch
endfn

// Records and variant cases are built with every field labeled.
// In ensures, result is the returned value.
fn restock
  takes {it: item extra: int}
  returns item
  requires int.ge(extra 0)
  ensures int.ge(result.count.value it.count.value)
  do
    let total: int = int.add(it.count.value extra)
    return item(name: it.name count: quantity.of(total))
endfn

// try unwraps an option, or returns none from the whole function.
fn first-count
  takes {items: list(item)}
  returns option(int)
  do
    let first: item = try list.get(items 0)
    return option.some(first.count.value)
endfn

// Repetition is recursion. An if without else is followed by more
// statements.
fn total-count
  takes {items: list(item)}
  returns int
  do
    if list.is-empty(items) then
      return 0
    endif
    let head: item = list.first(items)
    return int.add(head.count.value total-count(list.rest(items)))
endfn

// A nested fn can read the names around it. A function is passed by
// name.
fn bump-all
  takes {items: list(item) extra: int}
  returns list(item)
  do
    fn bump
      takes {it: item}
      returns item
      do
        return restock(it extra)
    endfn
    return list.map(items bump)
endfn

// if can also produce a value. A function with no parameters has no
// takes section.
fn sign
  takes {n: int}
  returns int
  do
    return if int.lt(n 0) then -1 else 1 endif
endfn

fn sample
  returns list(int)
  do
    return [1 2 3]
endfn
```
