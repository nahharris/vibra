# Model-writing pilot: registration

Frozen on 2026-11-26, before any model call. This document fixes every
choice of the pilot that the milestone
[README](../README.md#the-model-writing-pilot) describes: the question,
the arms, the tasks, the models, the schedule, the prompts, the grader,
the outcome, the decision rule, the exclusions, and the budget. From the
freeze commit it changes only by a dated amendment, written before the
runs it affects, that says what changed and why (see
[Amendments](#amendments)).

The pilot is evidence about one surface choice, on these tasks and these
models. It is not a release gate, and it says nothing about other models,
other tasks, other surface choices, or people.

## The question

Does an ordinary `let`, one with no `else`, end with `endlet`?

The specified form is `let pattern be expression endlet`
([source chapter](../../../../spec/01-source-language.md#let)). It is the
default and stays the default unless the [decision rule](#decision-rule)
favours the other arm, and even then the question only returns to the
owner. The owner's reason for the default: every form that takes labelled
sections or a variable number of parts has an explicit closer; `be` is a
labelled section of `let`, and the `else` of a `let` cannot be closed
without one; a `let` without `endlet` would be a second shape of one form.

## Arms

| Arm | Ordinary `let` | `let` with `else` |
| --- | --- | --- |
| `endlet` | `let pattern [: Type] be expression endlet`, and a module value `let name: Type be expression endlet` | `let pattern [: Type] be expression else body endlet` |
| `noEndlet` | `let pattern [: Type] be expression`, and a module value `let name: Type be expression` | the same as `endlet` |

An ordinary `let` is every `let` without `else`: a statement or a module
value. The two arms differ in nothing else: the same semantics, library,
tasks, statements, examples (rendered in each arm's `let` form), models,
schedule, grader, and outcome. The `noEndlet` arm adds one reading rule,
which its card states: an `else` written right after the expression of a
`let` begins that `let`'s fallback.

## Tasks

Ten tasks, each one module of the library unit `@pilot`. A task's project
holds its project file, one hidden test module `tests/<task>.vib`, and the
solution module `src/pilot/<task>.vib`. The reference solutions of the
eight pure tasks are those of M1's exit in
[`examples/pilot-pure/`](../../../../../examples/pilot-pure/); the
references of `reader` and `plan`, every hidden test module, and every
control are written by step 39 exactly as this registration states them.

| Id | Task | Module | What it exercises | Hidden tests | Controls |
| --- | --- | --- | --- | --- | --- |
| T1 | `packet` | `@pilot.packet` | nested patterns, a constant pattern, a list pattern with `else` | 7 | 1 |
| T2 | `pair` | `@pilot.pair` | a record parameter pattern, a labelled default, a tuple `let` | 5 | 1 |
| T3 | `positive` | `@pilot.positive` | a bare `where`, a bare `ensures`, proof by branch | 5 | 2 |
| T4 | `range` | `@pilot.range` | `where … else`, a handled constant error, `let … else` | 7 | 1 |
| T5 | `order` | `@pilot.order` | a bare `requires` discharged by the same predicate | 4 | 3 |
| T6 | `gate` | `@pilot.gate` | `requires … else`, `ensures … else`, a function value | 6 | 2 |
| T7 | `reader` | `@pilot.reader` | an optional grant, a typed operation failure | 5 | 2 |
| T8 | `plan` | `@pilot.plan` | a plan built by a `fn`, awaited twice; a `proc` guard with `else` | 6 | 2 |
| T9 | `optional` | `@pilot.optional` | a generic function, an anonymous function, a guard with `else` | 7 | 2 |
| T10 | `rows` | `@pilot.rows` | `Iter.map`, conversion to a dict, lookup | 5 | 1 |

Each task's statement is in [Prompts](#prompts), its hidden tests in
[Hidden tests](#hidden-tests), and its controls in
[Negative controls](#negative-controls). The effectful tasks take every
grant from `TestWorld` in their hidden tests.

## Models

| Key | Family | Model identifier | Setting |
| --- | --- | --- | --- |
| `haiku` | Claude | `claude-haiku-5-5` | effort `low` |
| `luna` | GPT | `gpt-6-luna` | reasoning effort `low` |

These are the cohorts of the earlier surface experiment, called the same
way: through the provider's command-line client, in an isolated
invocation with no tools, no project instructions, no memory, and one task
per fresh session, from an empty temporary working directory.

- `haiku`: the Claude Code client in print mode with JSON output, the
  model requested by the identifier above, every tool disabled, no setting
  sources, an empty strict MCP configuration, no slash commands, no
  session persistence, and the system prompt below; the prompt on standard
  input; the output cap set to 4,096 tokens through
  `CLAUDE_CODE_MAX_OUTPUT_TOKENS`. The completion is the response's
  `result` text.
- `luna`: the Codex client's `exec` command with JSON events, the model
  requested by the identifier above, ignoring user configuration and
  rules, ephemeral, with every tool and feature disabled, no web search,
  no project documents, no developer instructions, and the system prompt
  below as the model's base instructions; the prompt on standard input.
  Before every call the context is audited: exactly two messages, the
  system prompt and the prompt, and no tool. The completion is the text of
  the last agent message. The client offers no output cap, so none is
  enforced.

The system prompt of both is the one line
`You are a test participant. Output only what is asked.`

## Schedule

Five attempts per task, arm, and model: 10 × 2 × 2 × 5 = 200 runs. Each
model's 100 runs form one cohort, run in this order: attempt 1 to 5; within
an attempt, the tasks in the order T1 to T10; within a task, `endlet` then
`noEndlet` for an odd attempt and `noEndlet` then `endlet` for an even one.
A run's identifier is `<model>-<task>-<arm>-<attempt>`, as in
`haiku-packet-endlet-1`. A cohort makes one call at a time, in its order;
the two cohorts may run at the same time. The schedule is written before
the first call and never reordered.

## Prompts

Every run's prompt is the arm's card, one empty line, the library, one
empty line, and the task's statement: the bytes of the three parts joined
with one LF between them. Each part is UTF-8 with LF line ends and ends
with one LF. A part below is the text between the line `~~~~text` after
its heading and the next line `~~~~`. Nothing else is sent: no examples
from other tasks, no hidden test, no earlier output, no feedback.

### Card, `endlet` arm

~~~~text
# Vibra language card

You write one module of Vibra, a small typed language. Follow this card
exactly. Reply with the complete source of the module and nothing else.

## Shape

Every construct opens with a keyword and, when it holds other constructs,
ends with a closer named after its opener: `fn ... endfn`, `proc ...
endproc`, `type ... endtype`, `if ... endif`, `match ... endmatch`,
`do ... enddo`. Inside a declaration, a section is a keyword followed by
its operand: `takes`, `labelled`, `returns`, `requires`, `ensures`,
`where`, `performs`, `visibility`, and `do`, which comes last and holds
the body.

There are no commas, no semicolons, no operators, no `=`, and no
parentheses for grouping. Whitespace separates tokens; line breaks and
indentation never change meaning. `//` starts a comment that runs to the
end of the line.

| Token | Only use |
| --- | --- |
| `( )` | application: `f(a b)`, `Item(name: "x" count: 1u64)`, `List(I32)` |
| `{ }` | tuples, records, parameter lists, effect rows |
| `[ ]` | lists and dicts |
| `:` | joins a name to its type or value |

Names are ASCII letters and digits. Types are written in PascalCase and
every other name in camelCase. No declaration or binding may reuse a name
that is already visible: there is no shadowing.

## Values and types

- Integer literals carry their type: `7i32`, `-3i32`, `0u64`. The types
  are `I8` to `I64` and `U8` to `U64`. `I32.add`, `I32.sub`, and
  `I32.mul` (and the same members of every integer type) saturate at the
  type's bounds.
- `"text"` is a `Str`. `true` and `false` are the `Bool` values. `void`
  is the one value of `Void`.
- `Option(T)` is `Option.some(value: x)` or `Option.none()`.
  `Result(T E)` is `Result.ok(value: x)` or `Result.err(error: e)`.
- A tuple `{1i32 "a"}` has the type `{I32 Str}`. A record
  `{name: "a" count: 1u64}` has the type `{name: Str count: U64}`.
- A list `[1i32 2i32]` has the type `List(I32)`; a dict `["a": 1i32]` has
  the type `Dict(Str I32)`. `[]` and `[:]` are empty and need an expected
  type.
- `value.field` reads a field of a record. A list or dict applied to a key
  is a lookup and returns an `Option`: `items(0u64)`, `table("a")`.

## Declarations

A module is a sequence of `use`, `type`, `fn`, `proc`, and module `let`
forms, with every `use` first. A declaration is private to its module
unless it has `visibility @public`. Declarations may refer to each other in
any order. `use @std.fs.DirReadText enduse` imports one declaration under
its last name.

- `type Celsius of I32 endtype` declares a wrapper, built
  `Celsius(21i32)`.
- `type Item of {name: Str count: U64} endtype` declares a record type,
  built `Item(name: "a" count: 1u64)`.
- A variant type lists its cases, each optionally with fields:
  `type Stock case empty case low {remaining: U64} endtype`, built
  `Stock.empty()` and `Stock.low(remaining: 2u64)`.

Construction uses labels exactly when the shape has field names.

A `fn` is a pure function. `takes {pattern: Type ...}` lists its
positional parameters; a plain name is the simplest pattern.
`labelled {name: Type be constant}` lists optional parameters with their
defaults; a call passes one as `name: value`. `returns name: Type` gives
the result type and a result name, which only `ensures` can see. Every
path through a body ends in `return`; a `Void` function writes
`return void`. A function is generic over the type parameters written
after its name, `fn first(T)`, and a call never writes type arguments.

An anonymous function is an expression and can use the names around it:
`fn takes {x: I32} returns y: I32 do return I32.add(x step) endfn`. Its
type is `Fn({I32} I32)`. A named function is a value by its name.

There are no loops and no assignment: repeat with recursion or with the
`Iter` functions. There is no method syntax: a call names the owner, as in
`Str.length(text)`, never `text.length()`.

A module value is `let name: Type be expression endlet`. Its type is
written, its expression is pure, and when that expression is a constant the
name can be used as a constant pattern.

## Statements

A body is a sequence of statements; a statement is a `let` or an
expression. An expression statement must have the type `Void`, or never
complete, like `return`. To discard any other value, bind it to `_`:
`let _ be f(x) endlet`.

### let

`let pattern be expression endlet` evaluates the expression once and binds
the names of the pattern. The names are visible after `endlet` until the
end of the enclosing body. A written type fixes the expected type, as in
`let count: U64 be 0u64 endlet`.

Without `else`, the pattern must always match. With `else`, the pattern
may fail to match: then the fallback runs, and every path through it must
leave the body, usually by `return`.

```vibra
let total be I32.add(a b) endlet
let {low high} be pair endlet
let Option.some(value: head) be items(0u64) else
  return 0i32
endlet
```

### if, match, do

```vibra
if Ordered.less(a b) then
  return a
else
  return b
endif
```

`if`, `match`, and `do ... enddo` are expressions: the value is that of
the last expression of the branch that ran. When the value is used, an `if`
needs an `else` and every branch must have the same type. `match subject
case pattern body ... endmatch` runs the first arm whose pattern matches,
and its arms must cover every value.

### Patterns

`_` matches anything and binds nothing. A plain name binds the value,
except a name of a module value whose expression is a constant, which
matches that value. Literals match themselves. `Owner.case(field: p)` and
`Item(name: p)` match by field name; omitted fields are ignored.
`Celsius(p)` matches a wrapper. `{p q}` matches a tuple and `{name: p}` a
record. `[p q]` matches a list of exactly two items. A pattern binds each
name at most once.

### return and try

`return expression` leaves the enclosing function. `try expression` gives
the payload of `Option.some` or `Result.ok`; on `none` or `err` it returns
that `none` or `err` from the enclosing function, whose result must be an
`Option`, or a `Result` with the same error type.

## Comparisons and Booleans

`Equatable.equal(a b)`, `Ordered.less(a b)`, `Ordered.lessOrEqual(a b)`,
`Ordered.greater(a b)`, and `Ordered.greaterOrEqual(a b)` return a `Bool`.
`Bool.not(x)`, `Bool.and(x y)`, and `Bool.or(x y)` evaluate every operand.

## Guards

`requires C` states a condition on the parameters, `ensures C` a condition
on the result name, and `where p` on a type a condition on its
representation, where `p` is a function of the representation that returns
a `Bool`.

A guard without `else` is a proof the checker must find at compile time;
nothing is checked when the program runs. A `requires` is proved at each
call, an `ensures` at each `return`, and a `where` at each construction.
The checker proves a condition only from: constants; the `where` of a value
of that type; being inside the `if` branch or `match` arm that tested the
same function on the same values; renaming and argument substitution;
`Bool.and`; simple integer comparison facts; and the `ensures` of a called
function whose `requires` was proved. A condition it refutes is
`@contract.refuted` and one it cannot prove is `@contract.unproved`; both
reject the program.

A guard with `else` states what happens when it is false.
`requires C else v`: when `C` is false, `v` is returned and the body is
skipped. `ensures C else v`: when the returned value fails `C`, `v` is
returned instead. `where p else e`: construction returns
`Result.err(error: e)` when `p` fails, and the constructor's result type is
a `Result` of the type and the type of `e`, which must be a constant.

A condition may use only parameters, constants, constructions, field reads,
`if` and `match`, comparisons, `Bool` operations, and calls of functions
that are not recursive and are built the same way. A function whose
`requires` has no `else` can be used as a function value only where its
parameter types prove the condition.

## Procedures, plans, and grants

A `proc` is a procedure that may perform operations. Its section
`performs {Name ...}` lists the operation interfaces it may use, directly
or through what it awaits. A `fn` performs nothing.

Applying a `proc` builds a plan and runs nothing. `await plan` runs it and
is allowed only inside a `proc`. Each `await` runs the plan again. A plan
has the type `Intent(R {Name ...})` and a procedure value the type
`Proc({Params} R {Name ...})`. A `fn` may build and return a plan but
cannot run it.

Authority to perform an operation is a grant value, such as a
`DirReadText`, which a procedure receives as a parameter; a program cannot
create one. `Option(DirReadText)` is an optional grant: `Option.none()`
means the authority is absent. An operation is a member applied to its
grant, `await DirReadText.read(reader at)`, and its failures are `Result`
values.

## Example

```vibra
use @std.fs.DirReadText enduse

use @std.fs.FsError enduse

use @std.fs.RelativePath enduse

type Percent of I32
  where inBounds
  visibility @public
endtype

type Item of {name: Str count: U64}
  visibility @public
endtype

type Stock
  case empty
  case low {remaining: U64}
  case plenty
  visibility @public
endtype

type StockError
  case noThreshold
  visibility @public
endtype

let reorderLevel: U64 be 1u64 endlet

fn inBounds
  takes {value: I32}
  returns ok: Bool
  do
    return Bool.and(
      Ordered.greaterOrEqual(value 0i32)
      Ordered.lessOrEqual(value 100i32)
    )
endfn

// The branch on the same function proves the invariant of Percent.
fn toPercent
  takes {raw: I32}
  returns made: Option(Percent)
  visibility @public
  do
    if inBounds(raw) then return Option.some(value: Percent(raw)) endif
    return Option.none()
endfn

// A zero threshold returns the error and skips the body.
fn level
  takes {item: Item threshold: U64}
  returns stock: Result(Stock StockError)
  requires Ordered.greater(threshold 0u64)
    else Result.err(error: StockError.noThreshold())
  visibility @public
  do
    let Item(name: _ count: count) be item endlet
    if Equatable.equal(count 0u64) then
      return Result.ok(value: Stock.empty())
    endif
    if Ordered.less(count threshold) then
      return Result.ok(value: Stock.low(remaining: count))
    endif
    return Result.ok(value: Stock.plenty())
endfn

// reorderLevel is a constant pattern: it matches 1u64 and binds nothing.
fn describe
  takes {stock: Stock}
  returns text: Str
  visibility @public
  do
    return match stock
      case Stock.empty()
        "none"
      case Stock.low(remaining: reorderLevel)
        "reorder now"
      case Stock.low(remaining: remaining)
        Str.concat("only " From.convert(remaining))
      case Stock.plenty()
        "plenty"
    endmatch
endfn

fn firstCount
  takes {items: List(Item)}
  returns count: Option(U64)
  visibility @public
  do
    let first be try items(0u64) endlet
    return Option.some(value: first.count)
endfn

fn total
  takes {items: List(Item)}
  returns sum: U64
  visibility @public
  do
    return Iter.fold(
      items
      0u64
      fn
        takes {gathered: U64 item: Item}
        returns next: U64
        do
          return U64.add(gathered item.count)
      endfn
    )
endfn

// Called as spread({1i32 5i32}) or spread({1i32 5i32} floor: 2i32).
fn spread
  takes {{low high}: {I32 I32}}
  labelled {floor: I32 be 0i32}
  returns width: I32
  visibility @public
  do
    let gap be I32.sub(high low) endlet
    return if Ordered.less(gap floor) then floor else gap endif
endfn

proc joined
  takes {reader: DirReadText first: RelativePath second: RelativePath}
  returns text: Result(Str FsError)
  performs {DirReadText}
  visibility @public
  do
    let head be try await DirReadText.read(reader first) endlet
    let tail be try await DirReadText.read(reader second) endlet
    return Result.ok(value: Str.concat(head tail))
endproc

// Builds the plan of a read; the caller decides when to run it.
fn later
  takes {reader: DirReadText at: RelativePath}
  returns plan: Intent(Result(Str FsError) {DirReadText})
  visibility @public
  do
    return DirReadText.read(reader at)
endfn
```
~~~~

### Card, `noEndlet` arm

~~~~text
# Vibra language card

You write one module of Vibra, a small typed language. Follow this card
exactly. Reply with the complete source of the module and nothing else.

## Shape

Every construct opens with a keyword and, when it holds other constructs,
ends with a closer named after its opener: `fn ... endfn`, `proc ...
endproc`, `type ... endtype`, `if ... endif`, `match ... endmatch`,
`do ... enddo`. Inside a declaration, a section is a keyword followed by
its operand: `takes`, `labelled`, `returns`, `requires`, `ensures`,
`where`, `performs`, `visibility`, and `do`, which comes last and holds
the body.

There are no commas, no semicolons, no operators, no `=`, and no
parentheses for grouping. Whitespace separates tokens; line breaks and
indentation never change meaning. `//` starts a comment that runs to the
end of the line.

| Token | Only use |
| --- | --- |
| `( )` | application: `f(a b)`, `Item(name: "x" count: 1u64)`, `List(I32)` |
| `{ }` | tuples, records, parameter lists, effect rows |
| `[ ]` | lists and dicts |
| `:` | joins a name to its type or value |

Names are ASCII letters and digits. Types are written in PascalCase and
every other name in camelCase. No declaration or binding may reuse a name
that is already visible: there is no shadowing.

## Values and types

- Integer literals carry their type: `7i32`, `-3i32`, `0u64`. The types
  are `I8` to `I64` and `U8` to `U64`. `I32.add`, `I32.sub`, and
  `I32.mul` (and the same members of every integer type) saturate at the
  type's bounds.
- `"text"` is a `Str`. `true` and `false` are the `Bool` values. `void`
  is the one value of `Void`.
- `Option(T)` is `Option.some(value: x)` or `Option.none()`.
  `Result(T E)` is `Result.ok(value: x)` or `Result.err(error: e)`.
- A tuple `{1i32 "a"}` has the type `{I32 Str}`. A record
  `{name: "a" count: 1u64}` has the type `{name: Str count: U64}`.
- A list `[1i32 2i32]` has the type `List(I32)`; a dict `["a": 1i32]` has
  the type `Dict(Str I32)`. `[]` and `[:]` are empty and need an expected
  type.
- `value.field` reads a field of a record. A list or dict applied to a key
  is a lookup and returns an `Option`: `items(0u64)`, `table("a")`.

## Declarations

A module is a sequence of `use`, `type`, `fn`, `proc`, and module `let`
forms, with every `use` first. A declaration is private to its module
unless it has `visibility @public`. Declarations may refer to each other in
any order. `use @std.fs.DirReadText enduse` imports one declaration under
its last name.

- `type Celsius of I32 endtype` declares a wrapper, built
  `Celsius(21i32)`.
- `type Item of {name: Str count: U64} endtype` declares a record type,
  built `Item(name: "a" count: 1u64)`.
- A variant type lists its cases, each optionally with fields:
  `type Stock case empty case low {remaining: U64} endtype`, built
  `Stock.empty()` and `Stock.low(remaining: 2u64)`.

Construction uses labels exactly when the shape has field names.

A `fn` is a pure function. `takes {pattern: Type ...}` lists its
positional parameters; a plain name is the simplest pattern.
`labelled {name: Type be constant}` lists optional parameters with their
defaults; a call passes one as `name: value`. `returns name: Type` gives
the result type and a result name, which only `ensures` can see. Every
path through a body ends in `return`; a `Void` function writes
`return void`. A function is generic over the type parameters written
after its name, `fn first(T)`, and a call never writes type arguments.

An anonymous function is an expression and can use the names around it:
`fn takes {x: I32} returns y: I32 do return I32.add(x step) endfn`. Its
type is `Fn({I32} I32)`. A named function is a value by its name.

There are no loops and no assignment: repeat with recursion or with the
`Iter` functions. There is no method syntax: a call names the owner, as in
`Str.length(text)`, never `text.length()`.

A module value is `let name: Type be expression`. Its type is written, its
expression is pure, and when that expression is a constant the name can be
used as a constant pattern.

## Statements

A body is a sequence of statements; a statement is a `let` or an
expression. An expression statement must have the type `Void`, or never
complete, like `return`. To discard any other value, bind it to `_`:
`let _ be f(x)`.

### let

`let pattern be expression` evaluates the expression once and binds the
names of the pattern. An ordinary `let` has no closer: its expression ends
where the next statement begins. The names are visible after the
expression until the end of the enclosing body. A written type fixes the
expected type, as in `let count: U64 be 0u64`.

Without `else`, the pattern must always match. With `else`, the pattern
may fail to match: then the fallback runs, and every path through it must
leave the body, usually by `return`. A `let` with `else` ends with
`endlet`, and an `else` written right after the expression of a `let`
always begins that `let`'s fallback.

```vibra
let total be I32.add(a b)
let {low high} be pair
let Option.some(value: head) be items(0u64) else
  return 0i32
endlet
```

### if, match, do

```vibra
if Ordered.less(a b) then
  return a
else
  return b
endif
```

`if`, `match`, and `do ... enddo` are expressions: the value is that of
the last expression of the branch that ran. When the value is used, an `if`
needs an `else` and every branch must have the same type. `match subject
case pattern body ... endmatch` runs the first arm whose pattern matches,
and its arms must cover every value.

### Patterns

`_` matches anything and binds nothing. A plain name binds the value,
except a name of a module value whose expression is a constant, which
matches that value. Literals match themselves. `Owner.case(field: p)` and
`Item(name: p)` match by field name; omitted fields are ignored.
`Celsius(p)` matches a wrapper. `{p q}` matches a tuple and `{name: p}` a
record. `[p q]` matches a list of exactly two items. A pattern binds each
name at most once.

### return and try

`return expression` leaves the enclosing function. `try expression` gives
the payload of `Option.some` or `Result.ok`; on `none` or `err` it returns
that `none` or `err` from the enclosing function, whose result must be an
`Option`, or a `Result` with the same error type.

## Comparisons and Booleans

`Equatable.equal(a b)`, `Ordered.less(a b)`, `Ordered.lessOrEqual(a b)`,
`Ordered.greater(a b)`, and `Ordered.greaterOrEqual(a b)` return a `Bool`.
`Bool.not(x)`, `Bool.and(x y)`, and `Bool.or(x y)` evaluate every operand.

## Guards

`requires C` states a condition on the parameters, `ensures C` a condition
on the result name, and `where p` on a type a condition on its
representation, where `p` is a function of the representation that returns
a `Bool`.

A guard without `else` is a proof the checker must find at compile time;
nothing is checked when the program runs. A `requires` is proved at each
call, an `ensures` at each `return`, and a `where` at each construction.
The checker proves a condition only from: constants; the `where` of a value
of that type; being inside the `if` branch or `match` arm that tested the
same function on the same values; renaming and argument substitution;
`Bool.and`; simple integer comparison facts; and the `ensures` of a called
function whose `requires` was proved. A condition it refutes is
`@contract.refuted` and one it cannot prove is `@contract.unproved`; both
reject the program.

A guard with `else` states what happens when it is false.
`requires C else v`: when `C` is false, `v` is returned and the body is
skipped. `ensures C else v`: when the returned value fails `C`, `v` is
returned instead. `where p else e`: construction returns
`Result.err(error: e)` when `p` fails, and the constructor's result type is
a `Result` of the type and the type of `e`, which must be a constant.

A condition may use only parameters, constants, constructions, field reads,
`if` and `match`, comparisons, `Bool` operations, and calls of functions
that are not recursive and are built the same way. A function whose
`requires` has no `else` can be used as a function value only where its
parameter types prove the condition.

## Procedures, plans, and grants

A `proc` is a procedure that may perform operations. Its section
`performs {Name ...}` lists the operation interfaces it may use, directly
or through what it awaits. A `fn` performs nothing.

Applying a `proc` builds a plan and runs nothing. `await plan` runs it and
is allowed only inside a `proc`. Each `await` runs the plan again. A plan
has the type `Intent(R {Name ...})` and a procedure value the type
`Proc({Params} R {Name ...})`. A `fn` may build and return a plan but
cannot run it.

Authority to perform an operation is a grant value, such as a
`DirReadText`, which a procedure receives as a parameter; a program cannot
create one. `Option(DirReadText)` is an optional grant: `Option.none()`
means the authority is absent. An operation is a member applied to its
grant, `await DirReadText.read(reader at)`, and its failures are `Result`
values.

## Example

```vibra
use @std.fs.DirReadText enduse

use @std.fs.FsError enduse

use @std.fs.RelativePath enduse

type Percent of I32
  where inBounds
  visibility @public
endtype

type Item of {name: Str count: U64}
  visibility @public
endtype

type Stock
  case empty
  case low {remaining: U64}
  case plenty
  visibility @public
endtype

type StockError
  case noThreshold
  visibility @public
endtype

let reorderLevel: U64 be 1u64

fn inBounds
  takes {value: I32}
  returns ok: Bool
  do
    return Bool.and(
      Ordered.greaterOrEqual(value 0i32)
      Ordered.lessOrEqual(value 100i32)
    )
endfn

// The branch on the same function proves the invariant of Percent.
fn toPercent
  takes {raw: I32}
  returns made: Option(Percent)
  visibility @public
  do
    if inBounds(raw) then return Option.some(value: Percent(raw)) endif
    return Option.none()
endfn

// A zero threshold returns the error and skips the body.
fn level
  takes {item: Item threshold: U64}
  returns stock: Result(Stock StockError)
  requires Ordered.greater(threshold 0u64)
    else Result.err(error: StockError.noThreshold())
  visibility @public
  do
    let Item(name: _ count: count) be item
    if Equatable.equal(count 0u64) then
      return Result.ok(value: Stock.empty())
    endif
    if Ordered.less(count threshold) then
      return Result.ok(value: Stock.low(remaining: count))
    endif
    return Result.ok(value: Stock.plenty())
endfn

// reorderLevel is a constant pattern: it matches 1u64 and binds nothing.
fn describe
  takes {stock: Stock}
  returns text: Str
  visibility @public
  do
    return match stock
      case Stock.empty()
        "none"
      case Stock.low(remaining: reorderLevel)
        "reorder now"
      case Stock.low(remaining: remaining)
        Str.concat("only " From.convert(remaining))
      case Stock.plenty()
        "plenty"
    endmatch
endfn

fn firstCount
  takes {items: List(Item)}
  returns count: Option(U64)
  visibility @public
  do
    let first be try items(0u64)
    return Option.some(value: first.count)
endfn

fn total
  takes {items: List(Item)}
  returns sum: U64
  visibility @public
  do
    return Iter.fold(
      items
      0u64
      fn
        takes {gathered: U64 item: Item}
        returns next: U64
        do
          return U64.add(gathered item.count)
      endfn
    )
endfn

// Called as spread({1i32 5i32}) or spread({1i32 5i32} floor: 2i32).
fn spread
  takes {{low high}: {I32 I32}}
  labelled {floor: I32 be 0i32}
  returns width: I32
  visibility @public
  do
    let gap be I32.sub(high low)
    return if Ordered.less(gap floor) then floor else gap endif
endfn

proc joined
  takes {reader: DirReadText first: RelativePath second: RelativePath}
  returns text: Result(Str FsError)
  performs {DirReadText}
  visibility @public
  do
    let head be try await DirReadText.read(reader first)
    let tail be try await DirReadText.read(reader second)
    return Result.ok(value: Str.concat(head tail))
endproc

// Builds the plan of a read; the caller decides when to run it.
fn later
  takes {reader: DirReadText at: RelativePath}
  returns plan: Intent(Result(Str FsError) {DirReadText})
  visibility @public
  do
    return DirReadText.read(reader at)
endfn
```
~~~~

### Library, both arms

~~~~text
# Standard library

These need no `use`: the integer types, `Bool`, `Str`, `List`, `Dict`,
`Option`, `Result`, `Ordering`, `Equatable`, `Ordered`, `Iter`, `From`,
`true`, and `false`. Everything from `@std.fs` needs a `use`.

| Function | Result |
| --- | --- |
| `I32.add(a b)`, `I32.sub(a b)`, `I32.mul(a b)` | `I32`, saturating; the same for every integer type |
| `Str.length(text)` | `U64`, the number of characters |
| `Str.concat(left right)` | `Str` |
| `List.length(items)` | `U64` |
| `List.append(items item)` | the list with `item` added at the end |
| `items(index)` with `index: U64` | `Option(T)`, the item at that position |
| `Iter.map(items f)` | an `Iter` of `f` applied to each item, in order |
| `Iter.collect(iter)` | `List` of the items |
| `Iter.fold(items initial step)` with `step: Fn({Acc Item} Acc)` | the last accumulator |
| `Iter.next(items)` | `Option.some(value: {first rest})`, the first item and the remaining items, or `Option.none()` |
| `From.convert(value)` | `value` converted to the expected type, such as `Str` from an integer, or `Dict(K V)` from a `List({K V})` |
| `Dict.entries(table)` | `List({K V})` in ascending key order |
| `table(key)` | `Option(V)` |

A `List`, an `Option`, and a `Str` can each be used where an `Iter` is
expected. A `Dict` holds one value per key: built from a list of pairs, a
later pair with an equal key replaces an earlier one.

`@std.fs` declares:

- `FsError`, a variant type with the cases `notFound`, `permissionDenied`,
  `notAFile`, `notADirectory`, `invalidPath`, `invalidText`, `other`,
  `denied`, `limitExceeded`, `unsupported`, and `outcomeUnknown`, none with
  fields. `denied` means the grant does not permit the request and
  `limitExceeded` that a limit of the grant is used up.
- `RelativePath`, a path below the directory a grant holds, built from its
  names: `RelativePath(["notes" "a.txt"])` returns
  `Result(RelativePath ScopeError)`. When every name is a constant the
  checker knows the result, so `Result.ok(value: at)` is then a pattern
  that always matches.
- The operation interface `DirReadText`, with the operation
  `DirReadText.read(grant at)`, a `proc` that takes
  `{grant: Self at: RelativePath}` and returns `Result(Str FsError)`, the
  text of that file.
- The operation interface `DirWriteText`, with the operation
  `DirWriteText.write(grant at contents)`, a `proc` that takes
  `{grant: Self at: RelativePath contents: Str}` and returns
  `Result(Void FsError)`. It creates the file or replaces its contents.
~~~~

### Statement of T1 `packet`, both arms

~~~~text
# Task: packet

Write the module `@pilot.packet`. Each declaration named below has exactly
the name, parameters, and result written here, and has
`visibility @public` unless this task says it is private. You may add
declarations of your own. Write no `test`.

- The type `Packet`, a variant type with the cases `empty` and
  `item {points: List(I32) code: I32}`.
- The private module value `stopCode`, of type `I32`, whose value is
  `99i32`.
- The function `packetScore`, which takes `{packet: Packet}` and returns
  `score: Option(I32)`.

`packetScore` returns `Option.none()` for an empty packet. For an item
whose code is `stopCode`, matched by using `stopCode` as a constant
pattern, it returns `Option.none()`. For any other item it needs exactly
two points, `first` and `second`; with any other number of points it
returns `Option.none()`. Otherwise it returns
`Option.some(value: I32.add(I32.add(first second) code))`.

Examples:

- `packetScore(Packet.item(points: [2i32 3i32] code: 7i32))` is
  `Option.some(value: 12i32)`.
- `packetScore(Packet.item(points: [2i32 3i32] code: 99i32))` is
  `Option.none()`.
- `packetScore(Packet.item(points: [2i32] code: 7i32))` is
  `Option.none()`.
- `packetScore(Packet.empty())` is `Option.none()`.
~~~~

### Statement of T2 `pair`, both arms

~~~~text
# Task: pair

Write the module `@pilot.pair`. Each declaration named below has exactly
the name, parameters, and result written here, and has
`visibility @public` unless this task says it is private. You may add
declarations of your own. Write no `test`.

- The function `normalizePair`, which takes one parameter of the record
  type `{low: I32 high: I32}`, whose fields it binds by a record pattern
  in `takes`, and the labelled parameter `includeDelta: Bool` with the
  default `true`, and returns
  `pair: {first: I32 second: I32 delta: Option(I32)}`.

`first` is the lesser of `low` and `high` and `second` the greater; when
they are equal both are that value. `delta` is
`Option.some(value: I32.sub(second first))` when `includeDelta` is true,
and `Option.none()` when it is false.

Examples:

- `normalizePair({low: 8i32 high: 3i32})` is
  `{first: 3i32 second: 8i32 delta: Option.some(value: 5i32)}`.
- `normalizePair({low: 8i32 high: 3i32} includeDelta: false)` is
  `{first: 3i32 second: 8i32 delta: Option.none()}`.
- `normalizePair({low: 4i32 high: 4i32})` is
  `{first: 4i32 second: 4i32 delta: Option.some(value: 0i32)}`.
~~~~

### Statement of T3 `positive`, both arms

~~~~text
# Task: positive

Write the module `@pilot.positive`. Each declaration named below has
exactly the name, parameters, and result written here, and has
`visibility @public` unless this task says it is private. You may add
declarations of your own. Write no `test`.

- The type `PositiveError`, a variant type with the one case
  `nonPositive`.
- The type `Positive`, a wrapper of `I32` with the guard
  `where isPositive`, which has no `else`.
- The function `isPositive`, which takes `{value: I32}` and returns
  `ok: Bool`: true exactly when `value` is greater than `0i32`.
- The function `makePositive`, which takes `{raw: I32}` and returns
  `made: Result(Positive PositiveError)`.
- The function `readPositive`, which takes one parameter of type
  `Positive` and returns `value: I32`, with the guard
  `ensures isPositive(value)`, which has no `else`.

`makePositive` returns `Result.ok(value: Positive(raw))` when `raw` is
positive and `Result.err(error: PositiveError.nonPositive())` otherwise.
`readPositive` returns the `I32` the `Positive` wraps.

Examples:

- `makePositive(3i32)` is `Result.ok(value: Positive(3i32))`.
- `makePositive(0i32)` is `Result.err(error: PositiveError.nonPositive())`.
- `readPositive(Positive(3i32))` is `3i32`.
~~~~

### Statement of T4 `range`, both arms

~~~~text
# Task: range

Write the module `@pilot.range`. Each declaration named below has exactly
the name, parameters, and result written here, and has
`visibility @public` unless this task says it is private. You may add
declarations of your own. Write no `test`.

- The type `RangeError`, a variant type with the one case `invalid`.
- The type `Range`, a record type `{low: I32 high: I32}` with the guard
  `where orderedRange else RangeError.invalid()`.
- The private function `orderedRange`, which takes
  `{bounds: {low: I32 high: I32}}` and returns `ok: Bool`: true exactly
  when `low` is less than or equal to `high`.
- The function `makeRange`, which takes `{low: I32 high: I32}` and returns
  `made: Result(Range RangeError)`: the construction of a `Range` from
  `low` and `high`.
- The function `spanOr`, which takes `{low: I32 high: I32 fallback: I32}`
  and returns `width: I32`.
- The private module value `reversed`, of type `Result(Range RangeError)`,
  whose value is the construction of a `Range` with `low` `5i32` and
  `high` `2i32`.
- The function `reversedSpanOr`, which takes `{fallback: I32}` and returns
  `width: I32`.

The span of a range is `I32.sub(high low)`. `spanOr` returns the span of
the range `makeRange(low high)` makes, or `fallback` when it makes an
error. `reversedSpanOr` returns the span of the range in `reversed`, or
`fallback` when `reversed` holds an error.

Examples:

- `spanOr(2i32 5i32 -7i32)` is `3i32`.
- `spanOr(5i32 2i32 -7i32)` is `-7i32`.
- `reversedSpanOr(-7i32)` is `-7i32`.
- `makeRange(5i32 2i32)` is `Result.err(error: RangeError.invalid())`.
~~~~

### Statement of T5 `order`, both arms

~~~~text
# Task: order

Write the module `@pilot.order`. Each declaration named below has exactly
the name, parameters, and result written here, and has
`visibility @public` unless this task says it is private. You may add
declarations of your own. Write no `test`.

- The type `OrderError`, a variant type with the one case `reversed`.
- The function `inOrder`, which takes `{low: I32 high: I32}` and returns
  `ok: Bool`: true exactly when `low` is less than or equal to `high`.
- The function `chooseUpper`, which takes `{low: I32 high: I32}` and
  returns `upper: I32`, with the guards `requires inOrder(low high)` and
  `ensures inOrder(low upper)`, neither of which has an `else`. It returns
  `high`.
- The function `orderedChoice`, which takes `{low: I32 high: I32}` and
  returns `choice: Result(I32 OrderError)`.

`orderedChoice` returns `Result.ok(value: chooseUpper(low high))` when
`low` and `high` are in order and
`Result.err(error: OrderError.reversed())` otherwise.

Examples:

- `orderedChoice(2i32 5i32)` is `Result.ok(value: 5i32)`.
- `orderedChoice(5i32 2i32)` is
  `Result.err(error: OrderError.reversed())`.
- `orderedChoice(4i32 4i32)` is `Result.ok(value: 4i32)`.
~~~~

### Statement of T6 `gate`, both arms

~~~~text
# Task: gate

Write the module `@pilot.gate`. Each declaration named below has exactly
the name, parameters, and result written here, and has
`visibility @public` unless this task says it is private. You may add
declarations of your own. Write no `test`.

- The type `GateError`, a variant type with the cases `disabled` and
  `nonPositive`.
- The private function `positiveOk`, which takes
  `{outcome: Result(I32 GateError)}` and returns `ok: Bool`: for
  `Result.ok(value: amount)` whether `amount` is greater than `0i32`, and
  true for every error.
- The function `gateAmount`, which takes `{enabled: Bool amount: I32}` and
  returns `outcome: Result(I32 GateError)`, with these two guards, and
  whose body returns `Result.ok(value: amount)`:

  ```vibra
  requires enabled else Result.err(error: GateError.disabled())
  ensures positiveOk(outcome) else Result.err(error: GateError.nonPositive())
  ```

- The function `viaGate`, which takes
  `{gate: Fn({Bool I32} Result(I32 GateError)) enabled: Bool amount: I32}`
  and returns `outcome: Result(I32 GateError)`: the result of applying
  `gate` to `enabled` and `amount`.

Examples:

- `viaGate(gateAmount false 4i32)` is
  `Result.err(error: GateError.disabled())`.
- `viaGate(gateAmount true 4i32)` is `Result.ok(value: 4i32)`.
- `viaGate(gateAmount true -4i32)` is
  `Result.err(error: GateError.nonPositive())`.
~~~~

### Statement of T7 `reader`, both arms

~~~~text
# Task: reader

Write the module `@pilot.reader`. Each declaration named below has
exactly the name, parameters, and result written here, and has
`visibility @public` unless this task says it is private. You may add
declarations of your own. Write no `test`.

- The procedure `readOptional`, which takes
  `{reader: Option(DirReadText) at: RelativePath fallback: Str}`, returns
  `text: Result(Str FsError)`, and performs `{DirReadText}`.

When `reader` is `Option.none()`, `readOptional` returns
`Result.ok(value: fallback)` and performs no operation. When it holds a
grant, `readOptional` reads the file `at` through that grant and returns
the result of the read as it is: an error of the read is returned, never
the fallback.

Examples:

- With no reader, `readOptional` returns `Result.ok(value: fallback)`.
- With a reader whose directory holds the file `a.txt` with the text
  `hello`, reading `a.txt` returns `Result.ok(value: "hello")`.
- With a reader whose directory holds no `a.txt`, reading `a.txt` returns
  `Result.err(error: FsError.notFound())`.
~~~~

### Statement of T8 `plan`, both arms

~~~~text
# Task: plan

Write the module `@pilot.plan`. Each declaration named below has exactly
the name, parameters, and result written here, and has
`visibility @public` unless this task says it is private. You may add
declarations of your own. Write no `test`.

- The procedure `emit`, which takes
  `{enabled: Bool writer: DirWriteText at: RelativePath contents: Str}`,
  returns `outcome: Result(Void FsError)`, has the guard
  `requires enabled else Result.ok(value: void)`, and performs
  `{DirWriteText}`. Its body writes `contents` to the file `at` through
  `writer` and returns the result of the write.
- The function `prepare`, which takes the same parameters as `emit` and
  returns `plan: Intent(Result(Void FsError) {DirWriteText})`: the plan of
  `emit` applied to its parameters.
- The procedure `runTwice`, which takes the same parameters as `emit`,
  returns `count: Result(U32 FsError)`, and performs `{DirWriteText}`.

`prepare` runs nothing. `runTwice` builds the plan once with `prepare` and
runs that one plan twice, in order. When the first run returns an error,
`runTwice` returns that error and does not run the plan again; when the
second run returns an error, it returns that error. Otherwise it returns
`Result.ok(value: 2u32)`.

Examples:

- With `enabled` true and a writer whose directory permits the write,
  `runTwice` returns `Result.ok(value: 2u32)` and the file holds
  `contents`.
- With `enabled` false, `runTwice` returns `Result.ok(value: 2u32)` and
  writes nothing.
- With `enabled` true and a writer that permits only one write, `runTwice`
  returns `Result.err(error: FsError.limitExceeded())` and the file holds
  `contents` from the first run.
~~~~

### Statement of T9 `optional`, both arms

~~~~text
# Task: optional

Write the module `@pilot.optional`. Each declaration named below has
exactly the name, parameters, and result written here, and has
`visibility @public` unless this task says it is private. You may add
declarations of your own. Write no `test`.

- The generic function `mapOptional`, with the type parameters `A` and
  `B`, which takes `{item: Option(A) f: Fn({A} B)}` and returns
  `mapped: Option(B)`.
- The function `clampPositive`, which takes `{value: I32}` and returns
  `clamped: I32`, with the guard
  `requires Ordered.greater(value 0i32) else 0i32`. Its body returns
  `value`.
- The function `adjust`, which takes `{item: Option(I32) offset: I32}` and
  returns `adjusted: Option(I32)`.

`mapOptional` returns `Option.none()` for `Option.none()`, and
`Option.some(value: f(x))` for `Option.some(value: x)`, applying `f` once.
`adjust` applies `mapOptional` to `item` and an anonymous function that
returns `I32.add(clampPositive(value) offset)` for its parameter `value`.

Examples:

- `adjust(Option.none() 2i32)` is `Option.none()`.
- `adjust(Option.some(value: 3i32) 2i32)` is `Option.some(value: 5i32)`.
- `adjust(Option.some(value: -3i32) 2i32)` is `Option.some(value: 2i32)`.
~~~~

### Statement of T10 `rows`, both arms

~~~~text
# Task: rows

Write the module `@pilot.rows`. Each declaration named below has exactly
the name, parameters, and result written here, and has
`visibility @public` unless this task says it is private. You may add
declarations of your own. Write no `test`.

- The type `Row`, a record type `{key: Str amount: I32}`.
- The function `indexRows`, which takes `{rows: List(Row)}` and returns
  `index: Dict(Str I32)`.
- The function `latest`, which takes `{rows: List(Row) key: Str}` and
  returns `amount: Option(I32)`.

`indexRows` maps each row to the pair `{key amount}` with `Iter.map`,
collects the pairs, and converts them to the dict with `From.convert`.
When two rows have an equal key, the later row's amount is kept.
`latest` looks `key` up in the dict `indexRows(rows)` makes.

Examples:

- For the rows `Row(key: "b" amount: 1i32)`, `Row(key: "a" amount: 2i32)`,
  and `Row(key: "b" amount: 3i32)`, in that order, `Dict.entries` of their
  `indexRows` is `[{"a" 2i32} {"b" 3i32}]`.
- `latest([Row(key: "b" amount: 1i32) Row(key: "b" amount: 3i32)] "b")`
  is `Option.some(value: 3i32)`.
- `latest([] "a")` is `Option.none()`.
~~~~

### Digests

SHA-256 of the UTF-8 bytes, in lowercase hexadecimal.

| Part | Bytes | SHA-256 |
| --- | --- | --- |
| Card, `endlet` arm | 12171 | `029ef80832a110e7dc38d88a3eced18396dd369e9a382b389c58063b835cbb6c` |
| Card, `noEndlet` arm | 12316 | `f9fa18d467802418ed631d169a49125dd368359f3982833752ae314cc13c1e3d` |
| Library, both arms | 2526 | `8a0ee393b367009defafac9c755be88cc1dcb8712393476a8a823b869ec6e18d` |
| Statement of T1 `packet`, both arms | 1248 | `0cf1a04f5fd9170767e8d68a12e94690fc88ca9f0c0c5e8f255c8c06ecbeb4c2` |
| Statement of T2 `pair`, both arms | 1137 | `3d02c3a532887a8d4c38f954c029be9cf4f120814aaba308a7fc9a1d68ad9dab` |
| Statement of T3 `positive`, both arms | 1249 | `53f6ef5b1a38db2d2ba61cb1cffee30c09de3714fe01f8437839f7f1f0760a95` |
| Statement of T4 `range`, both arms | 1597 | `9f9d86a6a4f64ea70cadde076c2258cecffea29af658b2526f7a1e56336cca5e` |
| Statement of T5 `order`, both arms | 1180 | `57a6f2ffd00fbf37a2d40f9d21837f5a60f32175b8aaf85bcef6af1339d17580` |
| Statement of T6 `gate`, both arms | 1374 | `64cc12c3cd450a5fa4289b6ae56f47b9737b5df364066122cfe1b1066b4dfcea` |
| Statement of T7 `reader`, both arms | 1062 | `1efff3a855cd2df83472482ed7928950b24059b2499e15d009da79b5e5664220` |
| Statement of T8 `plan`, both arms | 1683 | `635ac7426a1a92c9a87a9b8d608aa32503c5220482923526ca2e6826e95374d7` |
| Statement of T9 `optional`, both arms | 1211 | `f626bd5552c3443f1f704ee8a38fc547d6f02c96c78bc9e3c2a0f494ed1c3322` |
| Statement of T10 `rows`, both arms | 1133 | `85453931fc302a263d2a12f9b9b69cffb9db8bf9adfc91f915664d8971c6c25c` |

| Task | Arm | Bytes | SHA-256 of the prompt |
| --- | --- | --- | --- |
| T1 `packet` | `endlet` | 15947 | `3f475c629dddccf8530458ad0f9228ac5e3fb73bf2400bc63539c873d28d68d9` |
| T1 `packet` | `noEndlet` | 16092 | `d7b00ff704bc3e9fdc0d53279e3fc72f21c8cf7302310ad6dd73f65b41a1f185` |
| T2 `pair` | `endlet` | 15836 | `2dba4a585cc0514d9fcf7a34c74f02ba1ad3784636a0374f9c46a599581e2d01` |
| T2 `pair` | `noEndlet` | 15981 | `03e3d565af140443633283eb477a55de1648c968246c76d16ba8b8f5f767ef50` |
| T3 `positive` | `endlet` | 15948 | `c67ca5a84c2487cd9ed6ae4c4dece60e97a33e09737539e22ed8b591831f1762` |
| T3 `positive` | `noEndlet` | 16093 | `3ccb6672ff7f7b4b22062cbb93841f2b80465f98aebf9048d902c26d6c60dcad` |
| T4 `range` | `endlet` | 16296 | `322c8110a40145b5473753745cff42ab076d6d1983c5d674b22cf9f31fa78b6d` |
| T4 `range` | `noEndlet` | 16441 | `b215ce6e36652c4b77e4aeb232fa9f2bdb5d3a53b81577790e9ba7e3303c26bc` |
| T5 `order` | `endlet` | 15879 | `567e23783ca87f5ee97ef6ef9adc0e6f4f90c6fcc9d8046c901e5710b439f81e` |
| T5 `order` | `noEndlet` | 16024 | `4a8a465ea1926d23b25d312905d830b85c9d60c2411c906f9f4538800e8a0e0e` |
| T6 `gate` | `endlet` | 16073 | `8e20e4e757bd447852943d33216efebe2e7e575d1aa89ba154366268495d2833` |
| T6 `gate` | `noEndlet` | 16218 | `eaeb0dc0c30101dda4d4f674e4a39df0c460399798a7a40c633eaf682d3ef4d1` |
| T7 `reader` | `endlet` | 15761 | `2a0f97b5e657d16ed72a28488478daae4d9db5a08338a58a8d04d9ec26066b2f` |
| T7 `reader` | `noEndlet` | 15906 | `56ff4144b7b1eb8537b3a8da064d549ca3b2d238eaf29f6b3b3ef895843f9a73` |
| T8 `plan` | `endlet` | 16382 | `fac777d00c40a23fbf9e750ea6de979f31fe59f774c48a8e065ee67745b05c75` |
| T8 `plan` | `noEndlet` | 16527 | `ef83e9f3e55d617004678bb051e328ff2bca97a383e5f9df0c67a1c0cf97a98d` |
| T9 `optional` | `endlet` | 15910 | `371f603cd88e4317da9ae592b0f6fc5322025c81c05dccb2786fc2f833198ea1` |
| T9 `optional` | `noEndlet` | 16055 | `9143a138aeec3b7461602881822ce745a51d344b6acbcf3aa5be2572cfdcd48c` |
| T10 `rows` | `endlet` | 15832 | `4c5d0a7d326f7fe41a33b44adc86ee44016e1399f1d106bc474725a6553b850a` |
| T10 `rows` | `noEndlet` | 15977 | `ce93530a9ac58b33260edcd356317fd5b1af99bef91cb1b67abe8c054348d1a1` |

## Extraction

The solution of a run is taken from its completion text. If the text has a
line that begins with three backticks, the solution is the lines after the
first such line, up to the next line that begins with three backticks or
the end of the text; otherwise it is the whole text. The bytes are kept as
they are. An empty completion gives an empty solution.

## Grading

The grader is the toolchain at the freeze commit: the commit of
`v0.0.2/m2-ops` that adds `pilot/registration.md`, the integrator's squash
of step 38. Steps 39 to 41 build the toolchain from the freeze commit or a
later one that changes no checker, interpreter, or test-world code. The
registration was validated against the toolchain at
`4294c424d71a9256113723599d561f1d22c0db45`, the exit commit of step 37:
the card's example module passes `check`, and the reference solutions pass
`check` and the 57 hidden tests, with each control failing as registered.
Step 39 validates them again at the freeze commit.

Each arm is graded on its own surface: an `endlet` run that leaves the
closer off an ordinary `let` does not pass, and neither does a `noEndlet`
run that writes it.

1. **Translation, `noEndlet` only.** The solution is read with the grammar
   of the source chapter in which `let` and `module-let` take an optional
   closer:

   ```ebnf
   let        = "let", pattern, [ ":", type-expr ], "be", expr,
                ( "else", body, "endlet" | [ "endlet" ] ) ;
   module-let = "let", identifier, ":", type-expr, { visibility | doc },
                "be", expr, [ "endlet" ] ;
   ```

   An `else` right after a `let`'s expression begins its fallback, and an
   `endlet` right after an ordinary `let`'s expression closes it. If the
   whole file is read without a syntax error, the translation writes it
   back with ` endlet` (a space and `endlet`) inserted right after the last
   token of the expression of every `let` read without `endlet`, and
   changes no other byte. It also counts the ordinary `let` forms it read
   closed by `endlet`: the run's **closed count**. If the reading fails
   anywhere, the file is left unchanged and the closed count is zero. The
   translation reads only the solution; it is applied to nothing else, and
   the `endlet` arm's solutions are not translated. A solution of the
   `endlet` arm passed through it is unchanged; as a `noEndlet` solution
   its closed count is the number of its ordinary `let` forms.
2. **Placement.** A fresh temporary directory receives the task's project
   file, its hidden test module, and the solution as
   `src/pilot/<task>.vib`, and nothing else. Nothing in a solution can
   replace the project file or the tests.
3. **Check and test.** `vibra --format json --workspace <dir> check`, then
   `vibra --format json --workspace <dir> test`, under the published limit
   profile `m1Default`. Each command has 120 seconds of wall-clock time. A
   command still running then is stopped, and its `status` is `unknown`
   with the reason "the grader's time limit": the limit profile bounds a
   test's memory and not its work, so a solution that loops without
   allocating reaches no other limit. A command that ends without a JSON
   document has the `status` `unknown` with the reason "the toolchain gave
   no document": the toolchain failed, which shows nothing about the
   solution except that it did not pass.
4. **The run's grade.** For a `noEndlet` run whose closed count is above
   zero, the grade is `refuted` with the reason "an ordinary let is
   closed", whatever `check` and `test` report. Otherwise the grade is the
   `status` of `check` when it is not `passed`, and else the `status` of
   `test`: one of `passed`, `refuted`, `unknown`, `unsupported`. The grader
   also records each command's `result`, the codes of the diagnostics, and
   the closed count of every `noEndlet` run.

The outcome of a run is **pass** when its grade is `passed`: its program
passes `check` and every hidden test (`test` reports `passed`, with every
selected test `@test.passed`), and, for a `noEndlet` run, its closed count
is zero. Every other grade is not a pass, a reached limit (`unknown`)
included.

Regrading: step 40 regrades 20 stored solutions, those whose run
identifiers come first when the 200 identifiers are sorted by the
lowercase hexadecimal SHA-256 of the UTF-8 text `vibraPilot20261126:`
followed by the identifier, skipping runs with no completion. Each must
get the same grade.

## Hidden tests

The hidden test module of each task imports `@std.assert` and the task's
module, and holds exactly these tests, in this order. A name `f` in an
expression is the task's own declaration. Each test asserts with
`assert.equal` that the expression has the expected value; where a row
gives two expected values, the test asserts both, in order.

### T1 `packet`

| Test | Expression | Expected |
| --- | --- | --- |
| an item scores its two points and its code | `packetScore(Packet.item(points: [2i32 3i32] code: 7i32))` | `Option.some(value: 12i32)` |
| the stop code scores nothing | `packetScore(Packet.item(points: [2i32 3i32] code: 99i32))` | `Option.none()` |
| the wrong number of points scores nothing | `packetScore(Packet.item(points: [2i32] code: 7i32))` | `Option.none()` |
| an empty packet scores nothing | `packetScore(Packet.empty())` | `Option.none()` |
| three points score nothing | `packetScore(Packet.item(points: [1i32 2i32 3i32] code: 7i32))` | `Option.none()` |
| negative points are added | `packetScore(Packet.item(points: [-5i32 -6i32] code: 1i32))` | `Option.some(value: -10i32)` |
| the score saturates | `packetScore(Packet.item(points: [2147483647i32 1i32] code: 0i32))` | `Option.some(value: 2147483647i32)` |

### T2 `pair`

| Test | Expression | Expected |
| --- | --- | --- |
| a reversed pair is ordered with its delta | `normalizePair({low: 8i32 high: 3i32})` | `{first: 3i32 second: 8i32 delta: Option.some(value: 5i32)}` |
| the delta can be left out | `normalizePair({low: 8i32 high: 3i32} includeDelta: false)` | `{first: 3i32 second: 8i32 delta: Option.none()}` |
| equal bounds have a delta of zero | `normalizePair({low: 4i32 high: 4i32})` | `{first: 4i32 second: 4i32 delta: Option.some(value: 0i32)}` |
| an ordered pair stays in order | `normalizePair({low: 3i32 high: 8i32})` | `{first: 3i32 second: 8i32 delta: Option.some(value: 5i32)}` |
| the delta saturates | `normalizePair({low: 2147483647i32 high: -2147483648i32})` | `{first: -2147483648i32 second: 2147483647i32 delta: Option.some(value: 2147483647i32)}` |

### T3 `positive`

| Test | Expression | Expected |
| --- | --- | --- |
| a positive raw value is made | `makePositive(3i32)` | `Result.ok(value: Positive(3i32))` |
| zero is not positive | `makePositive(0i32)` | `Result.err(error: PositiveError.nonPositive())` |
| a positive value reads back | `readPositive(Positive(3i32))` | `3i32` |
| a negative raw value is not positive | `makePositive(-1i32)` | `Result.err(error: PositiveError.nonPositive())` |
| the largest value is positive | `makePositive(2147483647i32)` | `Result.ok(value: Positive(2147483647i32))` |

### T4 `range`

| Test | Expression | Expected |
| --- | --- | --- |
| an ordered range has its span | `spanOr(2i32 5i32 -7i32)` | `3i32` |
| a reversed range gives the fallback | `spanOr(5i32 2i32 -7i32)` | `-7i32` |
| equal bounds span zero | `spanOr(4i32 4i32 -7i32)` | `0i32` |
| the constant reversed range is handled | `reversedSpanOr(-7i32)` | `-7i32` |
| a reversed construction is the error | `makeRange(5i32 2i32)` | `Result.err(error: RangeError.invalid())` |
| an ordered construction is the range | `makeRange(2i32 5i32)` | `Range(low: 2i32 high: 5i32)` |
| the span saturates | `spanOr(-2147483648i32 2147483647i32 0i32)` | `2147483647i32` |

### T5 `order`

| Test | Expression | Expected |
| --- | --- | --- |
| ordered bounds choose the upper | `orderedChoice(2i32 5i32)` | `Result.ok(value: 5i32)` |
| reversed bounds are an error | `orderedChoice(5i32 2i32)` | `Result.err(error: OrderError.reversed())` |
| equal bounds choose that value | `orderedChoice(4i32 4i32)` | `Result.ok(value: 4i32)` |
| the upper of constant ordered bounds | `chooseUpper(1i32 9i32)` | `9i32` |

### T6 `gate`

| Test | Expression | Expected |
| --- | --- | --- |
| a disabled gate refuses | `viaGate(gateAmount false 4i32)` | `Result.err(error: GateError.disabled())` |
| an enabled gate passes a positive amount | `viaGate(gateAmount true 4i32)` | `Result.ok(value: 4i32)` |
| an enabled gate refuses an amount that is not positive | `viaGate(gateAmount true -4i32)` | `Result.err(error: GateError.nonPositive())` |
| a disabled gate refuses a negative amount | `viaGate(gateAmount false -4i32)` | `Result.err(error: GateError.disabled())` |
| an enabled gate refuses zero | `viaGate(gateAmount true 0i32)` | `Result.err(error: GateError.nonPositive())` |
| the gate called directly | `gateAmount(true 7i32)` | `Result.ok(value: 7i32)` |

### T7 `reader`

Each test but the first takes `{world: TestWorld}`; each performs the
operation interfaces it uses. `at` is
`RelativePath(["a.txt"])`. A directory is made by
`TestWorld.directory(world files: …)` and its `readText` grant is the
reader.

| Test | Setup and expression | Expected |
| --- | --- | --- |
| no reader gives the fallback | `await readOptional(Option.none() at "fallback")` | `Result.ok(value: "fallback")` |
| a reader reads the file | files `[at: "hello"]`; `await readOptional(Option.some(value: reader) at "fallback")` | `Result.ok(value: "hello")` |
| a missing file is the error, not the fallback | no files; the same call | `Result.err(error: FsError.notFound())` |
| a path outside the reader's policy is denied | files `[at: "hello"]`; the reader restricted by `DirReadText.restrict` to the policy `FsPolicy.files(prefix: RelativePath([]) depth: FileDepth.children() actions: [FileAction.read()] names: NameTest.exact(names: ["b.txt"]))`; the same call | `Result.err(error: FsError.denied())` |
| an empty file reads as empty text | files `[at: ""]`; the same call | `Result.ok(value: "")` |

### T8 `plan`

Each test takes `{world: TestWorld}` and performs the operation interfaces
it uses. `at` is
`RelativePath(["out.txt"])`. `TestWorld.directory(world)` makes one empty
directory; its `writeText` grant is the writer and its `readText` grant
reads the file back afterwards. A writer bounded at `k` invocations is:

```vibra
DirWriteText.limit(
  writer
  GrantLimits(
    invocations: Option.some(value: k)
    inputBytes: Option.none()
    outputBytes: Option.none()
  )
)
```

| Test | Setup and expression | Expected |
| --- | --- | --- |
| an enabled plan runs twice and writes | `await runTwice(true writer at "x")`; then the read of `at` | `Result.ok(value: 2u32)`; then `Result.ok(value: "x")` |
| a disabled plan writes nothing | `await runTwice(false writer at "x")`; then the read | `Result.ok(value: 2u32)`; then `Result.err(error: FsError.notFound())` |
| the second run's failure is the result and the first write stays | `await runTwice(true w at "x")` with `w` the writer bounded at `1u64`; then the read | `Result.err(error: FsError.limitExceeded())`; then `Result.ok(value: "x")` |
| two runs use exactly two invocations | `await runTwice(true w at "x")` with `w` the writer bounded at `2u64`; then `await DirWriteText.write(w at "y")` | `Result.ok(value: 2u32)`; then `Result.err(error: FsError.limitExceeded())` |
| preparing a plan runs nothing | `prepare(true writer at "x")`, bound to `_` and never awaited; then the read | `Result.err(error: FsError.notFound())` |
| a disabled emit is ok and writes nothing | `await emit(false writer at "x")`; then the read | `Result.ok(value: void)`; then `Result.err(error: FsError.notFound())` |

### T9 `optional`

| Test | Expression | Expected |
| --- | --- | --- |
| nothing stays nothing | `adjust(Option.none() 2i32)` | `Option.none()` |
| a positive value is offset | `adjust(Option.some(value: 3i32) 2i32)` | `Option.some(value: 5i32)` |
| a value that is not positive is clamped before the offset | `adjust(Option.some(value: -3i32) 2i32)` | `Option.some(value: 2i32)` |
| zero is clamped before the offset | `adjust(Option.some(value: 0i32) 2i32)` | `Option.some(value: 2i32)` |
| the offset saturates | `adjust(Option.some(value: 2147483647i32) 1i32)` | `Option.some(value: 2147483647i32)` |
| a mapped value | `mapOptional(Option.some(value: "ab") Str.length)` | `Option.some(value: 2u64)` |
| nothing maps to nothing | `mapOptional(e Str.length)` with `e: Option(Str)` bound to `Option.none()` | `Option.none()` |

### T10 `rows`

| Test | Expression | Expected |
| --- | --- | --- |
| a later row of a key wins and the entries are in key order | `Dict.entries(indexRows([Row(key: "b" amount: 1i32) Row(key: "a" amount: 2i32) Row(key: "b" amount: 3i32)]))` | `[{"a" 2i32} {"b" 3i32}]` |
| the latest amount of a key | `latest([Row(key: "b" amount: 1i32) Row(key: "b" amount: 3i32)] "b")` | `Option.some(value: 3i32)` |
| a missing key has no amount | `latest([] "a")` | `Option.none()` |
| no rows make an empty index | `Dict.entries(indexRows([]))` | the empty `List({Str I32})` |
| an unknown key has no amount | `latest([Row(key: "b" amount: 1i32)] "a")` | `Option.none()` |

## Negative controls

A control is the task's reference solution with the one change given. It
fails as registered when its grade is not `passed` and it shows the
expected diagnostic code or failing tests. A control whose only error is
`@contract.unproved` has the grade `unknown`, as the tooling chapter
gives an obligation that is not established.

| Task | Control | Change from the reference | Expected |
| --- | --- | --- | --- |
| T1 | `packetStopIgnored` | the `match` arm for the code `stopCode` is removed | `refuted`; the test "the stop code scores nothing" fails |
| T2 | `pairDeltaOffByDefault` | the default of `includeDelta` is `false` | `refuted`; "a reversed pair is ordered with its delta" fails, among others |
| T3 | `positiveUnchecked` | `makePositive`'s body is `return Result.ok(value: Positive(raw))` | `@contract.unproved` |
| T3 | `positiveZero` | `makePositive` returns `Result.ok(value: Positive(0i32))` where it returned the error | `@contract.refuted` |
| T4 | `rangeUnhandled` | `spanOr` binds `Result.ok(value: range)` from `makeRange(low high)` with no `else` | `@pattern.refutableBinding` |
| T5 | `orderUnguarded` | `orderedChoice`'s body is `return Result.ok(value: chooseUpper(low high))` | `@contract.unproved` |
| T5 | `orderSwapped` | the guarded branch calls `chooseUpper(high low)` | `@contract.unproved` |
| T5 | `orderConstantFalse` | `orderedChoice` returns `Result.ok(value: chooseUpper(5i32 2i32))` where it returned the error | `@contract.refuted` |
| T6 | `gateBareRequires` | `gateAmount`'s `requires` has no `else` | `@contract.obligationLost` |
| T6 | `gateBareEnsures` | `gateAmount`'s `ensures` has no `else` | `@contract.unproved` |
| T7 | `readerFallbackOnError` | an error of the read returns `Result.ok(value: fallback)` | `refuted`; "a missing file is the error, not the fallback" fails |
| T7 | `readerPure` | `readOptional` is a `fn` with no `performs` | `@effect.invalidAwait` |
| T8 | `planOnce` | `runTwice` awaits the plan once | `refuted`; "two runs use exactly two invocations" fails |
| T8 | `planNoCeiling` | `runTwice` has no `performs` | `@effect.outsideCeiling` |
| T9 | `optionalBareRequires` | `clampPositive`'s `requires` has no `else` | `@contract.unproved` |
| T9 | `optionalNoOffset` | the anonymous function returns `clampPositive(value)` | `refuted`; "a positive value is offset" fails |
| T10 | `rowsAmountLost` | each row is mapped to `{key 0i32}` | `refuted`; "the latest amount of a key" fails |

## Budget

- **Calls.** One call per run, plus at most one retry of a call that
  failed at the provider: at most 400 calls. A call failed at the provider
  when no completion came back: the client exited with an error, timed
  out after 600 seconds, reported an error, returned no completion, or the
  context audit failed. A retry is made at once, with the same prompt, and
  never because a completion was wrong. A completion, empty or truncated
  at the cap, is graded as it is.
- **Setup calls.** Before a cohort's first run, one setup call checks the
  client, the model identifier, and the settings: the cohort's invocation
  as [Models](#models) gives it, with the prompt
  `Reply with the single word ready.` It is no run: it sends no part of a
  card, the library, or a task, and its output is recorded and not graded.
  A cohort starts only after a setup call returned a completion from the
  registered model identifier. A failed setup call is corrected and made
  again, at most five times per cohort; these calls are outside the 400,
  outside the stop rule, and are counted and reported.
- **Tokens.** The ceiling of one call is the byte count of the largest
  prompt, which bounds its input tokens, plus 4,096 output tokens:
  16,527 + 4,096 = 20,623 tokens. The ceiling of the pilot is 400 calls at
  that ceiling: 8,249,200 tokens. Each call's reported usage is recorded;
  a call that reports more than 4,096 output tokens is kept, graded, and
  counted.
- **Stop rule.** A cohort stops after five consecutive calls that failed at
  the provider, retries included; any call that returns a completion resets
  the count. A stopped cohort makes no further call and has no partial
  analysis. The other cohort continues.
- **Approval.** The owner fixed the call budget, the retry rule, and the
  stop rule on 2026-11-26, and left the token ceiling to this
  registration, which fixes it above from the size of the prompts. The
  setup calls, at most ten, were added by the integrator before the freeze
  and reported to the owner.

## Outcome measure

For a model `m` and an arm `a`, `n(m, a)` is the number of its runs that
are not excluded and `p(m, a)` the number of those that pass. A
`noEndlet` run with a closed count above zero is counted in `n` and not in
`p`. The pass rate is `p(m, a) / n(m, a)`. The measure is reported per
model and arm, per task, and overall.

## Decision rule

The `endlet` arm is the default. The `noEndlet` arm is **favoured** when,
for both models, `n(m, a)` is not zero for either arm and it passes at
least as often:

```text
p(m, noEndlet) × n(m, endlet) ≥ p(m, endlet) × n(m, noEndlet)
```

and, for at least one model, it passes at least 10 percentage points more
often:

```text
10 × (p(m, noEndlet) × n(m, endlet) − p(m, endlet) × n(m, noEndlet))
  ≥ n(m, endlet) × n(m, noEndlet)
```

With no exclusion, 10 percentage points is 5 of 50 runs. The rule reads
`p` and `n` as the outcome measure defines them, so a `noEndlet` run lost
to a closed `let` counts against its arm. The rule has two answers:

- **The default stays**: `let … be … endlet` remains the specified form.
  This is the answer for equal arms and whenever a cohort stopped.
- **The other arm is favoured**: the question returns to the owner. This
  changes nothing by itself.

## Exclusions

- A run whose call and retry both failed at the provider (`providerError`):
  it has no completion, so it says nothing about the surface. It is kept in
  the run table and counted, and left out of `n`.
- Every run of a stopped cohort, for the reason the stop rule gives.

Nothing else is excluded. A run whose provider reports a model identifier
other than the registered one is kept and listed.

## What the analysis reports

Besides the measure and the rule's answer, and outside the rule: the grade
counts per model and arm; the diagnostic codes of the runs that do not
pass, per arm; the number of `noEndlet` solutions the translation changed;
per model, the number of `noEndlet` runs with a closed count above zero,
and how many of them would otherwise have passed; every run that a
command's time limit stopped or that got no document; the exclusions with
their counts; the reported usage; and what the
observation does not show. Any other cut is exploratory and is labelled so.

## Freeze checklist

The freeze is refused while any item has no value.

| Item | Value |
| --- | --- |
| question | an ordinary `let` ends with `endlet` or not |
| arms | `endlet`, `noEndlet` |
| tasks | ten, T1 to T10 |
| models | `claude-haiku-5-5` at effort `low`; `gpt-6-luna` at reasoning effort `low` |
| attempts | five per task, arm, and model |
| schedule | per cohort: attempt, task, arm alternating by attempt |
| prompts | the parts above, with 20 prompt digests |
| extraction | the first fenced block, else the whole completion |
| translation | the optional-closer reading, `noEndlet` only, with its closed count |
| own surface | a `noEndlet` run with a closed count above zero is `refuted`: "an ordinary let is closed" |
| grader | the toolchain at the freeze commit |
| time limit | 120 seconds per command; a command stopped there is `unknown` |
| no document | a command that ends without a JSON document is `unknown` |
| outcome | `check` and every hidden test pass, and a `noEndlet` closed count of zero |
| decision rule | at least as often on both models, 10 points more on one |
| exclusions | provider errors; a stopped cohort |
| budget | 400 calls; 20,623 tokens per call; 8,249,200 in all |
| stop rule | five consecutive provider failures stop a cohort |
| regrade seed | `vibraPilot20261126:` |
| date | 2026-11-26 |

## Amendments

None.
