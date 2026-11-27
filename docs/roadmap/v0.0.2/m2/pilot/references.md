# Pilot references

Every reference solution of the [registration](registration.md) passes in
both arms, and every negative control fails as registered in both arms. No
model was called. The registration has no amendment.

## Toolchain

The table below was produced at the freeze commit,
`156f35cf2a7f10c8d9df37be43053ba74e941605` of `v0.0.2/m2-ops`, the commit
that adds the frozen registration. The runs used `target/debug/vibra`,
built from it with `cargo build --locked --offline -p vibra-cli --bin
vibra`. The branch `m2/39-pilot-references` that holds the runs changes no
checker, interpreter, or test-world code: its `crates`, `stdlib`, and
`Cargo.lock` are those of the freeze commit.

## Layout

[`examples/pilot/`](../../../../../examples/pilot/) holds one directory per
task:

- `statement.txt`, the statement as registered, with the registered length
  and SHA-256 digest;
- `project.vibon`, a library package `pilot` whose one target `@pilot` has
  the root `src/pilot`;
- `tests/<task>.vib`, the hidden tests of the registration's table, in its
  order and in canonical format;
- `endlet/` and `noEndlet/`, each with `reference.vib` and one file per
  control of the task, written in that arm's surface.

A `noEndlet` file is its `endlet` file with the closer of every ordinary
`let` removed. `controls.tsv` lists the 17 controls with the grade and the
code or failing test each one registers. `robustness.tsv` and
`robustness/` hold the solutions that are not source, ask for authority, or
do not end; a row with a `limit` runs with that many seconds.

## The grader

`grade.ps1 <task> <arm> <file>` grades one solution and prints one line:
the task, the arm, the grade, the closed count of a `noEndlet` run, the
result of `check` and of `test` (`stopped` for a command stopped at the time
limit, `noDocument(exit …)` with the exit code for a command that ended
without a JSON document), the diagnostic codes, the failing tests, and a
reason when the grade is not a command's own status: "an ordinary let is
closed" for an own-surface refusal, "the grader's time limit", or "the
toolchain gave no document". It follows the registration's "Grading"
steps:

1. For `noEndlet`, it reads the solution with the optional-closer grammar
   and inserts ` endlet` after the last token of the expression of every
   ordinary `let` read without one. The closed count is the number of
   ordinary `let`s read with `endlet`. A file it cannot read is placed
   unchanged with a closed count of zero. A reading that exhausts the
   reader's stack of 256 MB is a reading that failed, and is placed the
   same way.
2. It places the task's `project.vibon`, the task's hidden test module, and
   the solution at `src/pilot/<task>.vib` in a new directory under the
   system temporary directory, which it removes afterwards.
3. It runs `vibra --format json --workspace <dir> check`, then `test`.
   Each command has 120 seconds of wall-clock time. A command still running
   then is stopped with every process it started, and its status is
   `unknown`. A command that ends without a JSON document also has the
   status `unknown`.
4. A `noEndlet` run with a closed count above zero is `refuted`, with the
   reason "an ordinary let is closed". Otherwise the grade is the status of
   `check` when it is not `passed`, and the status of `test` when it is.
   The `unknown` of a stopped command carries the reason "the grader's time
   limit", and that of a command without a document the reason "the
   toolchain gave no document".

The script reads no network, writes only under the temporary directory, and
runs from any current directory. `-Vibra <path>` names the binary; the
default is the repository's `target/debug/vibra`. `-TimeLimit <seconds>`
sets the limit of each command; the default is the registered 120.

`grade.ps1 -All` first checks the task files against the registration: the
statement digests, the hidden test names in order, and the controls with
their evidence. It then grades every subject below and exits 0 only when
each one has its expected grade and evidence. A `noEndlet` subject also
needs a closed count of zero, except where the expected evidence is the
own-surface reason. Every subject runs with the registered limit of 120
seconds except `endlessTail`, which runs with 10, for time's sake. The run
takes between two and three minutes offline.

## Results

The closed count is `-` for an `endlet` run, which is not translated.

