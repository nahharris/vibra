# Model-writing pilot: observation

Dated 2026-11-26. This is the registered analysis of the pilot that the
[registration](registration.md) fixes, over the run table
[`runs.tsv`](runs.tsv) as committed in `2310f43d` ("M2 step 40: pilot
runs", `2310f43de1467761c596496cd50caf7ddbc1d3da`). It is evidence about
one surface choice, whether an ordinary `let` ends with `endlet`, on the
ten registered tasks and the two registered models.

## The rule's answer

The default stays: `let … be … endlet` remains the specified form, because
the `noEndlet` arm passes less often than the `endlet` arm on `haiku`.

The rule asks for no proposal against the specification.

## The runs

| Item | Value |
| --- | --- |
| question | an ordinary `let` ends with `endlet` or not |
| arms | `endlet`, `noEndlet` |
| tasks | T1 `packet` to T10 `rows`, as registered |
| models | `haiku`: `claude-haiku-5-5` at effort `low`; `luna`: `gpt-6-luna` at reasoning effort `low` |
| schedule | five attempts per task, arm, and model: 200 runs, 100 per cohort |
| calls | between 2026-11-26T12:12:22Z (the first setup call) and 2026-11-26T12:31:30Z (the end of the last run) |
| clients | Claude Code `2.1.318`; `codex-cli 0.168.0` |
| run records | [`runs/`](runs/README.md) |

Every one of the 200 runs had one call, and every call returned a
completion: no call failed at the provider, no retry was made, and no
cohort stopped, so nothing is excluded. Each cohort made one setup call
before its first run, and each returned a completion.

The `luna` cohort's client reports no model identifier, so its run and
setup records read `notReported`. The model of each of its calls was the
one requested on the command line, `gpt-6-luna`, and the context audit
passed before each call: exactly the system prompt and the prompt, and no
tool.

One `haiku` call reported more output tokens than the cap of 4,096:
`haiku-range-noEndlet-2`, with 4,273, of which 3,802 are thinking tokens.
It is kept and graded as recorded, as the registration's budget says.

## The grader

The grader is the toolchain at the freeze commit
`156f35cf2a7f10c8d9df37be43053ba74e941605`. The run records name the
binary that graded, `target/debug/vibra.exe` with SHA-256
`da9854d8fab200db7a96173fb77844a80a10e21fac0665fb7d68afbef107dcea`. The
integrator rebuilt `vibra` from the same sources in another worktree
(`v0.0.2/m2-ops` at `2310f43d`, whose crates are those of the freeze
commit), whose binary has the SHA-256
`3a120746241b937ce3e4cf4308c7c99e1736eac72cd60a438424a56f5792cf79`, and
with it `grade.ps1 -VerifyRuns` gave the same grade and the same line for
the 20 regraded runs and found the schedule complete (200 of 200), and
`grade.ps1 -All` gave 80 of 80 subjects as registered. A debug build's
digest depends on the directory it is built in, so the digest identifies
the binary that graded and the freeze commit identifies the toolchain.

## Where each number comes from

The script
[`examples/pilot/analyze.ps1`](../../../../../examples/pilot/analyze.ps1)
prints every table below from `runs.tsv` alone, except two: the setup
calls, which it reads from the run records
`runs/setup/<cohort>-<n>/record.tsv`, and the translation counts of
[Own surface](#own-surface), which `grade.ps1 -TranslationCounts` computed
from the stored solutions.
The measure and the rule read only the columns `cohort`, `arm`, `task`,
`outcome`, and `grade`; the grades, the closed counts, the codes, the
reported models, the calls, and the usage are the table's own columns. The
rule is applied in integers, as the registration writes it.

```powershell
pwsh -NoProfile -File examples/pilot/analyze.ps1 docs/roadmap/v0.0.2/m2/pilot/runs.tsv
```

## Exclusions

| Reason | `haiku` | `luna` | All |
| --- | --- | --- | --- |
| both calls failed at the provider (`providerError`) | 0 | 0 | 0 |
| a stopped cohort | 0 | 0 | 0 |
| excluded | 0 | 0 | 0 |
| counted in `n` | 100 | 100 | 200 |
| scheduled | 100 | 100 | 200 |

No run reports a model identifier other than the registered one; the
`notReported` runs of `luna` are listed under
[Calls and reported models](#calls-and-reported-models).

## Outcome measure

`n` counts the runs that are not excluded and `p` those that pass.

### Per model and arm

| Model | Arm | n | p | Pass rate |
| --- | --- | --- | --- | --- |
| `haiku` | `endlet` | 50 | 50 | 100.0% |
| `haiku` | `noEndlet` | 50 | 48 | 96.0% |
| `luna` | `endlet` | 50 | 45 | 90.0% |
| `luna` | `noEndlet` | 50 | 46 | 92.0% |

### Per task

Each cell is `p/n`.

| Task | `haiku` `endlet` | `haiku` `noEndlet` | `luna` `endlet` | `luna` `noEndlet` | Both `endlet` | Both `noEndlet` |
| --- | --- | --- | --- | --- | --- | --- |
| T1 `packet` | 5/5 | 5/5 | 5/5 | 5/5 | 10/10 | 10/10 |
| T2 `pair` | 5/5 | 5/5 | 1/5 | 4/5 | 6/10 | 9/10 |
| T3 `positive` | 5/5 | 5/5 | 5/5 | 5/5 | 10/10 | 10/10 |
| T4 `range` | 5/5 | 4/5 | 4/5 | 4/5 | 9/10 | 8/10 |
| T5 `order` | 5/5 | 5/5 | 5/5 | 5/5 | 10/10 | 10/10 |
| T6 `gate` | 5/5 | 4/5 | 5/5 | 5/5 | 10/10 | 9/10 |
| T7 `reader` | 5/5 | 5/5 | 5/5 | 3/5 | 10/10 | 8/10 |
| T8 `plan` | 5/5 | 5/5 | 5/5 | 5/5 | 10/10 | 10/10 |
| T9 `optional` | 5/5 | 5/5 | 5/5 | 5/5 | 10/10 | 10/10 |
| T10 `rows` | 5/5 | 5/5 | 5/5 | 5/5 | 10/10 | 10/10 |

### Overall

| Arm | n | p | Pass rate |
| --- | --- | --- | --- |
| `endlet` | 100 | 95 | 95.0% |
| `noEndlet` | 100 | 94 | 94.0% |

## Decision rule

The `noEndlet` arm is favoured only when `d ≥ 0` on both models and
`10 × d ≥ n(m, endlet) × n(m, noEndlet)` on at least one.

| Model | p(m, endlet) | n(m, endlet) | p(m, noEndlet) | n(m, noEndlet) | d = p(m, noEndlet) × n(m, endlet) − p(m, endlet) × n(m, noEndlet) | d ≥ 0 | 10 × d | n(m, endlet) × n(m, noEndlet) | 10 × d ≥ n × n |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `haiku` | 50 | 50 | 48 | 50 | -100 | no | -1000 | 2500 | no |
| `luna` | 45 | 50 | 46 | 50 | 50 | yes | 500 | 2500 | no |

No cohort stopped. The first condition fails on `haiku`, so the answer is
the one stated at the top: the default stays.

## Grades per model and arm

| Model | Arm | `passed` | `refuted` | `unknown` | `unsupported` | All |
| --- | --- | --- | --- | --- | --- | --- |
| `haiku` | `endlet` | 50 | 0 | 0 | 0 | 50 |
| `haiku` | `noEndlet` | 48 | 2 | 0 | 0 | 50 |
| `luna` | `endlet` | 45 | 5 | 0 | 0 | 50 |
| `luna` | `noEndlet` | 46 | 4 | 0 | 0 | 50 |

## Failure kinds per arm

A cell counts the runs that do not pass and show the code. Each of the
eleven runs that do not pass was refused by `check` with one code, and none
records a failing hidden test.

| Arm | Code | `haiku` | `luna` | All |
| --- | --- | --- | --- | --- |
| `endlet` | `@type.mismatch` | 0 | 5 | 5 |
| `noEndlet` | `@name.unknownSymbol` | 1 | 2 | 3 |
| `noEndlet` | `@syntax.invalidForm` | 1 | 0 | 1 |
| `noEndlet` | `@type.mismatch` | 0 | 2 | 2 |

| Run | Grade | Closed | `check` | `test` | Codes | Failing tests | Reason |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `haiku-range-noEndlet-2` | `refuted` | 0 | `@command.diagnostics` | `@command.diagnostics` | `@name.unknownSymbol` | - | - |
| `haiku-gate-noEndlet-2` | `refuted` | 0 | `@command.diagnostics` | `@command.diagnostics` | `@syntax.invalidForm` | - | - |
| `luna-reader-noEndlet-1` | `refuted` | 0 | `@command.diagnostics` | `@command.diagnostics` | `@name.unknownSymbol` | - | - |
| `luna-pair-noEndlet-2` | `refuted` | 0 | `@command.diagnostics` | `@command.diagnostics` | `@type.mismatch` | - | - |
| `luna-pair-endlet-2` | `refuted` | - | `@command.diagnostics` | `@command.diagnostics` | `@type.mismatch` | - | - |
| `luna-reader-noEndlet-2` | `refuted` | 0 | `@command.diagnostics` | `@command.diagnostics` | `@name.unknownSymbol` | - | - |
| `luna-pair-endlet-3` | `refuted` | - | `@command.diagnostics` | `@command.diagnostics` | `@type.mismatch` | - | - |
| `luna-pair-endlet-4` | `refuted` | - | `@command.diagnostics` | `@command.diagnostics` | `@type.mismatch` | - | - |
| `luna-range-noEndlet-4` | `refuted` | 0 | `@command.diagnostics` | `@command.diagnostics` | `@type.mismatch` | - | - |
| `luna-pair-endlet-5` | `refuted` | - | `@command.diagnostics` | `@command.diagnostics` | `@type.mismatch` | - | - |
| `luna-range-endlet-5` | `refuted` | - | `@command.diagnostics` | `@command.diagnostics` | `@type.mismatch` | - | - |

## Own surface

"Would otherwise have passed" reads the recorded results of both commands
as `@command.ok`.

| Model | `noEndlet` runs | closed count above zero | would otherwise have passed |
| --- | --- | --- | --- |
| `haiku` | 50 | 0 | 0 |
| `luna` | 50 | 0 | 0 |

No `noEndlet` run closed an ordinary `let`, so no run was lost to its own
surface.

The registration also asks for the number of `noEndlet` solutions the
translation changed. The runs did not record it, so it was computed after
the runs by running the translation alone, with no `vibra` and no grade,
over the stored `solution.vib` of every `noEndlet` run:

```powershell
pwsh -NoProfile -File examples/pilot/grade.ps1 -TranslationCounts docs/roadmap/v0.0.2/m2/pilot/runs.tsv
```

| Model | Solutions read | Changed | Unchanged | Reading failed |
| --- | --- | --- | --- | --- |
| `haiku` | 50 | 29 | 20 | 1 |
| `luna` | 50 | 28 | 22 | 0 |
| All | 100 | 57 | 42 | 1 |

The one failed reading is `haiku-gate-noEndlet-2`, the solution that
begins with `module @pilot.gate`; its file was placed unchanged, as the
registration says. The same command checked that each stored solution has
the digest its record names and that each closed count it reads equals the
run table's: no difference.

## Time limit and no document

No command was stopped at the time limit and every command gave a
document.

## Calls and reported models

| Model | Requested | Runs | Calls | Retries | Reported model | Runs reporting it |
| --- | --- | --- | --- | --- | --- | --- |
| `haiku` | `claude-haiku-5-5` | 100 | 100 | 0 | `claude-haiku-5-5` | 100 |
| `luna` | `gpt-6-luna` | 100 | 100 | 0 | `notReported` | 100 |

## Reported usage

The usage each client reported, summed over the cohort's calls; the fields
are each client's own, and no sum adds one field to another.

| Model | Field | Sum | Largest call |
| --- | --- | --- | --- |
| `haiku` | `input_tokens` | 204 | 6 |
| `haiku` | `cache_creation_input_tokens` | 175,936 | 7,141 |
| `haiku` | `cache_read_input_tokens` | 533,087 | 13,869 |
| `haiku` | `output_tokens` | 163,636 | 4,273 |
| `haiku` | `output_tokens_details.thinking_tokens` | 132,730 | 3,802 |
| `luna` | `input_tokens` | 553,835 | 12,493 |
| `luna` | `cached_input_tokens` | 24,448 | 4,992 |
| `luna` | `cache_write_input_tokens` | 0 | 0 |
| `luna` | `output_tokens` | 20,547 | 462 |
| `luna` | `reasoning_output_tokens` | 131 | 73 |

One call reports more than 4,096 output tokens: `haiku-range-noEndlet-2`
(4,273).

A call's sum adds the fields named below, since the fields are each
client's own. `haiku` reports cache writes and cache reads beside
`input_tokens`, and counts its thinking tokens within `output_tokens`; the
analysis reads `luna`'s cached input and reasoning output as parts of its
`input_tokens` and `output_tokens`, so it does not add them.

| Model | A call's sum | Calls | Largest sum | Its run | Calls above 20,623 | Cohort sum |
| --- | --- | --- | --- | --- | --- | --- |
| `haiku` | `input_tokens` + `cache_creation_input_tokens` + `cache_read_input_tokens` + `output_tokens` | 100 | 22,710 | `haiku-range-noEndlet-2` | 1 | 872,863 |
| `luna` | `input_tokens` + `output_tokens` | 100 | 12,955 | `luna-order-endlet-3` | 0 | 574,382 |

One call's reported tokens sum above the registration's per-call ceiling of
20,623: `haiku-range-noEndlet-2`, with 6 + 4,562 + 13,869 input-side tokens
and 4,273 output tokens, 22,710 in all. Every other call sums at most
12,955 (`luna-order-endlet-3`). Each cohort's sum is far below the
registration's ceiling of the pilot, 8,249,200.

The setup calls, outside the 400 calls of the budget:

| Model | Setup calls | Passed | Reported model | Usage |
| --- | --- | --- | --- | --- |
| `haiku` | 1 | 1 | `claude-haiku-5-5` | input_tokens 2, cache_creation_input_tokens 685, cache_read_input_tokens 0, output_tokens 4 |
| `luna` | 1 | 1 | `notReported` | input_tokens 707, cached_input_tokens 0, output_tokens 5 |

## Exploratory: what the stored solutions that do not pass show

This section is outside the rule and outside the registration's report
items. It reads the eleven stored `solution.vib` files and runs nothing,
so it says what the text shows, not where `check` placed each diagnostic.

- The six `@type.mismatch` runs (`luna`: `pair` four times in `endlet` and
  once in `noEndlet`, `range` once in each arm) bind a record value with a
  tuple pattern, such as `{first second}` for the record
  `{first: … second: …}` or `{low high}` for the record `bounds`.
- The two `luna` `reader` runs use `RelativePath` without
  `use @std.fs.RelativePath enduse`.
- `haiku-range-noEndlet-2`, the call over the output cap, holds only the
  function `reversedSpanOr`: its completion is 277 bytes, a fragment, and
  `reversed` is not declared in it.
- `haiku-gate-noEndlet-2` begins with the line `module @pilot.gate`, which
  is no form of the card.

In these eleven texts every ordinary `let` of an `endlet` solution is
closed by `endlet`, and every `noEndlet` run has a closed count of zero.

## Deviations from the registration

These are the places where what was done differs from what the
registration says. None changes a grade, the measure, or the rule's
answer.

- **The `luna` setup check.** The registration says a cohort starts only
  after a setup call returned a completion from the registered model
  identifier. The `luna` client reports no identifier, so its setup check
  was a completion from the call that requested `gpt-6-luna` on the
  command line, with a passing context audit. The setup call is no run: it
  is outside the 400 calls and is not graded, so it enters no grade and no
  count.
- **One call above the per-call ceiling.** `haiku-range-noEndlet-2`
  reports 22,710 tokens against the ceiling of 20,623 (see
  [Reported usage](#reported-usage)); its 18,437 input-side tokens are also
  above the 16,441 bytes of its prompt, which the registration takes to
  bound a call's input tokens. It is the only call that reports more input
  tokens than its prompt has bytes. The registration keeps and grades a
  call over the output cap, and the ceiling is a budget, not an exclusion,
  so the run is graded and counted as recorded.
- **The translation counts.** The number of `noEndlet` solutions the
  translation changed was not recorded by the runs and was computed
  afterwards from the stored solutions (see [Own surface](#own-surface)).
  It is a report item outside the rule; the computation runs the
  registered reader alone and reproduces every recorded closed count.

These are all the differences known to this analysis.

## What the observation does not show

- Other models: the pilot ran `claude-haiku-5-5` and `gpt-6-luna` at the
  registered settings, through the registered clients, and nothing here
  holds for another model, another setting, or another way of calling one.
- Other tasks: the ten tasks are small modules written from one card; the
  observation says nothing about larger programs, other constructs, or
  tasks with more than one module.
- Other surface choices: the arms differ only in the closer of an ordinary
  `let`; nothing here bears on any other spelling or form.
- People: no person wrote a solution, and nothing here describes how a
  person reads or writes either form.
- Five attempts per task, arm, and model is a small sample; the rule is a
  registered threshold, not a test of significance, and the per-task and
  per-code tables are reported outside it.
- The pilot is not a release gate.
