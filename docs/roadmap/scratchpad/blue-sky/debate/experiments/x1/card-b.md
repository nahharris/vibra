# Language card (surface B)

You are writing a small typed functional language. Follow this card
exactly. Output only code.

## Shape

Blocks are in braces. `let` and `return` statements end with `;`. Line
breaks and indentation do not matter. Comments start with `//` and run
to the end of the line. Names use lowercase letters, digits, and
hyphens, such as `total-count`.

- **Types:** `int`, `bool`, `text`, `list<T>`, `option<T>`, and the
  names you declare.
- **Calls:** `name(arg, arg)`. A call with no arguments is `name()`.
- **Literals:** `42`, `-7`, `"text"`, `true`, `false`, and lists
  `[a, b, c]` or `[]`.
- **Field read:** `value.field`.
- **Statements:** `let`, `if`, `match`, `return`, and a nested `fn`.
  Every path through a function ends in `return`. Each brace block
  holds one or more statements, and `match` or `if` may appear inside
  them.

## Example

```text
// A wrapped type: an int that is never negative.
type quantity = int where int.ge(self, 0);

// A record.
type item { name: text, count: quantity }

// A variant. A case may have fields or none.
type stock-level {
  case empty,
  case low { remaining: int },
  case plenty
}

fn level(count: quantity, threshold: int) -> stock-level
  requires int.ge(threshold, 1)
  example level(quantity.of(0), 5) == stock-level.empty()
{
  if int.eq(count.value, 0) {
    return stock-level.empty();
  } else if int.lt(count.value, threshold) {
    return stock-level.low { remaining: count.value };
  } else {
    return stock-level.plenty();
  }
}

// match lists every case. Inside a case, the matched name has that
// case's fields.
fn describe(current: stock-level) -> text {
  match current {
    case empty => {
      return "none";
    }
    case low => {
      return text.concat("only ", text.from-int(current.remaining));
    }
    case plenty => {
      return "plenty";
    }
  }
}

// Records and variant cases with fields are built with braces and
// every field labeled. A case with no fields is built as a call.
// In ensures, result is the returned value.
fn restock(it: item, extra: int) -> item
  requires int.ge(extra, 0)
  ensures int.ge(result.count.value, it.count.value)
{
  let total: int = int.add(it.count.value, extra);
  return item { name: it.name, count: quantity.of(total) };
}

// try unwraps an option, or returns none from the whole function.
fn first-count(items: list<item>) -> option<int> {
  let first: item = try list.get(items, 0);
  return option.some(first.count.value);
}

// Repetition is recursion. An if without else is followed by more
// statements.
fn total-count(items: list<item>) -> int {
  if list.is-empty(items) {
    return 0;
  }
  let head: item = list.first(items);
  return int.add(head.count.value, total-count(list.rest(items)));
}

// A nested fn can read the names around it. A function is passed by
// name.
fn bump-all(items: list<item>, extra: int) -> list<item> {
  fn bump(it: item) -> item {
    return restock(it, extra);
  }
  return list.map(items, bump);
}

// if can also produce a value. 
fn sign(n: int) -> int {
  return if int.lt(n, 0) { -1 } else { 1 };
}

fn sample() -> list<int> {
  return [1, 2, 3];
}
```
