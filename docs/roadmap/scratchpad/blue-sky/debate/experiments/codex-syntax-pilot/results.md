# Executed toy pilot results

The original first-complete programs remain unchanged in `participant.md`. Before implementing the evaluator, their three code blocks were preserved in `raw-programs.json`, in K → P → B order. All scripts and execution used PowerShell.

| Surface | Parse errors | Semantic passed/total | Characters | UTF-8 bytes | Lexical tokens |
| --- | ---: | ---: | ---: | ---: | ---: |
| K | 0 | 200/200 | 886 | 886 | 212 |
| P | 0 | 200/200 | 560 | 560 | 163 |
| B | 0 | 200/200 | 652 | 652 | 204 |

Each surface passed 35 ratio cases (`n=-3..3`, `d=-2..2`), 125 clamp cases (`x,lo,hi=-2..2`), and 40 classify cases (all lists of length 0..3 over `-1,0,1`). The interpreter raises an exception for out-of-bounds `list.get`; none occurred in these cases. The scripts and raw output are `evaluate.ps1` and `evaluation.json`.

Character counts are .NET UTF-16 code units; these particular programs are ASCII, so character and UTF-8 byte counts agree. Counts include the code block contents and their internal whitespace, excluding Markdown fences and their boundary newlines. Lexical tokens are identifiers, integer literals, and individual punctuation marks, excluding whitespace. They are not model tokens.

The card does not define rounding for `div`. This interpreter uses exact normalized rational division. It also implements the explicitly documented extension that a named `fn` may occur as a self-recursive value in `let`, enabling the required local `walk` helper. `list.append(list, element)` appends a single element to a fresh immutable list. Bounded arithmetic uses Int64; these cases stay within its range. Thus this is conditional execution under the stated toy conventions, not validation against a complete language specification.

This remains a one-participant, fixed-order pilot with transfer between completions and a shared handwritten evaluator. Passing finite cases does not prove correctness or termination, and the size measures do not establish a syntax winner.

`countermodels.ps1` produced `countermodels.json`. All four witness assertions passed:

1. `div(6, n-3)` completed on the five sampled inputs `-2..2`, but failed with a zero divisor at `n=3`.
2. Updating a mutable array through a transferred reference after dropping `x` changed the surviving alias `y` from `[10,20]` to `[11,20]`.
3. A singleton `unit` result admits exactly one function on the three-input toy domain and no rejected type-correct output; a criterion requiring a rejected output therefore rejects this fully determined contract.
4. The structurally decreasing recursive function `f(0)=1; f(n)=f(n-1)+f(n-1)` returned 32 at the executed input 5. Its analytical call count at input 30 is 2,147,483,647, exceeding a 1,000,000-call budget. The large case was not executed.

These are executable **TOY COUNTERMODELS of the explicitly stated candidate rules**, not Vibra checker verdicts. The exponential result is an analytical function-call count, not a wall-time measurement.
