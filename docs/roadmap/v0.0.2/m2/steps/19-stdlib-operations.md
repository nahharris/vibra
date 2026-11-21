# Step 19: stdlib-operations

Kind: vertical. Slice 2 completed: the five operation modules with the 17
standard operation interfaces, their error types, the filesystem policy
types, `RelativePath`, and the narrowing members, each `external @host`
member checked against its registry row.
Depends on: 02, 12, 18.
Crates: `stdlib`, `vibra-types`, `conformance/cases`.

## Prerequisites

- Read: [effects
  chapter](../../../../spec/03-effects.md#standard-operation-inventory)
  "Standard operation inventory", "Operation interfaces" (the two
  paragraphs on `external @host`), "Narrowing", "Scope policies" (the
  source types and "Paths", "Reserved device names"); [runtime
  chapter](../../../../spec/06-runtime.md#host-registry) "Host registry",
  "Error types", "External providers"; [questions](../spec-questions.md)
  Q1, Q2, Q10, Q16, Q26.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-types/src/stdlib/external.rs:57` | `pub fn check_externals` | checks a `@host` member against its row: owner, signature |
| `crates/vibra-types/src/check/effects/members.rs:48` | `CallableKind::Proc if member.external().is_some()` | `effects::members::contract_member` reports a host-bound member as unsupported; it gains the host-member rule |
| `crates/vibra-ir/src/external/host.rs:842` | `pub fn host_row` | the rows |
| `crates/vibra-ir/src/module_set.rs:46` | `pub enum FunctionBody` | `Host`, the body a bound member lowers to |

Proposed: `stdlib/src/std/{io,fs,env,time,random}.vib`.

## Files owned

- `stdlib/manifest.vibon`
- `stdlib/src/std/io.vib`
- `stdlib/src/std/fs.vib`
- `stdlib/src/std/env.vib`
- `stdlib/src/std/time.vib`
- `stdlib/src/std/random.vib`
- `crates/vibra-types/src/stdlib/external.rs`
- `crates/vibra-types/src/check/effects/members.rs`
- `crates/vibra-types/src/check/tests/rows.rs`
- `conformance/rules/m2-19.tsv`
- the case directories of the matrix below

## Ordered tasks

1. `members::contract_member`: a `proc` member with `external @host` has
   no body and no `performs`; its complete row is `{owner}`; it lowers to
   `FunctionBody::Host`. For an `impl` it is an abstract member (Q2).
   Invariant: outside the embedded library the section is
   `@syntax.invalidSection`, as M1 reports.
2. `check_externals`: for a `@host` symbol, the enclosing interface's
   module and name equal the row's owner (`@external.ownerMismatch`); the
   member's type equals the row's signature with `Self` first
   (`@external.signatureMismatch`); an unknown symbol is
   `@external.unknownSymbol`.
3. The five error types with their own cases followed by `denied`,
   `limitExceeded`, `unsupported`, `outcomeUnknown`; `FsKind`,
   `FsMetadata`.
4. `RelativePath` in `@std.fs`: a declared type over `List(Str)` whose
   fallible constructor accepts exactly the names of the chapter's
   profile, written as a pure Vibra predicate over the scalars of each
   name; `Equatable` and `Ordered` by names (Q16).
5. `FsPolicy`, `NameTest`, `FileDepth`, `DirDepth`, `FileAction`,
   `DirAction` in `@std.fs`, exactly as the chapter writes them (Q26).
6. The 17 interfaces, each with its one `external @host` member; the
   narrowing members as `external @compiler` members bound to
   `grant.limit` on all 17, and to `grant.restrict` and `grant.sub` on
   the six `Dir…` interfaces. For an `impl` each narrowing member is
   abstract, as the host-bound member is (Q1, Q2): a missing one is
   `@type.missingAbstractMember`. `StdinText.read` and `StdinBytes.read`
   take `max: U64` (Q10).
7. `@std.io.readAllText` and `@std.io.readAllBytes`: procedures in
   source, with the rows `{StdinText}` and `{StdinBytes}`, that await the
   member with a count of 65,536 until it returns the empty value, and
   return the concatenation or the first error (Q10). Written as a tail
   recursion over an accumulator.
8. The manifest: the five modules, and the `host` symbol list with the 17
   symbols, sorted.

## Positions

| Rule | Positions |
| --- | --- |
| a bound member matches its row | owner; each parameter; the result; the error type |
| an operation interface is used | parameter type; `performs` row; `impl` target; interface value |
| `RelativePath` names | each rejecting rule of the profile, and its nearest accepted name |

## Test matrix

`workspaceCheck`, profile `staticV2`, unless stated; a row that names another
operation has that operation's profile (Q34). Loader rows are tests
of `vibra-types` over an altered copy of a library module. `Memory` is a
type with `impl DirReadText` that supplies `read` (returning
`Result.err(error: FsError.notFound())`), `limit` (returning `self`), and
`restrict` and `sub` (returning `Result.ok(value: self)`). Every `impl`
of a standard operation interface in a later guide supplies its
narrowing members the same way unless its row says otherwise.

| Case id | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| `V2-EFFECT-std-copy-note` | positive | the chapter's `copyNote`, with its imports | accepted | 03 Effect ceilings, the example |
| `V2-EFFECT-std-prepare-write` | positive | the chapter's `prepareWrite` and `writeTwice` | accepted | 03 Cold plans, the example |
| `V2-EFFECT-std-each-interface` | positive | one `proc` per interface awaiting its member through a parameter of that type, all 17; the two standard-input members with `4096u64` | accepted; `@types.v2` gives each member the registry's result type | 03 Standard operation inventory, the table |
| `V2-EFFECT-std-read-all` | positive | `proc slurp takes {input: StdinText} returns r: Result(Str IoError) performs {StdinText} do return await io.readAllText(input) endproc` | accepted (Q10) | 03 Standard operation inventory, the table |
| `V2-EFFECT-std-read-all-row` | negative | the same with no `performs` | `@effect.outsideCeiling` at the `await` (Q10) | 03 Effect ceilings |
| `V2-EFFECT-std-stdin-count-missing` | negative | `await StdinText.read(input)` | `@type.argumentMismatch` at the call (Q10) | 03 Standard operation inventory, the table |
| `V2-EFFECT-std-row-missing` | negative | `copyNote` with `performs {DirReadText}` | `@effect.outsideCeiling` at the `await` of `DirWriteText.write` | 03 Effect ceilings |
| `V2-EFFECT-std-error-not-converted` | negative | `try await StdoutText.write(out "x")` in a `proc` returning `Result(Void FsError)` | `@type.invalidTry` at the `try` | 03 Delivery: "There is no implicit conversion between error types" |
| `V2-EFFECT-std-unhandled-result` | negative | the statement `await StdoutText.write(out "x")` | `@type.unhandledFallible` at the statement | 01 `let`: a fallible value left unused |
| `V2-TYPE-INTERFACE-std-error-cases` | positive | an exhaustive `match` over the eleven cases of `FsError` | accepted | 06 Error types, both tables |
| `V2-TYPE-INTERFACE-std-error-missing-case` | negative | the same without `outcomeUnknown` | `@pattern.nonExhaustive` | 06: "Each of the five also has these four cases" |
| `V2-TYPE-INTERFACE-std-fs-policy-types` | positive | the chapter's `textOnly` value | accepted | 03 Scope policies, the example |
| `V2-TYPE-INTERFACE-fs-policy-missing-field` | negative | `FsPolicy.files(prefix: p depth: FileDepth.tree() actions: [FileAction.read()])` | `@type.argumentMismatch` | 03 Meaning: "A missing field is an error, never a wildcard" |
| `V2-TYPE-INTERFACE-relative-path-constant` | positive | `let Result.ok(value: p) be RelativePath(["a.txt"]) endlet` with no `else` | accepted | 04 Tests: "the checker evaluates it and knows it succeeds" |
| `V2-TYPE-INTERFACE-relative-path-dotdot` | negative | the same with `[".."]` | `@pattern.refutableBinding` at the pattern | 03 Paths: "rejected if it is `.` or `..`" |
| `V2-RUNTIME-relative-path-names` | positive, `workspaceTest` | `assert.true(Result.isErr(RelativePath([n])))` for `""`, `.`, `a/b`, `a\\b`, `a:b`, `a.`, `a `, `NUL.txt`, `con .log`, `COM¹`; `assert.true(Result.isOk(…))` for `nul2`, `a.txt`, `.txt`, `com10` | `@test.passed` | 03 Paths; Reserved device names |
| `V2-TYPE-INTERFACE-relative-path-ordered` | positive | `Dict(RelativePath Str)` as a parameter type | accepted (Q16) | 04 Test worlds: "`files: Dict(RelativePath Str) be [:]`" |
| `V2-EFFECT-impl-host-member-supplied` | positive | `Memory` | accepted (Q1, Q2) | 03: "such as a package's in-memory model of the interface" |
| `V2-EFFECT-impl-host-member-missing` | negative | `Memory` without its `read` member | `@type.missingAbstractMember` at the `impl` (Q2) | 02 Implementations |
| `V2-EFFECT-impl-host-member-row` | negative | `Memory` whose `read` has `performs {StdoutText}` | `@type.mismatch` at the `performs` section | 03: "It MUST keep each member's row" |
| `V2-GRANT-impl-supplies-limit` | positive | `Memory`, and a `fn` that calls `DirReadText.limit` on a `Memory` viewed as a `DirReadText` | accepted (Q1) | 03 Narrowing |
| `V2-GRANT-impl-missing-limit` | negative | `Memory` without its `limit` member | `@type.missingAbstractMember` at the `impl` (Q1) | 02 Implementations |
| `V2-GRANT-impl-missing-restrict` | negative | `Memory` without its `restrict` member | `@type.missingAbstractMember` at the `impl` (Q1) | 02 Implementations |
| `V2-GRANT-narrowing-types` | positive | `fn f takes {d: DirReadText p: FsPolicy l: GrantLimits} returns r: Result(DirReadText ScopeError) do let n be DirReadText.limit(d l) endlet return DirReadText.restrict(n p) endfn` | accepted: narrowing is pure | 03 Narrowing: "They perform nothing and open nothing" |
| `V2-GRANT-restrict-on-stream` | negative | `StdoutText.restrict(out p)` | `@name.unknownSymbol` at `restrict` | 03 Narrowing, the table: "the six `Dir…` interfaces" |
| `V2-GRANT-sub-result-is-fallible` | negative | `let d2: DirReadText be DirReadText.sub(d p) endlet` | `@type.mismatch` at the initializer | 03: `sub` returns `Result(Self ScopeError)` |
| `V2-SRC-DECL-host-outside-std` | negative | a project interface with `proc p takes {s: Self} returns r: Void external @host "io.stdoutText" endproc` | `@syntax.invalidSection` at the section | 03: "Only the embedded standard library declares members bound to the host" |
| `ATK-stdlib-host-signature` | negative, loader test | `fs.vib` with `DirReadText.read` declared `at: Str` | `@external.signatureMismatch` | 03: "a wrong signature `@external.signatureMismatch`" |
| loader: wrong owner | negative, loader test | `fs.readText` bound on `DirList` | `@external.ownerMismatch` | 03: "A wrong owner is `@external.ownerMismatch`" |
| loader: unknown symbol | negative, loader test | `external @host "fs.remove"` | `@external.unknownSymbol` | 03: "an unknown symbol `@external.unknownSymbol`" |
| loader: `performs` on a bound member | negative, loader test | a bound member with `performs {StdoutText}` | rejected at load | 03: "Such a member has no body and no `performs`" |
| loader: the shipped library | positive, loader test | the embedded modules | every `@host` member matches; 17 symbols bound once each | 04: "every `external` declaration against the registry's signature and owner" |

## Diagnostics and schema changes

Codes: `@effect.outsideCeiling`, `@type.invalidTry`,
`@type.unhandledFallible`, `@pattern.nonExhaustive`,
`@pattern.refutableBinding`, `@type.argumentMismatch`, `@type.mismatch`,
`@type.missingAbstractMember`,
`@name.unknownSymbol`, `@syntax.invalidSection`,
`@external.signatureMismatch`, `@external.ownerMismatch`,
`@external.unknownSymbol`. The manifest gains five modules and the `host`
list.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-types -p vibra-workspace -p vibra-cli -p vibra-conformance --all-features
cargo run --locked --offline -p vibra-conformance --bin vibra-conformance -- --root conformance/cases
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0. Every lowered library module passes
`vibra-ir::effects`; a host body on a wrong member appears as
`@runtime.invalidCheckedProgram`.

## Stays unavailable

No value of an operation interface type exists outside a model: a test
has no `TestWorld` (21) and a target no entry (25). Awaiting a member on a
model runs; a host body is never reached.

## Excluded

`@std.test` (21); grant typing rules (22).

## Completion evidence

- [ ] `conformance/rules/m2-19.tsv` has one `active` row per `V2-` id.
- [ ] A script in the report lists the 17 `external @host` symbols of the
      library and compares them with `host-2.0.txt`.
- [ ] Every position of the table has a positive and a negative row.
- [ ] The validation commands exit 0.
