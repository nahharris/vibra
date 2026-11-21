# Step 21: test-world

Kind: vertical. Slice 6 completed: `@std.test`, the six `TestWorld` rows in
the broker, a test that takes its world, and the complete `@auditTrace.v2`
of each test in `@testRun.v2`. From this step a host operation runs from
source to trace.
Depends on: 16, 17, 19, 20.
Crates: `stdlib`, `vibra-host`, `vibra-types`, `vibra-workspace`,
`vibra-schema`, `vibra-conformance`, `conformance/cases`.

## Prerequisites

- Read: [programs
  chapter](../../../../spec/04-programs-and-packages.md#test-worlds-and-grants)
  "Tests", "Test worlds and grants"; [runtime
  chapter](../../../../spec/06-runtime.md#host-registry) the test-world
  registry paragraph, "Host world", "Audit trace"; [effects
  chapter](../../../../spec/03-effects.md#tests-and-model-profiles) "Tests
  and model profiles"; [tooling
  chapter](../../../../spec/05-tooling.md#command-payloads) `TestItem`,
  "Snapshot, claim and witness identity" (the `toolchain` record);
  [questions](../spec-questions.md) Q12, Q13, Q15, Q16, Q33, Q34.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-workspace/src/test_runner.rs:611` | `fn run_one` | builds one world, one `Context`, one trace per test |
| `crates/vibra-workspace/src/test_runner.rs:664` | `fn execute` | passes the world token and the context to `run_test` |
| `crates/vibra-types/src/check/testing.rs:101` | `test.view.takes().is_none()` | reaches `effects::procs::test_header` |
| `crates/vibra-conformance/src/manifest.rs:452` | `fn validate_host_world` | validates the case's `@hostWorld.v2` |
| `crates/vibra-schema/src/test_payload.rs:127` | `pub struct TestItem` | `auditTrace` carries the events |

Proposed: `stdlib/src/std/test.vib`; `vibra-host::test_world`
(`TestWorldRoot`, the six rows over `WorldDriver`).

## Files owned

- `stdlib/manifest.vibon`
- `stdlib/src/std/test.vib`
- `crates/vibra-host/src/test_world.rs`
- `crates/vibra-host/tests/it/test_world.rs`
- `crates/vibra-types/src/check/effects/procs.rs`
- `crates/vibra-workspace/src/test_runner.rs`
- `crates/vibra-workspace/src/lib.rs`
- `crates/vibra-workspace/src/snapshot.rs`
- `conformance/tools/revisions.ps1`
- `crates/vibra-workspace/src/revision.rs`
- `crates/vibra-schema/src/test_payload.rs`
- `crates/vibra-conformance/src/manifest.rs`
- `conformance/rules/m2-21.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `@std.test`: `TestWorld` with the six members, each
   `external @host` on its test-world row, the fixture data as labelled
   parameters with the constant defaults of the chapter.
2. `test_header`: `takes` is exactly one bare-name parameter whose type
   resolves to `@std.test.TestWorld`, else `@syntax.invalidForm` at the
   section (Q15); the test's function takes the world; a non-empty row is
   its ceiling.
3. `vibra-host::test_world`: `Context::test_world` requires a token of the
   test root (`InvalidHostValue` otherwise). `directory` makes a new
   virtual directory and issues six roots with the policies the chapter
   states; `streams` makes new streams and issues six; `environment` sets
   the world's variables and arguments and issues two (the variable grant
   covers every name, Q12); `clocks`, `random` issue grants over the
   world's one state; `output` returns the captured bytes. Every issued
   grant has origin `{kind: @testWorld}`. No `@host` event is appended.
4. The runner, per test: a fresh `WorldDriver` of the case's world or the
   default; a `Context`; the test root, issued only when the test takes a
   world; `run_test`; the trace with `target` absent.
5. The world is built only from `WorldDriver`: the runner names no system
   driver.
6. `@testRun.v2` and `TestItem.auditTrace` carry the test's events.
7. `vibra-workspace` re-exports `HostWorld`, `AuditTrace`,
   `decode_world`; the case manifest validates a `hostWorld` input with
   it.
8. Every snapshot gains the one `toolchain` record (C12): kind
   `toolchain`, identifier `@toolchain`, the canonical bytes of the
   `@toolchain.v2` document of the tooling chapter, with `stdlib` the
   digest of the embedded manifest. Invariant: no snapshot has a second
   toolchain record, and no revision is computed without one.
9. Check in `conformance/tools/revisions.ps1` (Q33). It computes the
   workspace revision of each case by the formula of the tooling chapter,
   from the case's input bytes and the toolchain record, with no code of
   the engine, since expected results are written from the specification.
   `-Write` rewrites every expected `revision` of the corpus; `-Check`
   changes nothing and exits 1 naming each case whose expected
   `revision` differs. Recompute the corpus with `-Write`. Invariant:
   every step that changes the library runs it, with `-Check` in its
   validation.

## Positions

| Rule | Positions |
| --- | --- |
| a test's `takes` | none; `{world: TestWorld}`; a free name; two parameters; another type; a labelled parameter |
| each member | default fixture; explicit fixture; called twice |
| each of the 17 operations | success; each typed error the fake world can produce |

## Test matrix

`workspaceTest`, profile `interpreterV2`, except the rows marked
`workspaceCheck`, whose profile is `staticV2` (Q34); each case states the
item results and the complete trace. Tests import what they use; `w` is
the world.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-GRANT-test-reads-note` | positive | the chapter's test "counts the lines of a note" with its `notes` module | `@test.passed`; one `@host` event `fs.readText`, `issuedBy: {kind: @testWorld}` | 04 Tests, the example |
| `V2-GRANT-test-without-world` | negative, `workspaceCheck`, `staticV2` | a test with no `takes` whose body names `TestWorld.clocks(world)` | `@name.unknownSymbol` at `world` | 04: "A test without `takes` receives no grant and can perform no host operation" |
| `V2-EFFECT-test-row-missing-test-world` | negative, `workspaceCheck`, `staticV2` | `takes {world: TestWorld}` with `performs {WallClock}`, awaiting `TestWorld.clocks(world)` | `@effect.outsideCeiling` for `TestWorld` | 04: "it MUST appear in the test's `performs` row" |
| `V2-EFFECT-test-row-missing-operation` | negative, `workspaceCheck`, `staticV2` | `performs {TestWorld}`, then `await WallClock.read(wall)` | `@effect.outsideCeiling` for `WallClock` | 03 Tests: "the test's row MUST also name whatever it then performs" |
| `V2-PROJECT-test-takes-renamed` | positive | `takes {sandbox: TestWorld}` | `@test.passed` (Q15) | 04: "A test with `takes {world: TestWorld}`" |
| `V2-PROJECT-test-takes-two` | negative, `workspaceCheck`, `staticV2` | `takes {a: TestWorld b: TestWorld}` | `@syntax.invalidForm` at the `takes` section | 01 `test` |
| `V2-PROJECT-test-takes-other-type` | negative, `workspaceCheck`, `staticV2` | `takes {world: Str}` | `@syntax.invalidForm` at the `takes` section (Q15) | the same |
| `V2-GRANT-test-directory-default` | positive | `TestWorld.directory(w)`; `DirList.entries(list root)` | `Ok([])` | 04: "`files: Dict(RelativePath Str) be [:]`" |
| `V2-GRANT-test-directory-two-files` | positive | `files: [a: "1" b: "2"]` with paths `["b.txt"]` and `["d" "a.txt"]`; list `[]` | `Ok(["b.txt" "d"])` (Q16) | 04: "A multi-name path in `files` implies its parent directories" |
| `V2-GRANT-test-directory-fresh` | positive | two `directory` calls; write through the first, read through the second | `Err(notFound)` | 04: "`directory` and `streams` make a new resource on every call" |
| `V2-GRANT-test-each-fs-operation` | positive | read bytes, read text, list, write bytes, write text, stat on one directory | each `Ok`; six events in order | 07 Required coverage 12: "Every operation, with success" |
| `V2-GRANT-test-fs-errors` | positive | `notFound`, `notAFile`, `notADirectory`, `invalidText` (a file written as bytes `[255]` then read as text) | each the typed error, each a completed event | 07 Required coverage 12: "and each typed error" |
| `V2-GRANT-test-create-and-replace` | positive | write a new file, then write it again, then read | the second content | 07 Required coverage 12: "create and replace" |
| `V2-GRANT-test-stdin-twice` | positive | `streams(w stdin: "x\ny")`; `io.readAllText` twice | `Ok("x\ny")`, then `Ok("")` (Q10) | 07 Required coverage 12: "standard input read twice" |
| `V2-GRANT-test-output-captured` | positive | write `a` to stdout text, `b` to stdout bytes, `e` to stderr; `TestWorld.output(w)` | `{stdout: "ab" stderr: "e"}` as bytes | 04: "per stream, in write order" |
| `V2-GRANT-test-output-two-stream-sets` | positive | two `streams` calls, a write through each | `output` holds both, in write order | 04: "everything written through every stream grant the test's world issued" |
| `V2-GRANT-test-variables` | positive | `environment(w variables: ["HOME": "/h"])`; read `HOME`, read `NOPE` | `Ok("/h")`; `Err(notPresent)` | 07 Required coverage 12: "present and absent variables" |
| `V2-GRANT-env-test-any-name` | positive | the same grant, read `PATH` with no such variable | `Err(notPresent)`, not `denied` (Q12) | 06 Environment |
| `V2-GRANT-test-arguments` | positive | `environment(w arguments: ["a" "--" "-b"])` | `Ok(["a" "--" "-b"])` | 07 Required coverage 12: "exact arguments" |
| `V2-GRANT-test-environment-shared` | positive | two `environment` calls, the second with other variables; read through the first grant | the second call's values | 04: "`environment` sets the world's variables and arguments on each call" |
| `V2-GRANT-test-clock-steps` | positive | world `clock: {wall: 5i64 monotonic: 10u64 step: 3u64}`; wall twice, monotonic three times through two `clocks` calls | 5, 5; 10, 13, 16 | 04: "so two calls share state"; 07 Required coverage 12: "fake clock steps" |
| `V2-GRANT-test-random-vector` | positive | world `random: {seed: 42u64}`; `generate(3)`, `generate(8)` | the vector of step 16 | 07 Required coverage 12: "random vectors" |
| `V2-GRANT-test-default-world` | positive | no world document; wall clock, `generate(1)`, arguments | `0`, the seed-0 byte, `[]` | 04: "those of the default `@hostWorld.v2` document" |
| `V2-RUNTIME-trace-order` | positive | three operations on two grants | three events, in completion order | 07 Required coverage 12: "Events are recorded once, in order" |
| `V2-RUNTIME-test-body-parent` | positive | the test body awaits a `proc` that awaits `WallClock.read` | the event's `parent` is the `proc`'s invocation; the body is invocation 1 (Q13) | 06 Invocations |
| `V2-RUNTIME-invocation-gaps` | positive | `TestWorld.clocks`, a denied read, then a successful read | one event; its `invocation` counts the earlier awaits (Q13) | 06: "Every `await` is an invocation" |
| `V2-RUNTIME-root-ordinals` | positive | `directory` then `clocks`; one event from each | `root` values in issue order (Q13) | 06: "a run-local ordinal of the root grant" |
| `V2-RUNTIME-trace-trap-no-event` | positive | a host write, then a trap | the item is `@test.trap`; the trace has exactly the one event | 07 Required coverage 12: "a trap or exhaustion invents no event" |
| `V2-RUNTIME-trace-test-world-no-event` | positive | only `TestWorld` members are awaited | an empty `events` | 04: "appends no `@host` event to the test's trace" |
| `V2-GRANT-test-worlds-isolated` | positive | two tests; the first writes a file through `directory`, the second lists a new one | both pass; the second sees `[]` | 04: "Every test runs in a fresh, deterministic, in-memory host world" |
| `ATK-test-world-op-on-leaf` | negative, host test | `Context::test_world` with a token of a directory root | `InvalidHostValue` | 04: "No binding recipe can name `TestWorld`" |
| `ATK-test-runner-no-system-driver` | negative, host test | the architecture test over `crates/vibra-workspace/src/test_runner.rs` | no reference to `SystemDriver` or `Launch::System` | 04: "no test reaches a real host resource" |
| foreign world's grant | negative, host test | a token issued by one test's context used in another's | `InvalidHostValue` | 04: "Grants from one test's world are unusable in another" |
| the toolchain record | positive, host test | a snapshot of `examples/hello/` | exactly one record of kind `toolchain`, identifier `@toolchain`, whose bytes are the canonical bytes of the chapter's `@toolchain.v2` example with `stdlib` the digest of the embedded manifest (C12) | 05: "Every snapshot has exactly one `toolchain` record, whose bytes are the canonical bytes of this document" |
| the revision follows the library | positive, host test | two snapshots of one project whose embedded manifests differ in one byte | two workspace revisions (C12) | 05: "a revision changes with the toolchain and with its embedded library even when no project byte changes" |
| the script agrees with the engine | positive, script | `revisions.ps1 -Check` over the corpus after `-Write`, and the host test's snapshot of `examples/hello/` | exit 0; the script's revision of `examples/hello/` equals the engine's (Q33) | 05: "A revision written in an expected observation, such as the `revision` of an audit trace, is computed with this record" |
| the script finds a stale revision | negative, script | one expected `revision` changed by one digit | `-Check` exits 1 and names that case (Q33) | the same |

## Diagnostics and schema changes

Codes: `@syntax.invalidForm`, `@name.unknownSymbol`,
`@effect.outsideCeiling`. Schema: `@testRun.v2` items carry
`@auditTrace.v2` events; `urn:vibra:schema:v2:command-result` `TestItem`
unchanged in shape. The manifest gains `@std.test` and the six test-world
symbols under `host`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-host -p vibra-types -p vibra-workspace -p vibra-schema -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
& conformance/tools/revisions.ps1 -Root conformance/cases -Check
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. A trace expectation that differs prints both documents;
the difference is read field by field against the chapter before either
side is changed.

## Stays unavailable

`@tool.unavailable`: a binary target's entry (25); `vibra run`.

## Excluded

Narrowing and limits under test (22); recipes under a world (28).

## Completion evidence

- [ ] `conformance/rules/m2-21.tsv` has one `active` row per `V2-` id.
- [ ] Each trace snapshot was written by hand; the report shows the
      script that computed `revision`, with the toolchain record, and the
      library `seed`.
- [ ] Every expected `revision` the corpus held before the step was
      recomputed by `conformance/tools/revisions.ps1`, and the report
      lists the files.
- [ ] `V2-PROJECT-test-unavailable-item` of M1 is rewritten or deleted,
      and the report says which.
- [ ] The validation commands exit 0.
