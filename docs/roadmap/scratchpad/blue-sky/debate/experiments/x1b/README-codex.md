# X1b parser checkpoint

Status: both 81-trial cohorts complete and graded. The registered primary rule
retains K; the cap-sensitive raw K3 advantage is reported in
[report-codex.md](report-codex.md). Earlier setup evidence follows.

[measured] `surface_parser.py` parses K, K2 and K3 into the frozen X1 common
tree without editing any original source. K2/K3 are direct recursive-descent
parsers over the original lexer; no response text is transformed. The fresh K
wrapper changes only the registered symmetric rejection of an explicitly
empty `takes` section. Shared type checking, examples and evaluation are
unchanged.

[measured] Test-first record: the card/reference golden was run before the
parser existed and failed with `X1b parser is not implemented`; the subsequent
implementation now passes all thirteen tests in `test_surface_parser.py`. Every card
example and the 27 task reference programs produce the exact frozen K trees.
An independently hand-written same-typed parameter/field-order golden also
checks the concrete result. Nested closers, value conditionals, options,
wrappers, `try`, negative literals, data punctuation, mixed syntax, forbidden
anonymous/brace values, duplicate fields and empty parameter sections have
focused positive/negative controls.

[measured] `validation.json` records the reference source and private fixture
hashes. All 27 programs pass shared checking/examples and all 1,116 case-runs
over the external T1–T9 fixtures. These are reference validations, not model
success counts. Original X1 public/source freezes still match. The external
test values were not copied here or supplied to any participant.

[argued] `preregistration.md` is the accepted three-package registration; the
old `../k2/preregistration.md` is retained as a superseded draft. Its decision
rule and common surface-neutral API are accepted. The updated card says
whitespace is required **between successive items**, not between an opening
delimiter and its first negative argument. `f(-1)` / `[-1 -2]` are valid;
`[1-2]` is not. No tuple/dictionary/anonymous-record feature is introduced.

Run the public parser controls with:

```powershell
python -B -m unittest test_surface_parser -q
```

## Frozen producer/grader checkpoint

[measured] The updated cards, surface-neutral API and peer hand-written golden
are checked by the new suite. The original 69 tests also pass. The full
reference manifest now runs through `grader.py` (derived from frozen X1 with
surface/parser/task dispatch changes and explicit shared-helper imports),
passing 27/27 references and 1,116/1,116 cases. Tests preserve failed-assignment
denominators and the separate structural column. `validate.py --tests-dir`
reproduces the checks and compares all private fixture hashes to old X1.

[measured] Standalone Luna and Haiku dry runs match across all 81 assignments
and prompt lengths; the first three are k3-t4-r1, k3-t4-r2, k-t6-r2. Both use
the same prompt builder and seed 2026111502. The new Haiku producer reserves
attempts before dispatch and recovers saved successes; focused interruption
tests prevent accidental participant calls. Luna reuses the original bounded,
write-once transport and zero-tool isolation, not the old assignment schedule.

[argued] Source and public freezes are separate: `study.py --write-freeze`
pins new sources, registration, cards/references/golden and every imported
original helper in `freeze-local.json`; the peer's `freeze.json` pins the 33
public artifacts. `study.py` verifies both and old X1 before dispatch.
`grade.py --manifest ... --tests-dir ...` additionally checks private hashes,
writes functional results and applies the unchanged aggregate-output policy.
`summarize.py` implements the accepted four-step choice and task-cluster
uncertainty. Synthetic selection checks precede model calls. No source changes
are permitted after freezing without a separately recorded amendment.
