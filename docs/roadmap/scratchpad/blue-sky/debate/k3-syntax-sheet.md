# K3 syntax sheet: every declaration kind

Status: superseded by `k3-carryover.md` (2026-11-16). This sheet was
built from the experiment subset and omits or reinvents constructs
current Vibra already has. Kept for history; working notes, not
normative

Experiments X1 and X1b measured only `type` and `fn`, in an earlier
spelling. Everything below that goes beyond them is unmeasured. Each
part is marked:

- **measured**: the shape was in a tested card.
- **owner**: Hannah ruled on it; there is no evidence either way.
- **proposed**: new here, to be attacked.

## 1. Rules that hold everywhere

| Rule | Mark |
| --- | --- |
| A construct is an opening keyword, sections that each start with a word, and a typed closing keyword (`fn … endfn`) | measured |
| A section word is never followed by a colon. A colon only joins a pair: `name: Type` or `name: value` | measured |
| Braces hold a list of pairs that declares a shape. Braces never appear in an expression | measured |
| Parentheses apply: `f(a b)`, `Cents(amount: 5)`, `List(Int)` | measured |
| No separator commas. Items are separated by spaces or line breaks | measured, adopted by owner decision |
| Layout is not significant | measured |
| Types are PascalCase, including `Int`, `Bool`, `Str`, `List`, `Option`, `Result`, `Unit` | owner |
| Values, functions, parameters, fields, and cases are camelCase | owner preference; proposed for cases |
| Casing is a convention. `check` reports a violation; `fmt` never renames | owner; the tool split is proposed |
| Names are case-sensitive and `-` is never part of a name. Case does not decide whether a name is a type; resolution does | proposed |
| Comments start with `//` and run to the end of the line | measured |

## 2. `type`

One form covers records, variants, and what the cards called wrapped
types. A wrapped type is a record with one field and a condition.

```text
type Line
  fields {quantity: Int price: Cents}
endtype

type Cents
  fields {amount: Int}
  where Int.ge(amount 0)
endtype

type Range
  fields {low: Int high: Int}
  where Int.le(low high)
endtype

type Shape
  case circle {radius: Int}
  case rect {width: Int height: Int}
  case unit
endtype

type Pair(A B)
  fields {first: A second: B}
endtype
```

| Part | Mark | Note |
| --- | --- | --- |
| `fields {…}`, `case name {…}`, `case name` | measured | |
| `where` on a record, naming its fields directly | proposed; Codex accepts with conditions | Replaces `wraps`, `self`, `.of`, and `.value`. Rules: fields are immutable; field expressions are evaluated once, in written order, then the condition is checked before the value exists; a false, faulting, or unsupported condition rejects the construction; the check also runs on decoding, driver results, imports, and restoration. This is new checker and evaluator work, not free: the experiment harness applied `where` only to wrapped values |
| Same-shaped types stay distinct: `Meters(amount: 5)` is never a `Cents`. Reading a field is not a cast. There is no `Int(c)` | proposed | Ledger D7 |
| A capability handle is not built this way: knowing its fields mints nothing | agreed in the ledger (F4) | |
| Building: `Cents(amount: 5)`, `Shape.rect(width: 2 height: 3)`, `Shape.unit()` | measured | Every field labeled, any order |
| Reading: `c.amount` | measured | |
| Type parameters: `type Pair(A B)`, used as `Pair(Int Str)` | proposed | The declaration mirrors the use |
| `Option(T)` and `Result(T E)` are ordinary variants declared in the library: `Option.some(value: x)`, `Option.none()`, `Result.ok(value: x)`, `Result.error(error: e)` | proposed | The cards wrote `option.some(x)` with a positional argument. One constructor rule means labels here too |

Open for Hannah: whether a one-field record may be built positionally,
`Cents(5)`. Codex and Claude both prefer no: it is a second
construction rule and looks like a cast.

## 3. `fn`

```text
fn splitCents
  doc
    """
    Splits a total into equal shares. Larger shares come first.
    """
    example splitCents(Cents(amount: 100) 3) = [Cents(amount: 34) Cents(amount: 33) Cents(amount: 33)]
  enddoc
  takes {total: Cents parts: Int}
  returns shares: List(Cents)
  requires Int.ge(parts 1)
  ensures Int.eq(List.length(shares) parts)
  do
    let base: Int = Int.div(total.amount parts)
    return List.build(parts share)
endfn
```

| Part | Mark | Note |
| --- | --- | --- |
| `takes {…}`, `requires`, `ensures`, `do`, `endfn` | measured | |
| `returns name: Type` | owner; Codex wrote it as `returns {name: Type}` before seeing the ruling | The function returns one value of `Type`. The name is always given. It exists only in `ensures`: not in `requires`, not as a variable in the body, not as a field the caller sees. It may not repeat a parameter name. Every return path, including an early return by `try`, binds it and runs the same `ensures` |
| `doc … enddoc` holding one multiline string and any number of `example` lines | owner for the string; block form from Codex | The string is Markdown by convention: triple quotes, no interpolation, common indentation removed. Examples are syntax, not text inside the string |
| `example call = value` | measured as syntax | Its status changes, see section 7. `=` here is structural equality, used only in examples |
| Type parameters: `fn firstOr(T)` | proposed | Mirrors `type Pair(A B)` |
| Statements `let name: Type = e`, `if … then … else … endif`, `match … case … endmatch`, `return`, nested `fn`, `try` | measured | |

