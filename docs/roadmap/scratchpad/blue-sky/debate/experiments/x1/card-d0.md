# Language card (surface D)

You are writing a small typed functional language. Follow this card
exactly. Output only code.

## Shape

Every line starts with a keyword. Blocks are indented by two spaces,
and indentation matters. A declaration closes with `end` and its name.
The word `body` stands alone, at the indentation of its `fn`, and opens
the function's statements. Comments are whole lines starting with `//`.
Names use lowercase letters, digits, and hyphens, such as
`total-count`.

- **Types:** `int`, `bool`, `text`, `(list T)`, `(option T)`, and the
  names you declare.
- **Calls:** `(name arg arg)`, in parentheses, arguments separated by
  spaces. A call with no arguments is `(name)`. Parentheses are only
  for calls: never wrap a name or a literal in them.
- **Literals:** `42`, `-7`, `"text"`, `true`, `false`, and lists
  `[a, b, c]` or `[]`.
- **Field read:** `value.field`.
- **Statements:** `let`, `if`, `match`, `return`, and a nested `fn`.
  Every path through a function ends in `return`. Each indented block
  (under `body`, `if`, `else`, a `case`) holds one or more
  statements, and `match` or `if` may appear inside them.

## Example

```text
// A wrapped type: an int that is never negative.
type quantity
  wraps int
  where (int.ge self 0)
end quantity

// A record.
type item
  field name: text
  field count: quantity
end item

// A variant. A case may have fields or none.
type stock-level
  case empty
  case low
    field remaining: int
  case plenty
end stock-level

fn level
  takes count: quantity, threshold: int
  returns stock-level
  requires (int.ge threshold 1)
  example (level (quantity.of 0) 5) = (stock-level.empty)
body
  if (int.eq count.value 0)
    return (stock-level.empty)
  else if (int.lt count.value threshold)
    return (stock-level.low remaining: count.value)
  else
    return (stock-level.plenty)
end level

// match lists every case. Inside a case, the matched name has that
// case's fields.
fn describe
  takes current: stock-level
  returns text
body
  match current
    case empty
      return "none"
    case low
      return (text.concat "only " (text.from-int current.remaining))
    case plenty
      return "plenty"
end describe

// Records and variant cases are built with every field labeled.
// In ensures, result is the returned value.
fn restock
  takes it: item, extra: int
  returns item
  requires (int.ge extra 0)
  ensures (int.ge result.count.value it.count.value)
body
  let total: int = (int.add it.count.value extra)
  return (item name: it.name count: (quantity.of total))
end restock

// try unwraps an option, or returns none from the whole function.
fn first-count
  takes items: (list item)
  returns (option int)
body
  let first: item = try (list.get items 0)
  return (option.some first.count.value)
end first-count

// Repetition is recursion. An if without else is followed by more
// statements.
fn total-count
  takes items: (list item)
  returns int
body
  if (list.is-empty items)
    return 0
  let head: item = (list.first items)
  return (int.add head.count.value (total-count (list.rest items)))
end total-count

// A nested fn can read the names around it. A function is passed by
// name.
fn bump-all
  takes items: (list item), extra: int
  returns (list item)
body
  fn bump
    takes it: item
    returns item
  body
    return (restock it extra)
  end bump
  return (list.map items bump)
end bump-all

// if can also produce a value. A function with no parameters has no
// takes line.
fn sign
  takes n: int
  returns int
body
  return (if (int.lt n 0) -1 1)
end sign

fn sample
  returns (list int)
body
  return [1, 2, 3]
end sample
```
