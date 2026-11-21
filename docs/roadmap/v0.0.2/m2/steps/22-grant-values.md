# Step 22: grant-values

Kind: vertical. Slice 3 completed: grants as values in source. The typing
rules that keep a grant sealed, the three grant shapes, brands, and
narrowing, policies, and limits observed from tests in a fake world.
Depends on: 21.
Crates: `vibra-types`, `conformance/cases`.

## Prerequisites

- Read: [effects chapter](../../../../spec/03-effects.md#capability-values)
  "Capability values", "Narrowing", "Brands", "Scope policies" (Meaning,
  Filesystem policies), "Limits", "Delivery and typed outcomes"; [type
  chapter](../../../../spec/02-type-system.md#interface-values) "Interface
  values"; [diagnostics
  chapter](../../../../spec/07-diagnostics-and-conformance.md#required-coverage)
  required coverage 10 and 11; [questions](../spec-questions.md) Q1, Q7,
  Q8, Q9, Q10, Q11, Q29.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/check/lower.rs:873` | `pub(crate) fn check_program` | calls into `effects::grants` through the hooks of step 04 |
| `crates/vibra-types/src/check/expr.rs:2764` | `fn unequatable_operands` | calls `effects::grants::check_equatable` (step 04) |
| `crates/vibra-types/src/check/declare.rs:965` | `pub(crate) fn key_fault` | calls `effects::grants::check_dict_key` (step 04) |
| `crates/vibra-ir/src/constant.rs:28` | `Type::Fn(_) \| Type::Proc(_)` | the IR-side rule of step 03 that this step mirrors |

Proposed: the body of `effects::grants::{contains_grant,
check_equatable, check_dict_key}`.

## Files owned

- `crates/vibra-types/src/check/effects/grants.rs`
- `crates/vibra-types/src/check/tests/grants.rs`
- `conformance/rules/m2-22.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `contains_grant(type)`: an operation interface value type, or a type
   that contains one through records, tuples, `Option`, `List`, wrappers,
   and declared types. Explicit work list.
2. An `assert.equal` operand whose type contains a grant is
   `@type.functionNotEquatable`, once per such operand, as for a callable.
3. A dict key type that contains a grant is `@type.invalidDictKey` at the
   key type.
4. A module value whose type contains a grant cannot be initialized: the
   rule is the existing constant rule; this step adds its case.
5. No new rule for "no constructor", brands, or shapes: they follow from
   interface value types and private representations. This step adds
   their cases.

## Positions

| Rule | Positions |
| --- | --- |
| a grant is not equatable | bare grant; `Option` of one; a record with one; a list of them |
| a grant is not a dict key | key type bare; key type a record with a grant |
| narrowing never adds | `restrict`, `sub`, `limit`; then each again on the result; through a copy; through a bundle field |
| a refusal is a value | `denied` before the adapter; `limitExceeded`; a `ScopeError` |

## Test matrix

`workspaceTest`, profile `interpreterV2`, unless stated; a row that names
another operation has that operation's profile (Q34). `dir` is the record
`TestWorld.directory(w files: [aTxt: "A" bBin: "B" subTxt: "S"])` with
paths `["a.txt"]`, `["b.bin"]`, `["sub" "s.txt"]`; `textOnly` is the
chapter's policy value. `Memory` is the memory-only model of step 19,
holding the text `"M"`, whose narrowing members return it. `Forward` is
the grant-backed model of Q1: a type over `{inner: DirReadText}` whose
`read` awaits `inner`, whose `limit` returns a `Forward` over
`DirReadText.limit(self.inner limits)`, and whose `restrict` and `sub`
do the same under `try`. `in` is the `stdinBytes` grant of
`TestWorld.streams(w stdin: "abc")`.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-GRANT-no-constructor` | negative, `workspaceCheck` | `await notes.countLines({root: "/"} path)` where a `DirReadText` is expected | `@type.argumentMismatch` at the argument | 03: "A path, a policy value, a wrapper type, an `impl`, and decoded data are not grants" |
| `V2-GRANT-model-is-not-grant` | positive | a `Memory` passed to `notes.countLines` | `@test.passed`; the trace has no event | 03 Operation interfaces: "Implementing an interface makes no grant" |
| `V2-GRANT-forward-model-reads` | positive | `Forward(inner: dir.readText)` passed where a `DirReadText` is expected; read `a.txt` | `Ok("A")`; one event, whose grant projection is that of `dir.readText` | 06: "A source implementation may model an interface using grants it holds" |
| `V2-GRANT-assert-equal-grant` | negative, `workspaceCheck` | `assert.equal(dir.readText dir.readText)` | `@type.functionNotEquatable` at each operand | 07: "an `assert.equal` operand whose type contains a callable, plan, or grant" |
| `V2-GRANT-assert-equal-option-grant` | negative, `workspaceCheck` | `assert.equal(Option.some(value: dir.readText) Option.none())` | `@type.functionNotEquatable` at the first operand | the same |
| `V2-GRANT-assert-equal-result-ok` | positive | `assert.equal(Result.ok(value: "A") await DirReadText.read(dir.readText aTxt))` | `@test.passed` | 03: "ordinary typed results" |
| `V2-GRANT-dict-key-grant` | negative, `workspaceCheck` | a parameter of type `Dict(DirReadText Str)` | `@type.invalidDictKey` at the key type | 03: "cannot be … used as a dict key" |
| `V2-GRANT-required-optional-bundle` | positive | the chapter's `report` shape with `DirReadText`, `Option(StdoutText)`, and `{reader writer}`, called with `Option.none()` | `@test.passed` | 03 Capability values, the three shapes |
| `V2-GRANT-bundle-field-only` | positive | `let {readText: r} be dir endlet`; a `proc` taking only `r` | `@test.passed`; that `proc` has no `DirWriteText` in scope | 03: "Passing on one field passes on only that grant" |
| `V2-GRANT-view-does-not-narrow` | negative, `workspaceCheck` | `DirWriteText.write(dir.readText aTxt "x")` | `@type.argumentMismatch` at the receiver | 03: "Viewing a value as an interface never reduces what the value can do" |
| `V2-GRANT-brand` | positive | a module declaring `type LogDir of DirWriteText` with a private representation and a `wrap` function; another module calls a `proc` taking `LogDir` | `@test.passed` | 03 Brands |
| `V2-GRANT-brand-forged` | negative, `workspaceCheck` | the other module writes `LogDir(dir.writeText)` | `@name.inaccessibleRepresentation` at the construction | 03: "Only the module that declares `LogDir` can construct one" |
| `V2-GRANT-denied-outside-policy` | positive | `restrict(dir.readText textOnly)`, read `b.bin` | `Err(denied)` | 03 `denied`: "The request was refused" |
| `V2-GRANT-denied-is-value` | positive | the same, matched in a `match` and the test continues | `@test.passed`, no trap | 07: "That is never a diagnostic and never a trap" |
| `V2-GRANT-denied-no-event` | positive | the same | `events` is empty | 06: "A refusal before the adapter is called … appends nothing" |
| `V2-GRANT-restrict-permits` | positive | the same grant, read `a.txt` | `Ok("A")`; the event's `policy` is a key of `policies` | 03 Scope policies, the example |
| `V2-GRANT-restrict-keeps-parent` | positive | `restrict(restrict(r textOnly) anyNamePolicy)`, read `b.bin` | `Err(denied)` | 03: "The result's policy is the parent's policy **and** the new one" |
| `V2-GRANT-parent-stays-valid` | positive | after a `restrict`, read `b.bin` through the original | `Ok("B")` | 03: "The parent stays valid" |
| `V2-GRANT-rule-correlation` | positive | a write grant restricted to `anyOf[files(prefix ["in"], create) files(prefix ["out"], replace)]`; create under `out` | `Err(denied)` | 03 Meaning: "Its fields are never separated" |
| `V2-GRANT-prefix-by-names` | positive | policy prefix `["sub"]`; a file at `["subOld" "s.txt"]` | `Err(denied)` | 03 Paths: "so `reports` does not cover `reportsOld`" |
| `V2-GRANT-file-rule-no-listing` | positive | `dir.list` restricted to a file rule with `read` | `entries` is `Err(denied)` | 03: "A file rule grants no listing" |
| `V2-GRANT-suffix-exact-scalars` | positive | `suffix(".txt")` against `a.txt`, `.txt`, `a.TXT`, `a.txt.exe` | `Ok`, `Ok`, `denied`, `denied` | 03 Names |
| `V2-GRANT-sub-rebases` | positive | `sub(dir.readText ["sub"])`, read `["s.txt"]` | `Ok("S")`; the event's `args` hold `["s.txt"]` (Q7) | 03 Narrowing: "`sub` adds a path prefix below the same held directory" |
| `V2-GRANT-sub-then-restrict` | positive | `restrict(sub(r ["sub"]) textOnly)`, read `["s.txt"]` | `Ok("S")` (Q7) | the same |
| `V2-GRANT-sub-escape-denied` | positive | `sub(r ["sub"])`, read `["a.txt"]` | `Err(notFound)`: the request is `sub/a.txt` (Q7) | 06: "A grant narrowed by `sub` keeps the same acquired anchor and adds a path restriction" |
| `V2-GRANT-sub-projection` | positive | one read through a `sub` grant | the projection's `policy` hash is that of the effective policy in root coordinates (Q7) | 06 Audit trace, grant projection |
| `V2-GRANT-sub-missing-directory` | positive | `sub(r ["nope"])` | `Result.ok`; a read is `Err(notFound)` | 03: "It does not check that the directory exists" |
| `V2-GRANT-restrict-empty-suffix` | positive | `restrict` with `NameTest.suffix(value: "")` | `Err(ScopeError.invalidPolicy)` (Q8) | 03 Names: "An empty suffix is invalid" |
| `V2-GRANT-restrict-empty-actions` | positive | `restrict` with `actions: []`, then a read | `Result.ok`; the read is `Err(denied)` (Q8) | 03 Meaning |
| `V2-GRANT-restrict-too-complex` | positive | 65 chained `restrict` calls, built by a recursive `fn` | the 65th is `Err(ScopeError.policyTooComplex)` (Q9) | 03: "counting ancestors acquired by restriction" |
| `V2-GRANT-policy-4096-nodes` | boundary | a source policy counted at 4,096 | `Result.ok` (Q9) | 03: "at most 4,096 nodes and entries in total" |
| `V2-GRANT-limit-exceeded-is-value` | positive | `limit(r invocations: some(1))`, two reads | `Ok`, then `Err(limitExceeded)`; one event | 03: "Exhaustion is the typed error `limitExceeded`" |
| `V2-GRANT-limit-keeps-smaller` | positive | `limit(limit(r 1) 5)`, two reads | the second is `limitExceeded` | 03: "`limit` keeps the smaller of each bound" |
| `V2-GRANT-limit-siblings-share` | positive | `p = limit(r 2)`; `a = limit(p 5)`, `b = limit(p 5)`; read through `a`, `b`, `a` | the third is `limitExceeded` (Q11) | 07 Required coverage 11: "shared counters under siblings" |
| `V2-GRANT-limit-child-after-use` | positive | one read through `r`, then `c = limit(r 1)`, one read through `c`, one more | `Ok`, `Ok`, `limitExceeded` (Q11) | 07 Required coverage 10: "limits never reset" |
| `V2-GRANT-limit-projection` | positive | a read through `limit(limit(r inv 5) inputBytes 9)` | `limits: {invocations: 5u64 inputBytes: 9u64}` (Q11) | 06: "a record of the bounds that are set" |
| `V2-GRANT-limit-input-bytes` | positive | `limit(r inputBytes: some(1))`; read `a.txt` (1 byte), then again | `Ok("A")`, then `limitExceeded`; one event (Q29) | 03 Limits: "Input bytes are the bytes read for the program" |
| `V2-GRANT-limit-output-bytes` | positive | `limit(dir.writeText outputBytes: some(2))`; write `"abc"` | `Err(limitExceeded)`; the file is absent | 06: "A write reserves the encoded size of its argument before it sends" |
| `V2-GRANT-limit-stdin-exact` | boundary | `limit(in inputBytes: some(3))`; `StdinBytes.read(… 3u64)` | `Ok` of the bytes `abc` (Q10) | 06 Closed operation semantics |
| `V2-GRANT-limit-stdin-over` | positive | `limit(in inputBytes: some(2))`; `StdinBytes.read(… 3u64)`; then `in` reads with `3u64` | `limitExceeded`, then `Ok` of `abc`; one event (Q10) | 06 Standard input: "consumes nothing" |
| `V2-GRANT-limit-stdin-zero` | boundary | empty stdin; `limit(in inputBytes: some(0))`; `StdinBytes.read(… 1u64)` | `Ok` of no bytes (Q10) | the same |
| `V2-GRANT-limit-stdin-count` | positive | `limit(in inputBytes: some(2))`; `StdinBytes.read(… 2u64)`; then `in` reads with `3u64` | `Ok` of `ab`, then `Ok` of `c` (Q10) | 06 Closed operation semantics |
| `V2-GRANT-narrow-model-limit` | positive | `DirReadText.limit(memory limits)` with `invocations: some(1)`, on a `Memory` viewed as a `DirReadText`; two reads | `Ok("M")` twice: the model's own `limit` ran; no event (Q1) | 03 Narrowing |
| `V2-GRANT-narrow-model-restrict` | positive | `DirReadText.restrict(memory textOnly)`; read `b.bin` | `Result.ok`; the read is `Ok("M")`; no event (Q1) | the same |
| `V2-GRANT-narrow-model-sub` | positive | `DirReadText.sub(memory p)` | `Result.ok`; no event (Q1) | the same |
| `V2-GRANT-narrow-forward-limit` | positive | `DirReadText.limit(forward limits)` with `invocations: some(1)`, on a `Forward` over `dir.readText` viewed as a `DirReadText`; two reads of `a.txt` | `Ok("A")`, then `Err(limitExceeded)`; one event, whose `limits` are `{invocations: 1u64}` (Q1) | 03: "never adds a right, changes the resource, or resets a limit" |
| `V2-GRANT-narrow-forward-restrict` | positive | `DirReadText.restrict(forward textOnly)`; read `b.bin`, then `a.txt` | `Err(denied)`, then `Ok("A")` (Q1) | the same |
| `V2-GRANT-narrow-forward-parent-stays` | positive | after the `limit` of `V2-GRANT-narrow-forward-limit`, two reads through the original `Forward` | `Ok("A")` twice | 03: "The parent stays valid" |
| `V2-GRANT-copy-shares` | positive | `let c be limit(r 1) endlet let d be c endlet`; read through `c`, then `d` | the second is `limitExceeded` | 06 Accounting: "A copy of a sealed authority shares its … accounting identity" |
| `V2-GRANT-closure-keeps-grant` | positive | a `proc` value capturing a grant, returned from a `fn`, awaited | `Ok` | 06: "A grant outlives the variable that introduced it" |

## Diagnostics and schema changes

Codes: `@type.functionNotEquatable`, `@type.invalidDictKey`,
`@type.argumentMismatch`, `@name.inaccessibleRepresentation`. No schema
change.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-host -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. A case that fails only in its trace is a broker or trace
defect, returned to the owner of that file with the two documents.

## Stays unavailable

`@tool.unavailable`: a binary target's entry.

## Excluded

Recipe policies (26); real directories (31).

## Completion evidence

- [ ] `conformance/rules/m2-22.tsv` has one `active` row per case id.
- [ ] Every row that names a question states the alternative result in
      its case's comment, so a ruling changes one line.
- [ ] Required coverage 10 and 11 are mapped item by item to case ids in
      the report; the reserved HTTP items are listed as not applicable.
- [ ] The validation commands exit 0.