## 4. Function types

A parameter that receives a function needs a type for it.

```text
takes {items: List(Line) step: Fn(takes {total: Int line: Line} returns Int)}
```

| Part | Mark | Note |
| --- | --- | --- |
| `Fn(takes {…} returns Type)` | proposed | Reuses the two section words. Verbose. The alternative `Fn(Int Line -> Int)` is shorter and adds an arrow used nowhere else |

Open: both forms. The cards never wrote a function type, because
standard functions took callbacks and participants never declared one.

## 5. `interface` and `impl`

An interface lists operations. An operation is a `fn` or `proc`
signature with no `do`.

```text
interface Console
  proc write
    takes {line: Str}
    returns outcome: WriteOutcome
    delivery once
  endproc
  proc readLine
    returns line: Option(Str)
    delivery repeatable
  endproc
endinterface

impl Console for MemoryConsole
  mode model
  proc write
    takes {self: MemoryConsole line: Str}
    returns outcome: WriteOutcome
    do
      …
  endproc
endimpl
```

| Part | Mark | Note |
| --- | --- | --- |
| `interface Name … endinterface` holding signatures | proposed | Ledger F4: a port is an interface |
| `delivery once`, `repeatable`, or `unknown` on an effectful operation | proposed | Ledger F8: delivery metadata from gate 1 |
| `impl Interface for Type … endimpl` | proposed | Ledger F4: static dispatch, nominal handle type |
| `mode model` or `mode host` | proposed | Ledger F5: model mode is explicit. A model cannot reach real input or output |
| `self` as an ordinary first parameter | proposed | No hidden receiver |

Open: how a host driver's body is written, since it is not Vibra code;
and where `choose`, the harness-controlled choice, appears in a model.

## 6. `proc`

A `proc` is a `fn` that may perform effects. Effects go through
capability parameters, which are values of an interface type.

```text
proc main
  takes {args: List(Str) console: Console files: Files}
  returns code: ExitCode
  do
    let name: Str = try List.get(args 0)
    let text: Result(Str ReadError) = await files.read(name)
    match text
      case ok
        let sent: WriteOutcome = await console.write(text.value)
        return ExitCode.success()
      case error
        return ExitCode.failure(code: 2)
    endmatch
endproc
```

| Part | Mark | Note |
| --- | --- | --- |
| `proc … endproc` with the same sections as `fn` | proposed | Ledger F6 |
| `await` before a call that performs an effect | proposed | Marks every point where the outside world is involved |
| A capability is a parameter typed by an interface: `console: Console` | proposed | Ledger F1 and F4. The program is generic over the implementation; the entry manifest binds it |
| Calling an operation: `console.write(line)` | proposed | The same dot as a field read |
| `main`'s parameters are the program's whole authority | agreed in the ledger (F2) | |

Open: bounded parallel map and scoped concurrency are library
operations (ledger H1, H2); their names and shapes are not designed.
Whether `try` works on `Result` as it does on `Option`.

## 7. Documentation and examples

Moving examples out of the contract changes what the contract means.
Three layers are kept apart and shown separately by tooling:

| Layer | What it is | Rule |
| --- | --- | --- |
| Formal contract | Types, `requires`, `ensures` | Decides which results are permitted. Examples are not part of it |
| Protected expectations | Each `example`: an input and its expected result | Must pass before a changed body is accepted. Failed, unrun, unsupported, and exhausted are not passed |
| Evidence | The record of what ran | Sampled evidence about one body, tool, and budget |

| Part | Mark | Note |
| --- | --- | --- |
| `doc` is a section of any declaration, placed before `do` | owner | |
| An `example` no longer narrows the contract, and still constrains every accepted body | proposed; Codex accepts on this condition | In the reading test, contract c4 allowed 5 or 10 for one input and its example fixed 5. Under this rule the contract allows both, and a body returning 10 is still rejected by the example |
| `doc` and `example` sit in the protected public part. Edits are classified as contract, documentation or example, or body | proposed | Ledger B2 and B3. A hash over the public part does not make every hashed item an assumption for a prover |
| Examples run bounded and isolated, with no real effects, and never on an ordinary call | proposed | |

The reading test X6 is not rescored: its key was right for the rule
its readers were given.

## 8. What is still undesigned

1. Names and shapes of the standard library beyond what the cards used.
2. The entry manifest and the target data file.
3. Modules, imports, and visibility.
4. Text escapes and number forms beyond decimal integers.
5. Infix operators: none. Current K3 has no ambiguity. Adding infix
   later needs operator, grouping, negative-literal, and item-boundary
   rules together; dropping kebab-case removes only the `a-1` name
   reading.
