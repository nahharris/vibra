# Language card (surface P)

You are writing a small typed functional language. Follow this card
exactly. Output only code.

## Shape

Everything is a parenthesized form whose first word says what it is:
`(fn …)`, `(type …)`, `(let …)`, `(cond …)`, `(match …)`. Line breaks
and indentation do not matter. Comments start with `;` and run to the
end of the line. Names use lowercase letters, digits, and hyphens, such
as `total-count`.

- **Types:** `int`, `bool`, `text`, `(list T)`, `(option T)`, and the
  names you declare.
- **Calls:** `(name arg arg)`. A call with no arguments is `(name)`.
  Never wrap a name or a literal in parentheses.
- **Literals:** `42`, `-7`, `"text"`, `true`, `false`. A list is
  `(list a b c)`, and the empty list is `(list)`.
- **Field read:** `value.field`.
- **Statements:** `let`, `cond`, `match`, `return`, and a nested `fn`.
  Every path through a function ends in `return`. A `body`, a `cond`
  clause, and a `case` each hold one or more statements after their
  first item, and `match` or `cond` may appear inside them.

## Example

```text
; A wrapped type: an int that is never negative.
(type quantity
  (wraps int)
  (where (int.ge self 0)))

; A record.
(type item
  (field name text)
  (field count quantity))

; A variant. A case may have fields or none.
(type stock-level
  (case empty)
  (case low (field remaining int))
  (case plenty))

(fn level
  (takes (count quantity) (threshold int))
  (returns stock-level)
  (requires (int.ge threshold 1))
  (example (level (quantity.of 0) 5) (stock-level.empty))
  (body
    (cond
      ((int.eq count.value 0)
        (return (stock-level.empty)))
      ((int.lt count.value threshold)
        (return (stock-level.low remaining: count.value)))
      (else
        (return (stock-level.plenty))))))

; match lists every case. Inside a case, the matched name has that
; case's fields.
(fn describe
  (takes (current stock-level))
  (returns text)
  (body
    (match current
      (case empty
        (return "none"))
      (case low
        (return (text.concat "only " (text.from-int current.remaining))))
      (case plenty
        (return "plenty")))))

; Records and variant cases are built with every field labeled.
; In ensures, result is the returned value.
(fn restock
  (takes (it item) (extra int))
  (returns item)
  (requires (int.ge extra 0))
  (ensures (int.ge result.count.value it.count.value))
  (body
    (let total int (int.add it.count.value extra))
    (return (item name: it.name count: (quantity.of total)))))

; try unwraps an option, or returns none from the whole function.
(fn first-count
  (takes (items (list item)))
  (returns (option int))
  (body
    (let first item (try (list.get items 0)))
    (return (option.some first.count.value))))

; Repetition is recursion. A cond without else is followed by more
; statements.
(fn total-count
  (takes (items (list item)))
  (returns int)
  (body
    (cond
      ((list.is-empty items)
        (return 0)))
    (let head item (list.first items))
    (return (int.add head.count.value (total-count (list.rest items))))))

; A nested fn can read the names around it. A function is passed by
; name.
(fn bump-all
  (takes (items (list item)) (extra int))
  (returns (list item))
  (body
    (fn bump
      (takes (it item))
      (returns item)
      (body
        (return (restock it extra))))
    (return (list.map items bump))))

; (if c a b) produces a value. A function with no parameters has no
; takes form.
(fn sign
  (takes (n int))
  (returns int)
  (body
    (return (if (int.lt n 0) -1 1))))

(fn sample
  (returns (list int))
  (body
    (return (list 1 2 3))))
```
