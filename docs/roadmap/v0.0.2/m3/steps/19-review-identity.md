# Step 19: review-identity

Kind: evidence. The adversarial review of identity, hashing, approvals,
and protected evidence: an independent model reads the code of steps 03,
04, 09, 10, 14, and 16 against the chapters and this plan's rows I1 to I4,
I12 to I14, I20, I21, I26, I27, and writes findings. It changes no code.
Depends on: 16.
Crates: none.

## Prerequisites

- Read: [05 Snapshot, claim and witness identity](../../../../spec/05-tooling.md#snapshot-claim-and-witness-identity);
  [02 Declared types](../../../../spec/02-type-system.md#declared-types);
  [04 Decoded values and canonical bytes](../../../../spec/04-programs-and-packages.md#decoded-values-and-canonical-bytes);
  [03 Target admission](../../../../spec/03-effects.md#target-admission);
  [architecture](../architecture.md) section 2; [questions](../spec-questions.md)
  Q2 to Q11, Q40, Q41, Q43; the M2 reviews [review-rows](../../m2/evidence/review-rows.md)
  and [review-grants](../../m2/evidence/review-grants.md) as the model of
  a review.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fmt/src/lib.rs:42` | `pub fn format_document` | the text the identities hash comes from here |
| `crates/vibra-resolve/src/identity.rs:131` | `pub fn identity` | the anchor |

The files of steps 03, 04, 09, 10, 14, 16: `crates/vibra-fmt/src/fragment.rs`,
`crates/vibra-workspace/src/identity/`, `approval.rs`, `present.rs`,
`approve_write.rs`, `crates/vibra-cli/src/approve/`.

## Files owned

- `docs/roadmap/v0.0.2/m3/evidence/review-identity.md`

## Ordered tasks

1. The reviewer is a model other than the one that wrote the six steps,
   given the chapters, this plan, and the merged tree, with the
   instruction to find a program, document, or sequence of commands under
   which an identity, an approval, the pin, or the protected set behaves
   other than the chapters and the questions say.
2. Probes it must try, at least: two spellings of one meaning with equal
   hashes where the table says they differ, and different hashes where it
   says they are equal (a string escape, a number suffix, a doc string's
   indentation, a `given` order, an `impl` order, an explicit empty row);
   a member edit that leaves an owner's witness; a selected `impl` member
   edit that leaves a caller's witness; a default member edit; a bound's
   conformance edit; a dependency edit that leaves a dependent's witness;
   a change of `@std` text; a path with a space, a non-ASCII name, a
   different drive; an interface whose member or associated type is added
   with an equal claim; an approval that stays in force after a protected
   change through a case alias, a link, a second module with the same
   name, a helper module no test names, a fixture constant; a test run
   that reaches the runner without the admission hook (a selector, the
   whole suite, a run from the CLI and from the engine); a freshly written
   approval that passes `--expect-approval` with the old identity; an
   envelope that names no approval while one is in force; a `check` that
   skips `admit` on some path (a target selector, a `fmt`); a presentation
   that differs with an environment variable; a renewal that writes while
   a type error stands; an approval written by `edit … --write`; a stale
   approval whose two faults are reported twice.
3. Each finding: a probe, the observed behaviour, the sentence it
   contradicts, a severity (blocker, defect, limit, accepted), and the
   file.
4. Write `evidence/review-identity.md` in the shape of the M2 reviews.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every probe answered | positive | the probe list | each probe has a result: holds, fails, or not run with a reason | execution model, "Review and handoff evidence" |
| every invariant read | positive | rows I1 to I4, I12 to I14, I20, I21, I26, I27 | each has a statement of whether the code enforces and re-validates it as the table says | architecture, section 2 |
| vectors recomputed | positive | the claim and witness vectors | recomputed by the reviewer outside the engine (a script over the framing), equal | 05 vectors |

## Diagnostics and schema changes

None.

## Validation

```powershell
git diff --check
```

Read: prints nothing.

## Stays unavailable

Unchanged.

## Excluded

Fixing anything (20).

## Completion evidence

- [ ] `evidence/review-identity.md` lists every probe with its result and
      every finding with its severity and file.
- [ ] The report names the reviewing model and the commit reviewed.
