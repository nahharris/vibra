# Step 21: review-edits

Kind: evidence. The adversarial review of the focused edit lane: an
independent model reads the code of steps 02, 07, 13, and 15 against the
chapters and rows I6 to I11, I13, I17, I28, and tries to make the tool
write what it must not, write when it must not, write under a name that
refers to a partial document, or report applied what is not. It changes no
code.
Depends on: 15.
Crates: none.

## Prerequisites

- Read: [05 Transactional edit plans](../../../../spec/05-tooling.md#transactional-edit-plans);
  [03 Filesystem policies](../../../../spec/03-effects.md#filesystem-policies) rules 1 to 6;
  [architecture](../architecture.md) sections 2 and 3; [questions](../spec-questions.md)
  Q1, Q10, Q12 to Q18, Q42; [review-drivers](../../m2/evidence/review-drivers.md)
  as the model of a review with probes on a real filesystem.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fs-scope/src/windows.rs:366` | `pub fn create_new` | the one creation primitive |

The files of steps 02, 07, 13, 15: `crates/vibra-fs-scope/src/`,
`crates/vibra-workspace/src/edit/plan.rs`, `body.rs`, `apply.rs`,
`format_plan.rs`, `crates/vibra-cli/src/edit/body.rs`, `write.rs`,
`fmt.rs`.

## Files owned

- `docs/roadmap/v0.0.2/m3/evidence/review-edits.md`

## Ordered tasks

1. The reviewer is a model other than the one that wrote the four steps,
   with the chapters, this plan, and the merged tree, on a Windows machine
   with Developer Mode, 8.3 names, and a case-sensitive directory.
2. Probes it must try, at least: a plan document whose path differs by
   case from the real name; a junction placed between the first capture
   and the lock, and between the second capture and the rename; a leftover
   reserved file that is a link, a directory, or a file with two names; a
   lock left by a crash, a lock that is a link, a lock created between the
   writer's check and its `create_new`; a failure injected at every point
   before the rename, and a reader holding the destination across the
   rename; a destination with two hard links; an unedited imported module
   changed between the first capture and the lock, and between the lock
   and the second capture; two writers planned from one revision, run in
   both orders; a two-document plan whose `--write` creates anything; a
   body text that closes the body early, that is a section, that holds a
   second declaration, that holds a `use`; a listed `use` that shadows or
   collides, and one that changes a resolution with the collision check
   bypassed; a body edit whose header bytes change; a protected module
   reached as a body subject under an approval in force and with none; a
   `fmt --write` of a protected module, of `project.vibon`, of a file under
   `dep/`; an `approve --write` reached through a plan; an envelope just
   over and just under the output bound for an applied edit; a plan whose
   `resultingDiagnostics` holds a warning only (it must apply); a
   read-only command run with a leftover reserved file and a stale lock
   present, compared byte for byte.
3. Each finding with a probe, the observed behaviour, the sentence it
   contradicts, a severity, and the file.
4. Write `evidence/review-edits.md`.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every probe answered | positive | the probe list | each holds, fails, or is not run with a reason | execution model |
| every invariant read | positive | rows I6 to I11, I13, I17, I28 | each has a statement | architecture, section 2 |
| the tree after each refusal | positive | each refused probe | a recursive listing and every file's bytes equal those before | 03 rule 4: "Never truncate or create before authorization" |
| the assumption named | positive | the review's statement on the rename | it separates what holds by construction, what the documentation says, and what the release assumes | architecture, section 3 |

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

Fixing anything (22); rename, imports, fix, and lint, which gate 33 reads.

## Completion evidence

- [ ] `evidence/review-edits.md` lists every probe with its result and
      every finding with its severity and file.
- [ ] The report names the reviewing model, the machine's capabilities,
      and the commit reviewed.
