# Step 27: review-edits

Kind: evidence. The adversarial review of the edit engine: an independent
model reads the code of steps 07, 11, 14, 19, 20, 21, and 24 against the
chapter and rows I7 to I11, I13, I17, and tries to make the tool write
what it must not, write when it must not, or report applied what is not.
It changes no code.
Depends on: 19, 20, 21, 24.
Crates: none.

## Prerequisites

- Read: [05 Transactional edit plans](../../../../spec/05-tooling.md#transactional-edit-plans);
  [03 Filesystem policies](../../../../spec/03-effects.md#filesystem-policies) rules 1 to 6;
  [architecture](../architecture.md) section 2 and 3; [questions](../spec-questions.md)
  Q1, Q10, Q12 to Q22; [review-drivers](../../m2/evidence/review-drivers.md)
  as the model of a review with probes on a real filesystem.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fs-scope/src/lib.rs:248` | `pub fn replace_contents` | the one write primitive |

The files of steps 07, 11, 14, 19, 20, 21, 24:
`crates/vibra-workspace/src/edit/`, `crates/vibra-cli/src/edit.rs`,
`lint.rs`.

## Files owned

- `docs/roadmap/v0.0.2/m3/evidence/review-edits.md`

## Ordered tasks

1. The reviewer is a model other than the one that wrote the seven steps,
   with the chapters, this plan, and the merged tree, on a Windows machine
   with Developer Mode, 8.3 names, and a case-sensitive directory.
2. Probes it must try, at least: a plan document whose path differs by
   case from the real name; a junction placed between the capture and the
   publish; a journal written by hand that names `project.vibon`, a file
   under `dep/`, or a path with `..`; a crash injected after the journal is
   flushed, after the first replacement, after the last replacement, and
   before the journal is removed; a journal left by a plan of another
   revision; two commands racing on one journal; a replacement whose
   `fingerprint` matches but whose document revision does not, and the
   reverse; a protected module reached through a rename's reference edit;
   an `edit replace` whose `DATA` holds a `use` of a `@tests` module from
   a target; a rename to a name that collides only in `@tests`; a fix
   whose span moved because an earlier fix of the same `lint` plan changed
   the file; an envelope just over and just under the output bound; a
   plan whose `resultingDiagnostics` holds a warning only (it must apply).
3. Each finding with a probe, the observed behaviour, the sentence it
   contradicts, a severity, and the file.
4. Write `evidence/review-edits.md`.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every probe answered | positive | the probe list | each holds, fails, or is not run with a reason | execution model |
| every invariant read | positive | rows I7 to I11, I13, I17 | each has a statement | architecture, section 2 |
| the tree after each refusal | positive | each refused probe | a recursive listing and every file's bytes equal those before | 03 rule 4: "Never truncate or create before authorization" |

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

Fixing anything (28).

## Completion evidence

- [ ] `evidence/review-edits.md` lists every probe with its result and
      every finding with its severity and file.
- [ ] The report names the reviewing model, the machine's capabilities,
      and the commit reviewed.
