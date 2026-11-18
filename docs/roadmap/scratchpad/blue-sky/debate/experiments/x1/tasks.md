# Experiment X1: common language and tasks (draft for Codex to amend)

Status: draft preregistration; not final until both reviewers accept it

Every surface writes the **same small language**. Only spelling differs.
This file is surface-neutral: it is not shown to participants as is.
Each surface card restates it in that surface's spelling.

## Common language

**Types.** `int` (unbounded), `bool`, `text`, `list of T`,
`option of T` (`some` with `value`, or `none`), records (named fields),
variants (named cases, each with named fields), and wrapped types with a
predicate (`cents` wraps `int` where it is at least 0; `T.of` builds
one, `x.value` reads it).

**Declarations.** A type. A function with typed parameters, a result
type, zero or more `requires`, `ensures`, and `example` clauses, and a
body.

**Statements.** Bind a name once (`let`). `if` / `else if` / `else`.
`match` on a named variant value, one arm per case; inside an arm the
value's fields for that case are readable. `return`. `try` on an option
binds its value or returns `none` from the function.

**Expressions.** Literals, names, field reads, calls, constructor
calls, list literals, and a conditional expression.

**No** assignment, loops, operators, or anonymous functions in the main
run. Operators and anonymous functions appear only in their arms.

**Standard functions.**

| Module | Functions |
| --- | --- |
| `int` | `add sub mul div rem neg min max`, `eq lt le gt ge` |
| `bool` | `and or not` |
| `text` | `length concat eq`, `from-int` |
| `list` | `length is-empty first rest get append concat reverse`, `map filter fold` |
| `option` | `some none is-some` |

`int.div` and `int.rem` truncate toward zero and require a non-zero
divisor. `list.first` and `list.rest` require a non-empty list.
`list.get` returns an option. `list.fold` takes the list, a start
value, and a function of accumulator and item.

## Tasks

Each task gives the participant a prose statement and the exact names
and types to declare. Hidden tests call the declared functions.

| # | Task | Exercises |
| --- | --- | --- |
| 1 | `clamp`: bound `x` to `lo..hi`; requires `lo <= hi`; two examples | Contract clauses, `if` chain |
| 2 | `grade`: map a score 0 to 100 to a variant `fail`, `pass`, `merit`, `distinction` at 50, 65, 80; out-of-range gives `none` | Variant construction, option, nested conditions |
| 3 | `area`: a variant `shape` with `circle` (radius), `rect` (width, height), `square` (side); return area with pi taken as 3 | Type declaration, `match`, field reads |
| 4 | `sum-positive`: sum the positive items of a list by recursion, no `fold` | Recursion, termination, `first`/`rest` guarded by `is-empty` |
| 5 | `total-due`: given a list of line items (record: quantity, unit price in `cents`), return the total as `cents`, using `map` and `fold` with named helper functions | Records, wrapped type, transforms, functions as values |
| 6 | `split-cents`: split a total into `parts` shares differing by at most one, largest first; `ensures` for count and sum; one example | A harder contract, arithmetic, building a list |
| 7 | `third-item-doubled`: take the third item of a list of options of int, doubled, or `none` if the list is short or the item is `none` | `try`, option chain |
| 8 | `parse-args`: turn a list of text arguments into a record of settings (`verbose`, `output` path, list of inputs) or an error variant naming the bad argument. About forty lines | Long function, several helpers, nested `match`, accumulation by recursion |
| 9 | Edit: given a correct `parse-args`, add a `--limit N` option that must be a non-negative integer | Editing existing code in the surface |
| 10 | Resume: given the first half of task 8's reference solution, cut mid-block, finish it without restating the given half | Recovery from truncation, closing open blocks correctly |

Tasks 1 to 7 are written in one response. Tasks 8, 9, and 10 are
separate responses.

## Scoring

Per task and participant:

1. **Parses** with that surface's parser, as first submitted.
2. **Passes** all hidden tests, as first submitted.
3. **Tokens** in the submitted program.
4. **Error kind** when it fails: unbalanced or wrong closer, wrong
   separator, wrong argument order, missing label, unknown name, type
   error, wrong logic, other.

No repair turn in the main run. A second run allows one repair turn
with the parser's error message, to measure repair cost.

## Readability test (X6)

Asked for by Hannah on 2026-11-15, who also added the weak-model
readers.

**Material.** For each surface, the same eight contracts with their
bodies removed: signature, `requires`, `ensures`, examples, and any
reference definition. Each comes with five questions of the form "what
does `f` return for these arguments?" or "is this call allowed?",
chosen at boundaries: equal bounds, zero, empty list, the first value
past a threshold.

**Readers.**

| Reader | Setting | Why |
| --- | --- | --- |
| Hannah | One sitting, about ten minutes per surface, surface order randomized | The person who must trust a witness from its claim |
| Claude Haiku | Lowest reasoning effort, fresh context per surface | The weakest Claude-family reader |
| Luna | Lowest reasoning effort, fresh context per surface | The weakest Codex-family reader |

Model readers run at least five times per surface. A reader sees the
surface's one-page card and the contracts, nothing else.

**Measures.** Share of questions answered correctly per surface, and
for Hannah the time taken. Questions and answer key are fixed before
any reader starts, and the key is produced by the harness evaluating
the reference solutions, not by hand.

**Hannah's interface.** One self-contained HTML page per session. It
shows each contract with its questions, takes an answer and an optional
comment per question and a free comment per surface, times each
surface, and has a button that copies the whole review as JSON for her
to paste into the session. No server and no network.

**Note on Hannah's run.** A person cannot be given a fresh context, so
her surface order is randomized and the contracts differ in constants
between surfaces to limit carry-over.

## Open choices for Codex to settle

- Whether surface K writes types as `x int` (so `:` only ever marks a
  section) or as `x: int`.
- Whether the draft surface D keeps its type-dependent label rule or
  uses the rule chosen for the label arm.
- Hidden-test format, once the harness fixes its value encoding.
