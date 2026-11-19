# X1 experimental harness

Python 3.14 standard library only. Run commands below from this directory in
PowerShell. This is a frozen experiment subset, not the active Vibra toolchain.

```powershell
python -B -m unittest discover -v
python -B validate.py --tests-dir '<external-hidden-test-directory>'
python -B runner.py --manifest reference-manifest.json --tests-dir '<external-hidden-test-directory>' --output reference-results.json
python -B luna_runner.py --out luna-setup/audit --audit-only
python -B luna_runner.py --out luna-setup/probe
```

The reference manifest points at public reference sources; fixtures remain at
the explicit external directory. Normal results contain case identifiers and
outcomes, not hidden arguments or expected values. All assigned trials remain
in the denominator. Instruction-following checks are a separate secondary
column. No participant source is passed to Python eval/exec.

Interfaces and canonical nodes are in [CONTRACT.md](CONTRACT.md). The grader's
entry points are `parsers.parse`, `checker.check`, `evaluator.evaluate`, and
`runner.run_manifest`. Each surface has its own grammar parser, producing the
same dictionaries. Diagnostics distinguish extraction, parse, check, examples,
and runtime. The harness checks types, scope, exhaustive cases and return paths;
it does not prove termination or general refinements. Executable contracts and
refinements are runtime guards. The evaluator meters fuel and limits integer,
collection and text sizes; host recursion exhaustion is a resource fault.

## Validation

[measured] The four card examples produce exactly the same canonical tree. All
four references for each task also match; T10 prefix and suffix bytes reconstruct
T8 exactly. The first complete reference grading passed 40/40 programs and
1,764/1,764 case-runs (441 distinct fixtures, four renderings each). These are
reference/harness checks, not model-writing results. See
[reference-results.json](reference-results.json) and [validation.json](validation.json).

## Luna isolation

[measured] `luna_runner.py` uses a process per prompt, `gpt-6-luna`, effort low,
ephemeral sessions, ignored user config/rules, disabled project instructions,
memory, skills, apps, plugins, MCP and builtin tools. The local model catalog is
copied from the installed CLI's Luna metadata with only its instructions/tool
exposure fields changed. Removing its default code-mode and multi-agent modes
is essential: feature flags alone left tools and thousands of instruction
characters. No credentials were copied into the repository.

[measured] The successful non-task setup probe is in
[luna-setup/probe2/response.json](luna-setup/probe2/response.json), with the
inspected two-message, zero-tool context in
[prompt-audit.json](luna-setup/probe2/prompt-audit.json). It returned ISOLATION_OK
using 707 input, 8 output, and 0 reasoning-output tokens. This is not a scored
trial. The fixed system line matches the peer runner's participant instruction.

[cited] The installed CLI help documents `--ignore-user-config`, `--ephemeral`,
and `--ignore-rules`; the official [configuration schema](https://developers.openai.com/codex/config-schema.json)
documents instruction/context/skill controls. The installed bundled model
catalog exposes Luna's `low` setting. The prompt dump was checked directly;
tool absence was not inferred merely from the model declining to call tools.

[argued] No verified CLI setting for a hard 4,096 completion-token request cap
was found. Metadata therefore says `output_cap_enforced: false`. Do not compare
this runner's output against Haiku as an equally enforced token-budget cohort
without a recorded preregistration amendment or a verified cap mechanism.
Exact provider snapshot identity is also unavailable when the CLI reports only
the requested model alias; preserve that uncertainty.

For a future frozen single-task prompt, use `--prompt-file <path> --out
<trial-directory>`. The setup code does not schedule or launch scored cohorts.
Freeze prompts and all artifacts before those calls; keep transport failures
and usage metadata. [Peer artifact audit](peer-artifact-audit.md) records the
independent X6 key audit and the pending Haiku-runner provenance fixes.

Those provenance fixes were completed before the scored freeze. The paragraph
above records the initial setup boundary; scored cohort outcomes are now in
[r3-codex.md](../../../r3-codex.md).

## Frozen cohorts and analysis amendment

The peer accepted the budget amendment: score a trial as budget eligible only
when its provider aggregate output count is present, valid and at most 4,096.
Reasoning is already included and is not added again. Preserve raw functional
success separately. Generation caps remain asymmetric. `budget.py` implements
this analysis rule; `readers.py` applies it to X6 answers.

`run_luna.py --write-freeze` pins harness sources and shared producers in
`freeze.json`; the peer public-artifact freeze is `../freeze.json`. Both must
match before `run_luna.py --mode writing` (120 assignments) or `--mode readers`
(20 sessions) dispatches calls. Prompt/prefix bytes are immutable, every attempt
is retained, and at most two transport attempts are permitted across restarts.
Wrong or malformed completed answers are scored without repair. All 69 unit
tests and the 40 reference programs pass before this freeze.
