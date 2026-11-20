# Step 30: system-drivers

Kind: vertical. Slice 7, the portable drivers: `SystemDriver` with the
real standard streams, environment, arguments, clocks, and random source.
The directory provider has no driver yet, so a target that binds one is
unavailable under the system launch.
Depends on: 24.
Crates: `vibra-host`.

## Prerequisites

- Read: [runtime
  chapter](../../../../spec/06-runtime.md#closed-operation-semantics)
  "Closed operation semantics": All operations, Standard input, Standard
  output and error, Environment, Clocks and randomness; [programs
  chapter](../../../../spec/04-programs-and-packages.md#startup-order) the
  refusal and failure table; [tooling
  chapter](../../../../spec/05-tooling.md#program-arguments-execution-and-evaluation)
  Arguments; [questions](../spec-questions.md) Q10, Q24.

## Entry points

From step 15: `driver::{Driver, Gate, Found, Completion, Acquire,
DriverIdentity}`.

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-conformance/tests/it/ambient_reads.rs:53` | `AMBIENT_PATTERNS` | the patterns this module alone may contain |

Proposed: `vibra-host::system::{SystemDriver, SystemInputs}`, with the
adapters `streams`, `env`, `clock`, `random` as submodules.

## Files owned

- `crates/vibra-host/src/system.rs`
- `crates/vibra-host/src/system/streams.rs`
- `crates/vibra-host/src/system/env.rs`
- `crates/vibra-host/src/system/clock.rs`
- `crates/vibra-host/src/system/random.rs`
- `crates/vibra-host/tests/it/system.rs`

## Ordered tasks

1. `SystemDriver::new(inputs)`: `inputs` holds the program arguments as
   `OsString`s and, for tests, replacements for the three streams and the
   variable set. The default reads the process. Invariant: nothing is
   read at construction.
2. `identity`: `{provider, version: "0.0.2"}` for the eight providers of
   this step (Q24); `None` for `@host.fs.directory` until step 31.
3. `acquire`: a stream the process does not have is `Refused`; any other
   error obtaining it is `Failed`; the others acquire, or fail when the
   operating system cannot provide the resource.
4. Streams: stdin reads to the end through the gate's size reservation
   as Q10 recommends; a second read is the empty value; undecodable text
   is `invalidText`; a read error is `other`. Writes deliver the whole
   value; a closed stream is `closed`; a failure after a possible partial
   write is reported as completed with `other`, never as not submitted.
5. Environment: a name is read exactly; absent, empty, containing `=` or
   NUL is `notPresent`; not Unicode is `notUnicode`. Arguments in order,
   without the executable; one that is not Unicode is `notUnicode`.
6. Clocks: wall in nanoseconds since the Unix epoch as `I64`; monotonic
   from an origin fixed at construction, never decreasing; a value that
   does not fit is `unavailable`.
7. Random: exactly `count` bytes from the operating system's generator,
   reserved against `inputBytes` before generation; no source is
   `unavailable`.
8. No adapter calls an exit function, aborts, or panics on an operating
   system error.

## Test matrix

Host tests through `Context` over a `SystemDriver` built from
`SystemInputs` with in-memory streams, a fixed variable set, and fixed
arguments, so every row runs on the three platforms; the two rows marked
"process" read the real process.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| stdin text | positive | stdin `x\ny`; `io.stdinText` twice | `Ok("x\ny")`, `Ok("")` | 06 Standard input: "a second completed read returns the empty value" |
| stdin invalid text | negative | stdin `[255]`; `io.stdinText` | `Err(invalidText)` | 06: "Text that cannot be decoded is `invalidText`" |
| stdin read error | negative | a reader that fails | `Err(other)` | 06: "Input that cannot be read is `other`" |
| stdin over the limit | negative | stdin `abc`, `inputBytes: 2` | `Err(limitExceeded)`; a later unbounded read returns `abc` (Q10) | 06: "A rejection before dispatch … consumes nothing" |
| stdout whole value | positive | `io.stdoutText "a"`, `io.stdoutBytes [98]` | the sink holds `ab` | 06: "A write delivers its whole value, in invocation order" |
| stdout closed | negative | a sink that reports a broken pipe | `Err(closed)`, completed | 06: "A closed stream is `closed`" |
| stdout partial failure | negative | a sink that accepts 2 bytes of 5 and then fails | `Err(other)`, a completed event; the 5 bytes stay charged | 06: "The outcome MUST still say accurately whether the write completed or was submitted" |
| stream absent | negative | `acquire` of `@host.io.stdin` with no stdin in `inputs` | `Refused` | 04: "the process has no such stream" |
| variable present | positive | `HOME=/h`; `env.readVar "HOME"` | `Ok("/h")` | 06 Environment |
| variable absent | negative | `env.readVar "NOPE"` (in `names`) | `Err(notPresent)` | 06: "A missing name is `notPresent`" |
| variable name unholdable | negative | `""`, `"A=B"`, a name with NUL | `Err(notPresent)` | 06: "So is a name no environment can hold" |
| variable not Unicode | negative | a value of invalid UTF-16 or bytes, by platform | `Err(notUnicode)` | 06: "A value that is not Unicode is `notUnicode`" |
| arguments exact | positive | arguments `a`, `--`, `-b`, the empty string | `Ok(["a" "--" "-b" ""])` | 05: "including another `--` or a token that begins with `-`" |
| argument not Unicode | negative | one argument of invalid encoding | `Err(notUnicode)` | 05: "An argument that is not Unicode is that operation's typed error" |
| wall clock, process | positive | `time.nowWall` | a value after 2020 and before 2100 in nanoseconds | 06: "nanoseconds since the Unix epoch in UTC" |
| monotonic clock, process | positive | `time.nowMonotonic` 1,000 times | never decreases | 06: "never decreases within it" |
| random count | positive | `generateBytes(0)`, `(1)`, `(4096)` | exactly that many bytes | 06: "returns exactly `count` bytes" |
| random over the limit | negative | `inputBytes: 8`, `generateBytes(9)` | `Err(limitExceeded)` | 03 Limits: "and random bytes" |
| random huge count | negative | `generateBytes(u64::MAX)` with no limit | a typed error or the memory host event; no allocation of that size is attempted first | 06: "A driver MUST NOT read, allocate, or send an unbounded payload first" |
| identities | positive | `identity` of the eight providers | each `version: "0.0.2"` (Q24) | 06 Audit trace, `drivers` |
| `ATK-driver-absent-at-admission` | negative | `startup::admit` of a plan with a `@host.fs.directory` leaf over this driver | "unavailable"; nothing acquired | 04: "A driver the platform does not support at all … is rejected at admission" |
| `ATK-no-process-exit` | negative | an architecture test over `crates/vibra-host/src` for `process::exit`, `process::abort`, `panic!` outside tests, and `unwrap()` on an operating-system result | none | 03: "No operation returns `Never`, and none terminates the process" |
| ambient reads confined | positive | the ambient-reads test | references only under `src/system.rs` and `src/system/` | 03 Exclusions: "ambient authority" |

## Diagnostics and schema changes

None.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-host -p vibra-workspace -p vibra-conformance --all-features
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0, on Windows locally and on the three platforms in CI.

## Stays unavailable

`@tool.unavailable` at admission: a target that binds
`@host.fs.directory`, on every platform until step 31 and on Linux and
macOS after it.

## Excluded

Directories (31); the command (32).

## Completion evidence

- [ ] Each of the 23 rows is a test, named in the report.
- [ ] The crate has no `unsafe`.
- [ ] CI is green on the three platforms.
- [ ] The validation commands exit 0.
