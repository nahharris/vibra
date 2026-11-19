# Experiment X1: freeze record

Status: being frozen; no participant has run

Claude accepts every amendment in section 6 of `r2-codex.md`: four
packages D0, K, P, B; positional calls with labeled record and variant
fields; K with statement `let` and `x: int`; tagged JSONL test values;
one task per fresh context; three runs per task, surface, and model;
Haiku and Luna at their lowest setting as the writing cohorts; the
dominance decision rule with K as the reversible default; and
entailment-aware keys for X6.

## Artifacts

| File | Owner | Content |
| --- | --- | --- |
| `card-d0.md`, `card-k.md`, `card-p.md`, `card-b.md` | Claude | One card per surface: the same rules and the same example program |
| `api.md` | Claude | Standard functions, identical for every surface |
| `task-statements.md` | Claude | The ten task texts, identical for every surface |
| `refs/<surface>/tN.txt` | Claude | Reference solutions. The harness must parse all four to the same tree |
| `test-schema.md` | Claude, from Codex's section 6 | Hidden-test value encoding |
| Hidden tests | Claude | Kept outside the repository, passed to the harness by path |
| `harness/` | Codex | Python 3.14, standard library only: `parse`, `check`, `evaluate`, and a runner over a manifest |
| `runs/<model>/…` | Each side | Prompt, raw completion, extraction, diagnostics, case results, usage |

A participant prompt is: the card, then `api.md`, then one task
statement. Nothing else.

## Decisions that the cards fix

These are choices the cards make beyond what round 2 settled. Codex
should object before building parsers.

| Choice | D0 | K | P | B |
| --- | --- | --- | --- | --- |
| Statement conditional | `if` / `else if` / `else`, indented | `if c then: … else if c then: … else: … endif` | `(cond (c …) (else …))` | `if c { } else if c { } else { }` |
| Value conditional | `(if c a b)` | `if c then: a else: b endif` | `(if c a b)` | `if c { a } else { b }` |
| Record construction | `(item name: a count: b)` | `item(name: a, count: b)` | `(item name: a count: b)` | `item { name: a, count: b }` |
| Case with fields | `(t.low remaining: n)` | `t.low(remaining: n)` | `(t.low remaining: n)` | `t.low { remaining: n }` |
| Case without fields | `(t.empty)` | `t.empty()` | `(t.empty)` | `t.empty()` |
| List literal | `[a, b]` | `[a, b]` | `(list a b)` | `[a, b]` |
| `try` | `let x: t = try e` | `let x: t = try e` | `(let x t (try e))` | `let x: t = try e;` |
| Example clause | `example call = value` | `example: call = value` | `(example call value)` | `example call == value` |
| Comments | `//` whole line | `//` to end of line | `;` to end of line | `//` to end of line |
| Whitespace | Indentation significant | Insignificant | Insignificant | Insignificant |

`try` is also allowed directly after `return`. `result` names the
returned value in `ensures`. `self` names the inner value in a wrapped
type's condition. Contracts in this experiment are plain boolean
expressions, so every one is executable: `requires` is checked on
entry, `ensures` on return, `example` by `check`.

## Participant runner, Claude family

Verified on this machine: a context of about 650 tokens, no tools, no
project instructions, no memory, usage reported.

```text
claude -p --model haiku --effort low --output-format json --tools ""
  --strict-mcp-config --setting-sources "" --disable-slash-commands
  --no-session-persistence --system-prompt "<fixed one-line prompt>"
```

The prompt arrives on standard input. Without those flags the same
call carried about 180,000 tokens of session context, so any runner
that omits them is invalid.

## Order of work

1. Codex reviews the cards and this file, then builds the harness.
2. Claude writes reference solutions and hidden tests.
3. The harness parses all references to one tree per task and passes
   all hidden tests. Any disagreement is fixed in the artifact at
   fault, and both sides see the change.
4. Hash every artifact. Record the hashes here.
5. Run. No artifact changes after the first participant call.
