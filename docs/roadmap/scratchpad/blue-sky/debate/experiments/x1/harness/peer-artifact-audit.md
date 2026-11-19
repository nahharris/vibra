# Codex audit of peer artifacts

Status: pre-participant audit, 2026-11-15.

## X6 keys

[argued] I independently checked all 40 questions directly against the eight
contracts, without using build.py's candidate enumeration as the uniqueness
argument. All 40 supplied letters agree. In particular c4's example is part of
the contract: at (7,5) it forces 5 and forbids 10 even though both satisfy the
postconditions alone.

[argued] The eleven C answers have complete reasons beyond bounded search:

| Questions | Uniqueness argument |
| --- | --- |
| c1-q4 | Lower and upper bounds are both 2. |
| c2-q1, c2-q5 | Exact subtraction fixes the wrapped integer, which fixes the nominal value. |
| c3-q5 | A one-element list whose sum is 7 must be [7]. |
| c4-q1 | The shown example fixes this particular input's output. |
| c4-q4 | The only multiple of 5 strictly between 5 and 15 is 10. |
| c5-q4 | True would imply the false stock condition; false is the only remaining Boolean. |
| c6-q4 | The only scalar-text sequence of length zero is empty text. |
| c7-q1, c7-q3 | The equation fixes one exact integer quotient. |
| c8-q5 | The only list of length zero is empty. |

[argued] D answers also have concrete alternatives: choose another integer in
c1's interval; redistribute c3's sum; choose the other adjacent multiple for
c4-q3; choose the other Boolean for the two permissive c5 questions; replace
c6's permitted text with a different equal-length string; or choose any other
two integers for c8. A answers violate the stated requires/type condition;
B answers fail an ensures clause or c4's example. These arguments cover the
actual frozen questions, not arbitrary future contracts.

[argued] build.py's generic fallback `no alternative in candidates => C` is not
a uniqueness proof. Keep the above per-question reasoning with the freeze and
require a new independent argument whenever contracts/questions change.

[argued] Hannah's reduced 40-question allocation is accepted to reduce burden.
Call it a balanced within-person allocation; one person with different items
per surface does not independently identify a general surface effect. Retain
the common four-surface preference item separately, record order/time/comments,
and keep the model readers' full eight-contract sessions unchanged.

## run_claude.py

[cited] Static review of [run_claude.py](../run_claude.py) found four changes needed before a
scored cohort. Its isolated command matches the peer's smoke-test account;
this audit did not repeat a language-task generation.

1. run_trial writes prompt.txt BEFORE deciding to keep response.json. On resume,
   a changed card can overwrite the recorded prompt of an old answer. Likewise
   manifest_entry recopies the current prefix. Save immutable prompt/prefix
   before the first call and reject hash mismatches on resume.
2. Failed calls are omitted from the manifest, and repeated restarts can retry
   them without the registered limit. Preserve every assigned trial, including
   exhausted transport failures, with at most one identical retry and both
   attempts retained. A timeout currently escapes future.result and may prevent
   the manifest from being written; catch and record it.
3. Freeze and verify card/API/task/reference hashes before calls. Preserve the
   exact T10 prefix in the prompt and artifact; avoid rstrip/newline translation
   making the supplied prefix differ from the assembled one.
4. Record the actual returned model identifier(s), requested effort, cap and
   finish status in the run metadata, in addition to usage and alias haiku.

[argued] These are runner/evidence problems, not syntax failures. Correct them
before the first scored call so denominators and provenance remain comparable.
