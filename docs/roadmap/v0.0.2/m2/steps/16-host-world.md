# Step 16: host-world

Kind: vertical. Slice 6, the fake world: the `@hostWorld.v2` document and
`WorldDriver`, the deterministic driver of the 17 operations and of recipes
resolved under a world.
Depends on: 15.
Crates: `vibra-host`.

## Prerequisites

- Read: [runtime chapter](../../../../spec/06-runtime.md#host-world) "Host
  world" whole, "Closed operation semantics" whole;
  [architecture](../architecture.md) "The gate", "Settlement", "The
  order of a mutating call"; [questions](../spec-questions.md) Q10, Q12,
  Q24.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-host/src/random.rs:13` | `pub struct SplitMix64` | the world's random source |
| `crates/vibra-syntax/src/schema.rs:55` | `pub enum Slot` | the schema of the document |

From step 15: `driver::{Driver, Gate, Found, Completion, Progress,
Measured, Acquire}`.

Proposed: `vibra-host::world::{HostWorld, WORLD_FORMAT, decode_world,
WorldDriver, VirtualFs}`.

## Files owned

- `crates/vibra-host/src/world.rs`
- `crates/vibra-host/tests/it/world.rs`

## Ordered tasks

1. `decode_world`: the closed schema, fields in the chapter's order, every
   field but `format` optional with its default; `clock` and `random`
   under the nested-record rule; path validation (canonical, rooted, no
   file and directory conflict, implied ancestors). Invariant: decoding
   yields data and never a grant.
2. `WorldDriver::new(world)` copies the world. Every instance is
   independent. Its `identity` is `{provider: @hostWorld.v2 version:
   "0.0.2"}` for every provider (Q24).
3. `acquire`: `@fixedPath` as a canonical virtual path, a relative one
   under `/`; an absent directory or a file there is `Refused`; streams,
   clocks, random, arguments, and variables always acquire.
4. `call`, per row, with the gate consulted before any content is
   produced and before the copy changes. A gate refusal returns
   `Completion::Refused`. Every completion carries `Known` progress: the
   world measures every byte.
   - files: `notFound`, `notAFile`, `notADirectory`, `invalidText`;
     `links` is always `Some(1)` and `real_names` the requested names;
   - a write of an existing file asks the gate with `Replace`, then
     replaces in the copy; a write of an absent file whose parent exists
     asks the gate with `Create` on the parent's names and the leaf, then
     inserts; it never makes a parent directory. Invariant: a refused
     write leaves the copy equal to what it was;
   - `entries`: names sorted by Unicode scalar;
   - `stat`: kind and size, zero for a directory;
   - stdin: the remaining input is of known size, which the driver
     reports to the gate as `size` before it yields a byte; a read yields
     the remaining input, then the empty value. How a read is bounded is
     Q10;
   - stdout and stderr: appended to per-stream buffers;
   - variables and arguments from the world; a name no environment can
     hold is `notPresent`;
   - wall: always `clock.wall`; monotonic: first `clock.monotonic`, then
     plus `step`, `unavailable` on overflow;
   - random: SplitMix64 as the chapter states.
5. `WorldDriver::make_directory(files)`, `make_streams(stdin)`,
   `set_environment(vars, args)`, `output()`: the primitives step 21
   builds `TestWorld` from.
6. A driver error is always a `Completion`; nothing here returns a
   `HostFault` or panics on world content.

## Test matrix

Host tests through `Context` over a `WorldDriver`, with root grants issued
by a test-only helper. The world `W` is the chapter's example document. A
row marked *awaits Q10* is written as option (a) of that question gives
it, and is not implemented and not counted before the ruling; under that
option every other stdin row names a `max` and is otherwise the same.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| decode the example | positive | `W` | decodes; `/notes/a.txt` is `hello` | 06 Host world, the example |
| decode the minimum | positive | `{format: @hostWorld.v2}` | the default world: a root directory and nothing else | 06: "The default world has a root directory and nothing else" |
| unknown field | negative | `W` with `network: []` | `@data.invalidShape` | 06: "An unknown field or an invalid value is rejected" |
| version 1 | negative | `format: @hostWorld.v1` | rejected | the same |
| `clock` with no member | negative | `clock: {}` | `@data.invalidShape` | 06: "A `clock` or a `random` with no member is the empty tuple and `@data.invalidShape`" |
| `clock` with one member | positive | `clock: {step: 1000u64}` | `wall` 0, `monotonic` 0 | 06: "leaves `wall` and `monotonic` at zero" |
| unsuffixed integer | negative | `random: {seed: 42}` | data decoding fails | 06: "Every integer literal in a world carries its exact width suffix" |
| path not canonical | negative | `files: ["/a/../b": "x"]` | invalid data | 06 Paths: "no empty, `.`, or `..` component" |
| file and directory conflict | negative | `files: ["/a": "x" "/a/b": "y"]` | invalid data | 06: "including an ancestor used as both" |
| root as a file | negative | `files: ["/": "x"]` | invalid data | 06: "The root cannot be a file" |
| implied ancestors | positive | `files: ["/a/b/c.txt": "x"]`; stat of `["a" "b"]` under root `/` | directory, 0 | 06: "Missing ancestor directories are implied" |
| read text | positive | `fs.readText` `["a.txt"]` under `/notes` | `Ok("hello")` | 06 Host registry, `fs.readText` |
| read invalid text | negative | `fs.readText` `["raw.bin"]` | `Err(invalidText)` | 06: "invalid bytes are `invalidText`" |
| read bytes | positive | `fs.readFile` `["raw.bin"]` | `Ok([0 255])` | 06: "Byte operations never inspect contents" |
| absent file | negative | `fs.readText` `["no.txt"]` | `Err(notFound)` | 06 Behavior: "An absent file is `notFound`" |
| read a directory | negative | `fs.readText` `[]` | `Err(notAFile)` | 06: "a directory … where a file is required as `notAFile`" |
| list | positive | files `b`, `A`, `a` under `/notes` | `["A" "a" "b"]` | 06: "sorted by Unicode scalar" |
| list a file | negative | `fs.readEntries` `["a.txt"]` | `Err(notADirectory)` | 06 |
| stat | positive | file, then directory | `{file 5}`, `{directory 0}` | 06 `DirStat.stat` |
| write creates | positive | `fs.writeText` `["new.txt"]` | `Ok`; a read returns it; a second `WorldDriver` of `W` does not have it | 06: "A write changes only the copy" |
| write replaces | positive | `fs.writeText` `["a.txt"]` `"x"` | a read returns `x` | 06 File writes: "create or replace the whole contents" |
| write without a parent | negative | `fs.writeText` `["no" "a.txt"]` | `Err(notFound)` | 06: "do not create parent directories" |
| a refused create leaves the copy | negative | policy `actions: [@replace]`; `fs.writeText` `["new.txt"]` through a gate that sees `Create` | `Err(denied)`; a listing of `/notes` through an unrestricted grant equals the listing before | 03: "Never truncate or create before authorization" |
| a refused replace leaves the copy | negative | policy `actions: [@create]`; `fs.writeText` `["a.txt"]` `"x"` | `Err(denied)`; a read returns `hello` | the same |
| stdin once | positive | `io.stdinText` twice | `Ok("x\ny")` then `Ok("")` | 06 Standard input: "a second completed read returns the empty value" |
| stdin invalid text | negative | `stdin` a bytes record `[255]`; `io.stdinText` | `Err(invalidText)`; a second read is empty | 06: "After a completed success or domain failure the position is at the end" |
| stdin reports its size | boundary | `io.stdinText` on `W` through a gate-recording wrapper | the gate saw `size: Some(3)` before the driver yielded a byte | 06: "A read first learns the size of the opened object, reserves it" |
| stdin by count | positive, *awaits Q10* | `io.stdinText` with `max: 2` three times on `W` | `Ok("x\n")`, `Ok("y")`, `Ok("")` | 06 Standard input: "a second completed read returns the empty value" |
| stdout order | positive | `io.stdoutText "a"`, `io.stdoutBytes [98]`, `io.stderrText "e"` | `output()` is `ab` and `e` | 06: "A write delivers its whole value, in invocation order" |
| variable present, absent | positive, negative | `env.readVar "HOME"`; `"NOPE"` with both in `names` | `Ok("/home/vibra")`; `Err(notPresent)` | 06 Environment |
| unholdable name | negative | `env.readVar ""` and `"A=B"`, both in `names` | `Err(notPresent)` | 06: "So is a name no environment can hold" |
| arguments | positive | `env.readArgs` | `Ok(["/notes"])` | 06: "in their original order" |
| wall clock | positive | `time.nowWall` twice | `1700000000000000000` both times | 06: "always returns `clock.wall`" |
| monotonic clock | positive | `time.nowMonotonic` three times | 0, 1000, 2000 | 06: "Each later one adds `step`" |
| monotonic overflow | negative | `monotonic: 18446744073709551615u64 step: 1u64`, two reads | the value, then `Err(unavailable)` | 06: "returns `unavailable` if the `U64` would overflow" |
| random vector | positive | seed 42; `generateBytes(3)`, then `(8)` | the bytes computed by hand in the test from SplitMix64, one output then one more | 06: "consumes ceil(count / 8) outputs … discards the unused tail" |
| random zero | boundary | `generateBytes(0)` then `(8)` | empty; then the first output | 06: "A count of zero consumes nothing" |
| fixed path absent | negative | `acquire` of `@fixedPath "/missing"` | `Refused` | 06 Recipes under a world: "a virtual directory that is absent is a refusal" |
| fixed path relative | positive | `@fixedPath "notes"` | acquires `/notes` | 06: "with a relative one taken under `/`" |
| the world's identity | positive | `WorldDriver::identity` of each of the nine providers | `{provider: @hostWorld.v2 version: "0.0.2"}` each time | 06: "`provider` is `@hostWorld.v2` for a fake world's driver" |
| progress is measured | positive | a read of 5 bytes and a write of 3, through a completion-recording wrapper | `input: Known(5)`; `output: Known(3)` | 03 Limits: "charges actual progress" |
| `ATK-driver-error-not-fault` | negative | every error row above | each is a `Completion`, none a `HostFault` | 06: "A driver MUST NOT turn an ordinary failure into a process crash" |
| `ATK-no-redispatch-on-error` | negative | a read that returns `notFound`, counted by a wrapper driver | exactly one `call` | 03 Delivery: "The runtime never dispatches one invocation twice" |
| gate before content | negative | policy `suffix(".txt")`, read `raw.bin` through a gate-recording wrapper | denied before step 3 ends; the gate is not called; the virtual file is not read | 06 Host calls, step 3 |

## Diagnostics and schema changes

Codes: `@data.invalidShape`, `@data.invalidValue` for a rejected document.
Schema: `@hostWorld.v2`, new in `vibra-host`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-host -p vibra-workspace --all-features
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

Everything observable until steps 20 and 21.

## Excluded

Links, races, permission failures, and fault injection: "This world has no
links"; their evidence is steps 13, 31, and 33.

## Completion evidence

- [ ] Each of the 43 rows that await no ruling is a test, named in the
      report; the row marked *awaits Q10* is listed as open, or as a test
      once the question is ruled.
- [ ] The random vector is derived in the report from the SplitMix64
      definition, not from the implementation's output.
- [ ] `world.rs` names no item of `std::fs`, `std::env`, or `std::time`.
- [ ] The validation commands exit 0.
