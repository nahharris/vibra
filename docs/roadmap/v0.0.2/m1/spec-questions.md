# v0.0.2 M1: specification questions

Questions the M1 plan could not answer from the specification. Each names
the place, the question, the answer the plan assumes, and the first step
that needs it. An answer changes the specification, not a step guide; the
guide then follows the chapter. No step invents a diagnostic code: where a
code seems missing, the question says so.

| Q | Blocks step | Area |
| --- | --- | --- |
| Q1 | 27 | Wasm backend and the v0.0.1 corpus |
| Q2, Q3, Q7, Q8 | 03, 04 | lexer and recovery |
| Q4, Q5, Q6 | 03, 25 | VIBON and reader observations |
| Q9, Q10 | 05, 07 | declaration sections |
| Q11 | 25 | `check` and the test unit |
| Q12, Q27, Q28 | 04, 10 | `use … enduse` |
| Q13, Q16 | 10, 15 | identities and snapshot shapes |
| Q14, Q25, Q26 | 19 | associated types and implementations |
| Q15, Q17 | 14, 20 | standard library |
| Q18, Q19, Q20, Q21 | 21, 22 | guards |
| Q22 | 18 | reserved labels |
| Q23, Q24, Q29, Q30 | 02, 09, 27 | transition and roadmap |

## Q1. The Wasm backend and the v0.0.1 corpus

