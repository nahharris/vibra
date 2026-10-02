# M3 review fixes

After Step 18 an adversarial review of the `m3` → `main` pull request ran
seven scoped reviewers over the milestone. Every finding listed here was
reproduced with the toolchain before it was accepted. Steps 19 onward fix
them; each fix lands with a corpus case that fails without it.

These steps correct the implementation against the specification. Where the
specification itself changes, the step names the ledger row.

| Step | Fixes | Cases |
| --- | --- | --- |
| 19 | Runtime shapes. A payload slot instantiated to `void` had two runtime shapes, so generic code over `(result void e)` trapped and `try` over a nullary success failed; a tail-position call whose result is widened failed at run time; a tuple-bodied `deftype` key ignored its own `ordered`; a binder on a `void` `enumof` payload was an internal error; a `void` operand that fixes an open payload was accepted | `V1-RUNTIME-void-payload-shapes`, `V1-RUNTIME-widened-tail-call`, `V1-RUNTIME-tuple-user-key`, `V1-TYPE-NOMINAL-void-payload-operand` |
