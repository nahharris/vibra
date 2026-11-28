# Step 33: review-breadth

Kind: evidence. The adversarial review of the commands and documents
beside the repair scenario: an independent model reads the code of steps
11, 25, 26, 27, 28, 29, 30, 31, and 32 against the chapters and rows I11,
I15, I17, I23, I24, and tries to make an evaluation reach a host, a rename
rebind a reference, a fix change a claim, an expansion disagree with the
index, or a generated reference drift. It changes no code.
Depends on: 11, 26, 27, 29, 30, 32.
Crates: none.

## Prerequisites

- Read: [05 Program arguments, execution and evaluation](../../../../spec/05-tooling.md#program-arguments-execution-and-evaluation);
  [05 Transactional edit plans](../../../../spec/05-tooling.md#transactional-edit-plans);
  [05 Index records](../../../../spec/05-tooling.md#index-records);
  [architecture](../architecture.md) section 2; [questions](../spec-questions.md)
  Q19 to Q26, Q34, Q38.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-interp/src/evaluate.rs:313` | `pub fn evaluate` | the one entry evaluation may use |

The files of the nine steps: `crates/vibra-workspace/src/eval.rs`,
`query/expansions.rs`, `query/index.rs`, `edit/rename.rs`, `edit/imports.rs`,
`edit/fix.rs`, `edit/lint.rs`, `crates/vibra-cli/src/eval.rs`, `edit/`,
`lint.rs`, `crates/vibra-diagnostics/src/registry.rs`, and
`crates/vibra-conformance/src/bin/reference/`.

## Files owned

- `docs/roadmap/v0.0.2/m3/evidence/review-breadth.md`

## Ordered tasks

1. The reviewer, with the chapters, this plan, and the merged tree.
2. Probes it must try, at least: an `eval` of a `fn` that calls a `proc`
   through a value; an `eval` whose argument decodes without its `where`;
   an `eval` of a generic through a non-generic wrapper; a tail loop that
   is not counted; a rename that rebinds a reference with the collision
   check bypassed; a rename to a name that collides only in `@tests`; a
   rename over several documents whose `--write` writes one; a protected
   module reached through a rename's reference edit; a fix whose span
   moved because an earlier fix of the same `lint` plan changed the file; a
   fix in `project.vibon` that writes; an expansion that disagrees with the
   index; an index whose `facts` has an identity that differs from
   `Identities`; an envelope just over the bound for an index and for an
   expansion; a reference example whose registered verdict the engine does
   not give; a card token the reader does not know.
3. Each finding with a probe, the observed behaviour, the sentence, a
   severity, and the file.
4. Write `evidence/review-breadth.md`.

## Test matrix

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| every probe answered | positive | the probe list | each holds, fails, or is not run with a reason | execution model |
| every invariant read | positive | rows I11, I15, I17, I23, I24 | each has a statement | architecture, section 2 |

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

Fixing anything (34); the focused lanes, which gates 19, 21, and 23 read.

## Completion evidence

- [ ] `evidence/review-breadth.md` lists every probe and finding.
- [ ] The report names the reviewing model and the commit reviewed.
