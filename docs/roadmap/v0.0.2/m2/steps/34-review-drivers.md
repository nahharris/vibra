# Step 34: review-drivers

Kind: evidence. The review gate of slices 4, 5, and 7: an adversarial
reading of startup, the run, the command, and the real drivers, with the
Windows adapter as its centre. It writes findings and changes no code.
Depends on: 27, 32, 33.
Crates: none.

## Prerequisites

- Steps 25 to 33 are merged.
- Read: [effects chapter](../../../../spec/03-effects.md#target-admission)
  "Target admission", "Filesystem policies"; [programs
  chapter](../../../../spec/04-programs-and-packages.md#startup-order)
  "Startup order"; [runtime
  chapter](../../../../spec/06-runtime.md#entry-outcome) "Entry outcome";
  [the feasibility record](../../m0/file-scope-feasibility.md);
  [architecture](../architecture.md) rows A17 to A26 and A31;
  the dual-run evidence, `evidence/dual-run.md`.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-fs-scope/src/windows.rs:170` | `pub fn open_root` | where a path becomes a held directory |
| `crates/vibra-cli/src/lib.rs:82` | `fn execute` | where the system launch is chosen |

It reads `crates/vibra-fs-scope/src/`, `crates/vibra-host/src/system/`,
`crates/vibra-host/src/startup.rs`, `crates/vibra-workspace/src/run.rs`,
and `crates/vibra-cli/src/run.rs`.

## Files owned

- `docs/roadmap/v0.0.2/m2/evidence/review-drivers.md`

## Ordered tasks

1. For every MUST of the sections above, name the case or host test that
   shows it, or record the gap as a finding.
2. Read every `unsafe` block of `windows.rs` against the documentation of
   the system call it makes: the access mask, the share mode, the create
   disposition, the reparse flag, the handle's lifetime.
3. Run each probe below on a Windows machine and record the result, or
   `NOT RUN` with the reason.
4. Read every path from an operating-system error to a `Completion`:
   none may turn "may have happened" into "not submitted".
5. Classify each finding: defect, question, or accepted limit; an
   accepted limit is one the chapter says a user is shown.

## Test matrix

Probes, each run from a scratch test and recorded.

| Probe | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| root through a link | negative | the `@fixedPath` itself is a junction or a symbolic link | a stated behaviour: the root is the opened directory; recorded as the limit "acquired through path rules" | 03: "Authority follows the opened directory, not its path" |
| root on a mapped or UNC volume | negative | a `@fixedPath` on a network share | `@grant.acquisitionFailed` | 04: "a directory on which the driver cannot enforce the leaf's policy" |
| mount point inside the root | negative | a volume mount point as a component | `denied` | 03: "and without crossing a mount point" |
| alternate data stream | negative | every way to reach `a.txt:secret` through names | `RelativePath` refuses each | 03 Paths |
| case-sensitive directory | boundary | a directory with per-directory case sensitivity holding `a.txt` and `A.TXT` | each name opens its own file; the real-name check holds | 03: "A name test compares exact Unicode scalars" |
| name normalization | boundary | names that differ by Unicode normalization form | compared as scalars; no folding | the same |
| long path | boundary | a path over 260 characters under the root | opens, or a typed error; never another file | 06 `invalidPath` |
| file opened elsewhere without sharing | negative | another process holds the file exclusively | a typed `FsError`, not a trap | 06: "A driver MUST NOT turn an ordinary failure into a process crash" |
| write interrupted | negative | the volume fills during a replace | a completed `other`; the bytes stay charged | 06: "A write is not transactional" |
| create race | negative | another thread creates the file between the gate and `create_new` | `denied`; the other content is kept | 06: "Authorization to create a new file requires exclusive creation" |
| startup with a failing release | boundary | a handle that fails to close | the binding is `released`; the run's result is unchanged | 06: "Release MUST NOT be presented as rollback" |
| exit paths of the run | negative | every `return`, `?`, and panic path of `run_target` and `vibra-cli::run` | each releases | 06: "On every ending of the process" |
| JSON on every ending | positive | each row of step 32, piped to a schema validator | valid | 05: "including rules that relate fields" |
| arguments that look like options | negative | `vibra run -- --format json` | the program receives both tokens | 05 Arguments |
| the fake world's blind spots | boundary | list what the dual run cannot compare | written into the evidence as its domain | 06: "their absence here is not evidence of confinement" |

## Diagnostics and schema changes

None.

## Validation

```powershell
cargo test --locked --offline -p vibra-fs-scope -p vibra-host -p vibra-workspace -p vibra-cli --all-features -- --nocapture
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
```

Read: both exit 0 on the unchanged tree; `NOT RUN` lines are counted.

## Stays unavailable

Unchanged.

## Excluded

Fixing anything; Linux and macOS adapters.

## Completion evidence

- [ ] `evidence/review-drivers.md` lists every probe with its source,
      expected and observed result, and verdict.
- [ ] Every `unsafe` block has a paragraph: the call, its flags, and why
      they give the guarantee.
- [ ] The two limits a user must be shown are stated in the words of the
      chapter.
- [ ] Every finding has a class; the report states the commit reviewed.
