# Step 33: dual-run-and-attacks

Kind: vertical. Slice 7 completed: one case run against the fake world and
against the real adapter, with the results compared; and the attack cases
that need a real filesystem, run through a whole target.
Depends on: 29, 31.
Crates: `vibra-workspace`.

## Prerequisites

- Read: [architecture](../architecture.md) section 3; [effects
  chapter](../../../../spec/03-effects.md#tests-and-model-profiles) "Tests
  and model profiles", the last paragraph; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#required-coverage)
  required coverage 11; [the feasibility
  record](../../m0/file-scope-feasibility.md); [milestone
  README](../README.md) exit evidence.

## Entry points

From step 29: `run_target`, `Launch`. From step 30: `SystemInputs`. From
step 16: `HostWorld`.

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/tests/it/support.rs:1` | `//! Temporary project trees` | temporary workspaces and links |

Proposed: test-only `dual::{materialize, run_both, compare}` in
`tests/it/dual_run.rs`.

## Files owned

- `crates/vibra-workspace/tests/it/dual_run.rs`
- `crates/vibra-workspace/tests/it/attacks_run.rs`
- `docs/roadmap/v0.0.2/m2/evidence/dual-run.md`

## Ordered tasks

1. `materialize(world, dir)`: write the world's `directories` and `files`
   under a new temporary directory; return `SystemInputs` with the
   world's `stdin`, `env`, and `args`, each argument that is a virtual
   path re-rooted. Invariant: nothing outside the temporary directory is
   written.
2. `run_both(tree, world)`: the target under `Launch::World`, then under
   `Launch::System` with every `@fixedPath` re-rooted.
3. `compare`: the `@command.*` result, the application code, both
   captured streams, each binding's status, and the trace's events, their
   source origins included, with `drivers`, each `driver`, and the
   results of clock and random operations removed. A difference prints
   both sides.
4. The attack runs: each builds its objects in the temporary directory
   after `materialize` and runs the target under `Launch::System` only.
   After a run that refuses a write, the test reads the temporary
   directory with `std::fs` and compares its recursive listing and every
   file's bytes with what they were before the run. Invariant: a refusal
   is judged by the directory, never by the reply alone.
5. Write `evidence/dual-run.md`: per case, the platform, the result, and
   the statement that agreement is sampled evidence over the stated
   domain.

## Test matrix

Windows only; elsewhere every row prints `NOT RUN: no directory driver`.
`C` is the copy program: an entry taking `args`, `source: DirReadText`
(an `@argumentPath`, policy suffix `.txt`, read), `dest: DirWriteText`
(a `@fixedPath`, create and replace), and `out: StdoutText`; it reads the
file named by its second argument and writes it under `dest`. "Unchanged"
is the comparison of task 4.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| dual: copy | positive | `C`; world with `/in/a.txt`, `/out`, `args: ["/in" "a.txt"]` | both runs agree; exit 0; the destination holds the text | 03: "A comparison of a real driver against its model is sampled evidence with a stated domain" |
| dual: denied by policy | positive | the same asking for `a.bin` | both agree: the entry's code for `denied`; no read event | 06: "Policy and broker outcomes … still occur" |
| dual: not found | positive | asking for `no.txt` | both agree: `notFound`, one completed event | 06 `FsError` |
| dual: required refused | positive | `args: ["/missing" "a.txt"]` | both agree: `@grant.requiredUnavailable` | 06 Recipes under a world: "a virtual directory that is absent is a refusal" |
| dual: listing | positive | an entry listing a directory with files `b`, `A`, `a` | both agree: `["A" "a" "b"]` | 06 `DirList.entries` |
| dual: limits | positive | `source` with `limits: {inputBytes: 3u64}` and a 5-byte file | both agree: `limitExceeded` | 06 Closed operation semantics |
| dual: replace then read | positive | an entry that writes, replaces, and reads back | both agree | 06 File writes |
| dual: invalid text | positive | a bytes file `[255]` read as text | both agree: `invalidText` | 06 |
| dual: disagreement is reported | negative | `compare` given two outcomes that differ in one event | a failure naming the event's index and field | architecture, section 3 |
| `ATK-run-symlink-denied` | negative | `/in/l.txt` replaced by a symbolic link to a file outside | the entry sees `denied`; the outside content never appears in stdout or the destination | 07 Required coverage 11: "reparse points, links, mounts" |
| `ATK-run-junction-denied` | negative | `/in/sub` replaced by a junction outside; asking for `sub/a.txt` through a policy with depth `@tree` | `denied` | the same |
| `ATK-run-alias-denied` | negative | `/in/longsecretname.dat`, requested by its 8.3 short name under a policy whose `exact` names hold that short name | `denied`, a completed event | 07 Required coverage 11: "the real name of an entry" |
| `ATK-run-hard-link-read-allowed` | positive | `/in/a.txt` a hard link to a file outside | the read succeeds | 03 Hard links: "Reading such a file is allowed" |
| `ATK-run-hard-link-replace-denied` | negative | the destination file a hard link to a file outside | `denied`; the outside file and the temporary directory are unchanged | 03: "Replacing it MUST be refused with `denied`" |
| `ATK-run-denied-leaves-file` | negative | the destination exists; `dest` permits `create` only | `denied`; the temporary directory is unchanged | 03: "Never truncate or create before authorization" |
| `ATK-run-create-denied-leaves-directory` | negative | the destination directory is `out` on disk; `dest` has the policy prefix `["OUT"]` with `create`; the entry writes `["OUT" "n.txt"]` | `denied`, a completed event; the temporary directory is unchanged: `out` holds no entry | 03: "Never truncate or create before authorization" |
| `ATK-run-root-replaced` | positive | after startup (a hook between startup and entry), rename the source directory and put another at its path | the entry reads from the renamed directory | 07 Required coverage 11: "renames, and a replaced root" |
| `ATK-run-prefix-by-names` | negative | policy prefix `["reports"]`; a sibling `reportsOld/a.txt` requested | `denied` | 07 Required coverage 11: "prefix by names, not by string" |
| `ATK-run-swapped-component` | negative | a thread swaps `/in/sub` between a directory and a junction during 1,000 runs' worth of reads in one entry | every read is the inside content or `denied` | 07 Required coverage 11: "no cached intermediate handle" |
| `ATK-run-ambient-path` | negative | an entry asking for `["..", "x"]`, `C:\x`, `\\?\C:\x` as names | `RelativePath` construction fails in the program; no host call | 03 Paths: "Absolute, drive-relative, UNC, and device paths cannot be represented" |

## Diagnostics and schema changes

None.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-workspace --all-features -- --nocapture
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: exit 0. Count the `NOT RUN` lines; on a Windows machine with
Developer Mode there are none. Each is copied into the evidence file.

## Stays unavailable

Unchanged.

## Excluded

Proof of equivalence between the drivers; non-NTFS volumes; Linux and
macOS, whose adapters the native release qualifies.

## Completion evidence

- [ ] `evidence/dual-run.md` has one line per row with platform, result,
      and the domain of the evidence.
- [ ] No row is reported as passed that printed `NOT RUN`.
- [ ] The harness names `Launch::System` only in test code; the
      architecture test of step 32 allows `tests/it/dual_run.rs` and
      `tests/it/attacks_run.rs` by name.
- [ ] The validation commands exit 0.