| Subject | Task | Arm | Grade | Closed | Evidence |
| --- | --- | --- | --- | --- | --- |
| `reference` | packet | `endlet` | passed | - | none |
| `reference` | packet | `noEndlet` | passed | 0 | none |
| `translationIsLocal` | packet | `noEndlet` | refuted | 1 | an ordinary let is closed |
| `packetStopIgnored` | packet | `endlet` | refuted | - | the test "the stop code scores nothing" fails |
| `packetStopIgnored` | packet | `noEndlet` | refuted | 0 | the test "the stop code scores nothing" fails |
| `reference` | pair | `endlet` | passed | - | none |
| `reference` | pair | `noEndlet` | passed | 0 | none |
| `translationIsLocal` | pair | `noEndlet` | refuted | 1 | an ordinary let is closed |
| `pairDeltaOffByDefault` | pair | `endlet` | refuted | - | 4 tests fail, among them "a reversed pair is ordered with its delta" |
| `pairDeltaOffByDefault` | pair | `noEndlet` | refuted | 0 | 4 tests fail, among them "a reversed pair is ordered with its delta" |
| `reference` | positive | `endlet` | passed | - | none |
| `reference` | positive | `noEndlet` | passed | 0 | none |
| `translationIsLocal` | positive | `noEndlet` | passed | 0 | none |
| `positiveUnchecked` | positive | `endlet` | unknown | - | `@contract.unproved` |
| `positiveUnchecked` | positive | `noEndlet` | unknown | 0 | `@contract.unproved` |
| `positiveZero` | positive | `endlet` | refuted | - | `@contract.refuted` |
| `positiveZero` | positive | `noEndlet` | refuted | 0 | `@contract.refuted` |
| `reference` | range | `endlet` | passed | - | none |
| `reference` | range | `noEndlet` | passed | 0 | none |
| `translationIsLocal` | range | `noEndlet` | refuted | 2 | an ordinary let is closed |
| `rangeUnhandled` | range | `endlet` | refuted | - | `@pattern.refutableBinding` |
| `rangeUnhandled` | range | `noEndlet` | refuted | 0 | `@pattern.refutableBinding` |
| `reference` | order | `endlet` | passed | - | none |
| `reference` | order | `noEndlet` | passed | 0 | none |
| `translationIsLocal` | order | `noEndlet` | passed | 0 | none |
| `orderUnguarded` | order | `endlet` | unknown | - | `@contract.unproved` |
| `orderUnguarded` | order | `noEndlet` | unknown | 0 | `@contract.unproved` |
| `orderSwapped` | order | `endlet` | unknown | - | `@contract.unproved` |
| `orderSwapped` | order | `noEndlet` | unknown | 0 | `@contract.unproved` |
| `orderConstantFalse` | order | `endlet` | refuted | - | `@contract.refuted` |
| `orderConstantFalse` | order | `noEndlet` | refuted | 0 | `@contract.refuted` |
| `reference` | gate | `endlet` | passed | - | none |
| `reference` | gate | `noEndlet` | passed | 0 | none |
| `translationIsLocal` | gate | `noEndlet` | passed | 0 | none |
| `gateBareRequires` | gate | `endlet` | refuted | - | `@contract.obligationLost` |
| `gateBareRequires` | gate | `noEndlet` | refuted | 0 | `@contract.obligationLost` |
| `gateBareEnsures` | gate | `endlet` | unknown | - | `@contract.unproved` |
| `gateBareEnsures` | gate | `noEndlet` | unknown | 0 | `@contract.unproved` |
| `reference` | reader | `endlet` | passed | - | none |
| `reference` | reader | `noEndlet` | passed | 0 | none |
| `translationIsLocal` | reader | `noEndlet` | passed | 0 | none |
| `readerFallbackOnError` | reader | `endlet` | refuted | - | 2 tests fail, among them "a missing file is the error, not the fallback" |
| `readerFallbackOnError` | reader | `noEndlet` | refuted | 0 | 2 tests fail, among them "a missing file is the error, not the fallback" |
| `readerPure` | reader | `endlet` | refuted | - | `@effect.invalidAwait` |
| `readerPure` | reader | `noEndlet` | refuted | 0 | `@effect.invalidAwait` |
| `reference` | plan | `endlet` | passed | - | none |
| `reference` | plan | `noEndlet` | passed | 0 | none |
| `translationIsLocal` | plan | `noEndlet` | refuted | 1 | an ordinary let is closed |
| `planOnce` | plan | `endlet` | refuted | - | 2 tests fail, among them "two runs use exactly two invocations" |
| `planOnce` | plan | `noEndlet` | refuted | 0 | 2 tests fail, among them "two runs use exactly two invocations" |
| `planNoCeiling` | plan | `endlet` | refuted | - | `@effect.outsideCeiling`, `@contract.unusedEffect` |
| `planNoCeiling` | plan | `noEndlet` | refuted | 0 | `@effect.outsideCeiling`, `@contract.unusedEffect` |
| `reference` | optional | `endlet` | passed | - | none |
| `reference` | optional | `noEndlet` | passed | 0 | none |
| `translationIsLocal` | optional | `noEndlet` | passed | 0 | none |
| `optionalBareRequires` | optional | `endlet` | unknown | - | `@contract.unproved` |
| `optionalBareRequires` | optional | `noEndlet` | unknown | 0 | `@contract.unproved` |
| `optionalNoOffset` | optional | `endlet` | refuted | - | 3 tests fail, among them "a positive value is offset" |
| `optionalNoOffset` | optional | `noEndlet` | refuted | 0 | 3 tests fail, among them "a positive value is offset" |
| `reference` | rows | `endlet` | passed | - | none |
| `reference` | rows | `noEndlet` | passed | 0 | none |
| `translationIsLocal` | rows | `noEndlet` | refuted | 1 | an ordinary let is closed |
| `rowsAmountLost` | rows | `endlet` | refuted | - | 2 tests fail, among them "the latest amount of a key" |
| `rowsAmountLost` | rows | `noEndlet` | refuted | 0 | 2 tests fail, among them "the latest amount of a key" |
| `empty` | packet | `endlet` | refuted | - | `@name.unknownSymbol` |
| `empty` | packet | `noEndlet` | refuted | 0 | `@name.unknownSymbol` |
| `prose` | packet | `endlet` | refuted | - | `@syntax.invalidForm`, `@name.unknownSymbol` |
| `prose` | packet | `noEndlet` | refuted | 0 | `@syntax.invalidForm`, `@name.unknownSymbol` |
| `fence` | packet | `endlet` | refuted | - | `@syntax.invalidForm` |
| `fence` | packet | `noEndlet` | refuted | 0 | `@syntax.invalidForm`, `@syntax.unmatchedDelimiter`, `@name.unknownSymbol` |
| `authority` | reader | `endlet` | refuted | - | 2 tests fail, among them "a missing file is the error, not the fallback" |
| `endless` | packet | `endlet` | unknown | - | `@runtime.memoryExhausted` |
| `endless` | packet | `noEndlet` | unknown | 0 | `@runtime.memoryExhausted` |
| `notUtf8` | packet | `endlet` | refuted | - | `@syntax.invalidEncoding`, `@name.unknownSymbol` |
| `notUtf8` | packet | `noEndlet` | refuted | 0 | `@syntax.invalidEncoding`, `@name.unknownSymbol` |
| `endlessTail` | packet | `endlet` | unknown | - | `test` stopped: the grader's time limit |
| `endlessTail` | packet | `noEndlet` | unknown | 0 | `test` stopped: the grader's time limit |
| `deep` | packet | `endlet` | unknown | - | `check` ends without a document, exit `0xC00000FD`: the toolchain gave no document |
| `deep` | packet | `noEndlet` | unknown | 0 | the reading fails at the reader's stack; `check` ends without a document, exit `0xC00000FD`: the toolchain gave no document |

