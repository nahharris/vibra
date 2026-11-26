# Pilot run records

The registered runs of the [registration](../registration.md), made by
[`examples/pilot/run.ps1`](../../../../../../examples/pilot/run.ps1) and
checked by `examples/pilot/grade.ps1 -VerifyRuns ../runs.tsv`. This
directory holds observations only; the analysis is step 41's.

## Toolchain and dates

| Item | Value |
| --- | --- |
| grader | the toolchain at the freeze commit `156f35cf2a7f10c8d9df37be43053ba74e941605` |
| binary | `target/debug/vibra.exe`, SHA-256 `da9854d8fab200db7a96173fb77844a80a10e21fac0665fb7d68afbef107dcea` |
| first call | 2026-11-26T12:12:22Z (setup), 2026-11-26T12:13:02Z (first run) |
| last call | ended 2026-11-26T12:31:30Z |
| clients | Claude Code `2.1.318`; `codex-cli 0.168.0` |

## Layout

- `schedule.tsv`: the 200 runs in the registered order, written and
  committed before the first call.
- `setup/<cohort>-<n>/`: the setup calls, and `setup/luna-catalog.json`,
  the model catalog every Codex call names: the installed client's entry
  for `gpt-6-luna` with the system prompt as its base instructions and its
  tool fields cleared.
- `<run>/`: one directory per run, named by the run identifier:
  - `record.tsv`: the run's fields (schedule index, prompt digest,
    requested model and setting, reported model, client version, start and
    end time, calls, outcome, usage, the command, the digests of the
    completion and the solution, the grader's line);
  - `calls.tsv` and `call-<n>.tsv`: every call with its times, exit code,
    outcome, reported model, usage, and, for Codex, the context audit;
  - `call-<n>.stdout.txt` and `call-<n>.stderr.txt`: the whole output of
    the client, byte for byte;
  - `completion.txt`, `solution.vib`, `grade.txt`: the completion text, the
    solution extracted from it, and the grader's line.
- [`../runs.tsv`](../runs.tsv): one line per scheduled run, in schedule
  order.

The Codex client's events name no model, so its runs record the reported
model as `notReported`; each of its calls requested `gpt-6-luna` and its
context audit held exactly the system prompt and the prompt.