Where: [release README](../README.md#scope-and-priorities); [runtime
chapter](../../../spec/06-runtime.md#wasm-transition).

Question: the release README says the WebAssembly backend keeps running the
v0.0.1 corpus. The backend consumes checked IR, and M1 replaces the IR and
deletes the old reader. Does the backend keep a second IR to run old-surface
programs, or does it run the new corpus where it can?

Recommended: it runs the pure executable cases of the new corpus that lower
one to one, under `wasmTransitionV2`, and reports the rest unavailable. The
sentence becomes "keeps a differential run of the cases it supports". The
plan assumes this from step 27.

## Q2. Hyphen in `atom-part`

Where: [source chapter, lexical
structure](../../../spec/01-source-language.md#lexical-structure).

Question: `atom-part = letter, { letter | digit | "-" }` admits `@a-b`. The
prose and every atom in the specification are camelCase. Is the hyphen
part of the language?

Recommended: remove `"-"` from the production. `@a-b` is then
`@syntax.invalidName`.

## Q3. What `@syntax.retiredForm` reports

Where: [diagnostics chapter,
registry](../../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface);
[source chapter, exclusions](../../../spec/01-source-language.md#exclusions).

Question: the registry has the code and no chapter lists its triggers.

Recommended: exactly two: a `(` where a top-level form or a statement is
expected (the parenthesized surface of v0.0.1), reported once over the
balanced form; and `as` outside `use`. Every other excluded spelling is
`@syntax.invalidForm`.

## Q4. The document of the `reader` operation

Where: [diagnostics chapter, persistent
observations](../../../spec/07-diagnostics-and-conformance.md#persistent-observations).

Question: the table lists `@sourceGraph.v1`, `@resolved.v2`, `@types.v2`,
and others, and no format for the lossless tree a `reader` case compares.

Recommended: add `@tree.v2`: `format`, `kind` (`source` or `data`), and
`root`, a node with `kind`, and either `children` or a leaf's `text`, where
the concatenated leaf texts equal the input. Error and missing nodes are
kinds, not flags.

## Q5. What `fmt` changes in a `.vibon` file

Where: [programs chapter, VIBON data
documents](../../../spec/04-programs-and-packages.md#vibon-data-documents);
[tooling chapter, formatting](../../../spec/05-tooling.md#formatting).

Question: the programs chapter describes a canonical encoding with sorted
entries; the tooling chapter says the formatter never reorders field
initializers or dict pairs. Which applies to `vibra fmt project.vibon`?

Recommended: `fmt` changes whitespace only. Sorted order belongs to the
canonical encoder that tools use when they write a document.

## Q6. Terminals of the data grammar

Where: [programs chapter, VIBON data
documents](../../../spec/04-programs-and-packages.md#vibon-data-documents).

Question: is `true` a `boolean` production of the data grammar or an
identifier; what does any other identifier, a doc string, or an application
such as `Option.some(value: 1i32)` produce in a data document?

Recommended: `true` and `false` are the only identifiers, as a `boolean`
production; any other identifier, a doc string, and an application are
`@syntax.invalidForm` at the token. Typed decoders reject a wrong shape
with `@data.invalidShape` afterwards.

## Q7. Triggers of `@syntax.missingSeparator`

Where: [source chapter, lexical
structure](../../../spec/01-source-language.md#lexical-structure).

Question: which adjacent token pairs need whitespace?

Recommended: two tokens that would lex differently when joined: a literal
followed directly by a letter or digit (`1i32x`, `"a"b`), an atom followed
by `@`, and a closing quote followed by an opening quote. Brackets and `:`
never need a separator.

## Q8. Recovery at a closer that does not match

Where: [diagnostics chapter,
recovery](../../../spec/07-diagnostics-and-conformance.md#recovery).

Question: when `endfn` appears while a `match` is open inside a `fn`, which
constructs close?

Recommended: if a construct of the closer's kind is open, every construct
inside it closes as missing, with one `@syntax.unmatchedDelimiter` at the
closer related to the innermost opener; otherwise the closer is an error
leaf and nothing closes.

## Q9. A tag outside a closed set

Where: [source chapter,
declarations](../../../spec/01-source-language.md#declarations).

Question: `visibility @friend`, `representation @open`, `role @thing`, and
`external @os` are grammatical. Which code rejects them?

Recommended: `@syntax.invalidForm` at the atom, since the set is closed by
the grammar's prose and no name is being resolved.

## Q10. A top-level `fn` with no body and no `external`

Where: [source chapter, `fn` and
`proc`](../../../spec/01-source-language.md#fn-and-proc).

Question: the grammar makes `do` optional so that interface members and
external functions can omit it. Which code rejects a body-less ordinary
`fn`?

Recommended: `@syntax.invalidForm` at the `endfn`.

## Q11. Whether `check` covers `@tests`

Where: [tooling chapter, command
surface](../../../spec/05-tooling.md#command-surface).

Question: "without `TARGET`, `check` covers every local target". The test
unit is not a target.

Recommended: `check` with no target also checks `@tests`; `check @app`
does not. The payload's obligations include those of test modules.

## Q12. `From` and `TryFrom` in the prelude

Where: [type chapter, the prelude](../../../spec/02-type-system.md#the-prelude);
[type chapter, conversion](../../../spec/02-type-system.md#conversion).

Question: the conversion examples call `From.convert` with no `use`, and
the prelude list must say whether the two interfaces are in it.

Recommended: both are prelude names, with `Equatable`, `Ordered`, and
`Iter`.

## Q13. Encoding of a declaration identity

Where: [tooling chapter, snapshot, claim and witness
identity](../../../spec/05-tooling.md#snapshot-claim-and-witness-identity).

Question: `@resolved.v2` and `predicateIdentity` need a stable identity for
a declaration. Its bytes are not specified.

Recommended: `sha256` over a framed tuple of scheme version, unit, module
path, declaration kind, and name path, with the same framing as
`workspaceRevision`.

## Q14. An associated type of a type parameter bounded by `Any`

Where: [type chapter, associated
types](../../../spec/02-type-system.md#associated-types).

Question: in `fn f(I) takes {x: I.Item}` where `I` has no bound that
declares `Item`, which code is reported?

Recommended: `@name.unknownSymbol` at `Item`, related to the type
parameter.

## Q15. An empty `native` list

Where: [programs chapter, the standard
library](../../../spec/04-programs-and-packages.md#the-standard-library);
[runtime chapter, native
implementations](../../../spec/06-runtime.md#native-implementations).

Question: may the manifest's `native` list be empty, with the interpreter
running every Vibra body?

Recommended: yes. Acceleration is an optimization the manifest declares;
M1 declares none.

## Q16. Closed shapes of the snapshot documents

Where: [diagnostics chapter, persistent
observations](../../../spec/07-diagnostics-and-conformance.md#persistent-observations).

Question: `@resolved.v2`, `@types.v2`, and `@testRun.v2` are given as field
lists. A case written from the specification needs the closed record of
each, with field order and the encoding of types and spans.

Recommended: add one closed record per document in the data grammar, with
types written in `@type.v2` and spans as `SpanDocument`.

## Q17. Unlisted members of `Str` and `Char`

Where: [runtime chapter, character, sequence and dictionary
operations](../../../spec/06-runtime.md#character-sequence-and-dictionary-operations).

Question: the M1 scope names `Str.trim` and character classification; the
member tables list neither.

Recommended: list `Str.trim` (ASCII space, tab, LF, CR at both ends) and
`Char.isAsciiDigit`, `Char.isAsciiAlphabetic`, `Char.isAsciiWhitespace`,
each defined through `Ordered.inRange`, or remove them from the scope.

## Q18. Diagnostics when a limit stops an obligation

Where: [type chapter, bare guards](../../../spec/02-type-system.md#bare-guards);
[diagnostics chapter, runtime and tool
failures](../../../spec/07-diagnostics-and-conformance.md#runtime-and-tool-failures).

Question: compile-time evaluation of a predicate reaches a tool limit. Is
that `@contract.unproved`, `@tool.limitExceeded`, or both?

Recommended: `@tool.limitExceeded` alone at the predicate, obligation
status `unknown`, envelope status `unknown`. `@contract.unproved` is kept
for an obligation the rules examined and could not establish.

## Q19. Recursion in a certified function

Where: [type chapter, predicates](../../../spec/02-type-system.md#predicates).

Question: may a certified function call itself, directly or through
another certified function?

Recommended: no in M1. A recursive function is outside the fragment and
reported `@contract.unsupportedPredicate` where a predicate calls it.

## Q20. The operand of `where`

Where: [type chapter, type
invariants](../../../spec/02-type-system.md#type-invariants).

Question: the operand is a function over the representation. May it be the
name of a module `fn`, an anonymous `fn`, or both, and how does a named one
resolve when it is declared after the type?

Recommended: both; a name resolves in the flat module scope like any other.
The chapter gains one example of each.

## Q21. Integer facts and the `Ordered` defaults

Where: [diagnostics chapter, integer
facts](../../../spec/07-diagnostics-and-conformance.md#integer-facts).

Question: the facts are stated over `compare` and `equal`. A predicate is
written `Ordered.greaterOrEqual(x 1i32)` or `Ordered.inRange(x a b)`. Do
the facts apply through the default members?

Recommended: yes, for the sized integer types only: the checker reads each
default member of `Ordered` as its defining relation. The table names the
members it covers.

## Q22. What `@name.reservedLabel` reports

Where: [diagnostics chapter,
registry](../../../spec/07-diagnostics-and-conformance.md#diagnostics-are-a-language-surface).

Question: no chapter names a reserved label.

Recommended: either state the rule (a labelled parameter or record field
spelled as a prelude value name) or remove the row.

## Q23. Where the registry test is restored

Where: [M0 README](../m0/README.md#scope).

Question: M0 says it restores the diagnostic registry test against the new
table. The code's registry is the v0.0.1 one until the new diagnostics
crate exists.

Recommended: the test is restored in M1 step 02, with the new
`vibra-diagnostics` crate; M0's sentence is corrected.

## Q24. A binary target in M1

Where: [programs chapter,
targets](../../../spec/04-programs-and-packages.md#targets).

Question: M1 has no entry contract. What does `check` report for a project
with a binary target?

Recommended: the project decodes; the target is reported `@tool.unavailable`
at its record with status `unsupported`; library targets of the same
project are checked.

## Q25. A contract call with no implementation

Where: [type chapter, dispatch](../../../spec/02-type-system.md#dispatch).

Question: `Ordered.less(1.0f64 2.0f64)` and `Iter.collect(r)` for a
`Result` have a receiver type with no implementation. No registry row
names that case.

Recommended: `@type.unsatisfiedBound` at the receiver, since the member's
receiver is implicitly bounded by its interface. The plan writes
`@type.mismatch` until this is answered.

## Q26. Name collisions of an associated type

Where: [type chapter, associated
types](../../../spec/02-type-system.md#associated-types).

Question: an associated type spelled like a member of the same interface,
like one of its type parameters, or chosen twice in one `impl`.

Recommended: `@name.memberCollision` for the first, `@name.genericRedeclaration`
for the second, `@name.redeclaration` for the third.

## Q27. A `use` after another declaration

Where: [source chapter, canonical
format](../../../spec/01-source-language.md#canonical-format); [source chapter,
`use`](../../../spec/01-source-language.md#use).

Question: the format says "imports first, in written order". The grammar
admits `use` anywhere at top level. Does the formatter move a late `use`,
or is it an error?

Recommended: the formatter moves nothing. A `use` after a non-`use` form is
`@syntax.invalidForm` at the `use`.

## Q28. An import that collides with a prelude name

Where: [source chapter, `use`](../../../spec/01-source-language.md#use).

Question: `use @app.option enduse` derives `option`, which is free, but
`use @app.x as Option enduse` binds a prelude name.

Recommended: `@name.reservedDeclaration` at the alias, as for any binder.

## Q29. The milestone of local dependencies

Where: [release README](../README.md#scope-and-priorities).

Question: `@project.staleLock`, `@project.dependencyCycle`, and
`@project.ambiguousDependencyTarget` belong to no M1 form, and no milestone
README names local dependencies.

Recommended: M2, with the rest of the project record. The owner table
assumes it.

## Q30. Format atoms of the transitional host documents

Where: [runtime chapter, host world](../../../spec/06-runtime.md#host-world);
[runtime chapter, audit trace](../../../spec/06-runtime.md#audit-trace).

Question: from step 27 until M2 the host crate reads the v0.0.1 world and
trace shapes in the data grammar. What are they called?

Recommended: `@hostWorld.v1` and `@auditTrace.v1`, internal to the
toolchain and its tests, replaced by the major-2 documents in M2.