Every row above is as registered.

`translationIsLocal` grades the `endlet` reference as a `noEndlet`
solution: the translation leaves the file unchanged and reads every
ordinary `let` as closed, so a reference with an ordinary `let` is
`refuted` for its own surface. The references of
`positive`, `order`, `gate`, `reader`, and `optional` have no ordinary
`let`, so they are the same bytes in both arms and pass as either.

The robustness subjects:

- `empty`, `prose`, and `fence` are an empty file, a paragraph, and the
  packet reference inside a Markdown fence; each is `refuted` by `check`,
  and the grader prints its line.
- `notUtf8` is seven bytes that are not UTF-8; the reader refuses it, the
  file is placed unchanged, and `check` refutes it.
- `authority` is a reader solution that falls back on every error, shipped
  with a project file that adds a binary target with grants and with a test
  module that passes it. The grader places the task's own project and
  tests, so it is `refuted`. `authorityUnrestored` runs the solution's own
  workspace, copied to the temporary directory, and both `check` and `test`
  pass there, which shows that the refusal comes from the restoration.
- `endless` is a packet solution whose helper calls itself before it
  returns. Every test that reaches it ends `@test.exhausted` with
  `@runtime.memoryExhausted`, so the grade is `unknown` by the memory
  limit.
- `endlessTail` is a packet solution whose helper first returns a call of
  itself under `Equatable.equal(code code)`, which stays true. It allocates
  nothing, so no limit of the toolchain stops it; `test` is stopped at the
  grader's time limit and the grade is `unknown`. The registered limit is
  120 seconds; the self-check runs this subject with 10.
- `deep` is the packet reference with its stop code nested in 1,000,000
  list brackets, deeper than the reader's stack. In `noEndlet` the reading
  fails there, and the file is placed unchanged with a closed count of
  zero. In both arms `check` and `test` then overflow the toolchain's own
  stack and end with exit `0xC00000FD` and no document, so the grade is
  `unknown` with the reason "the toolchain gave no document". That is the
  toolchain at the freeze commit, which graded the pilot. A toolchain with
  the source nesting limit of the
  [tooling chapter](../../../../spec/05-tooling.md) answers the same file
  with `@tool.limitExceeded`, which is also `unknown`, so the self-check
  asks this subject for its grade alone.

## The reader

`grade.ps1 -CheckReader` reads every `.vib` file of `conformance/cases`,
`stdlib`, and the other examples. A file a case expects no syntax
diagnostic for must read; its closers are removed, the result is read and
translated back, and the token sequence must be the original. A file with
an expected syntax diagnostic must be refused. The run at the freeze commit
reads 1,533 files: 1,440 read and round-trip, 69 are refused as expected, 22 are
skipped because they are not source of the case's operation or carry a
syntax diagnostic with no source, and none mismatch. Two files are listed
with their reason:

- the test module of the case for a test parameter of another type reads,
  because the checker, not the grammar, reports that parameter;
- the case for a `let`-`else` fallback that is not `Never` ends its
  fallback with an ordinary `let` followed by two closers, so the file
  without its closers reads the first `endlet` as the inner one, as the
  registration's rule gives it.

The reader has a stack of 256 MB. It checks the stack at each construct it
enters, so a file deep enough to exhaust it is a reading that failed, as
the `deep` subject shows.
