# Vibra diagnostics and conformance

Status: normative target
Implementation status: this chapter defines the diagnostics and the evidence
a release of the rewritten language needs. None of that evidence exists yet.
The shipped v0.0.1 evidence describes the earlier language and is not
relabelled.

## Diagnostics are a language surface

Every rejected construct has a structured diagnostic. A diagnostic code is a
stable atom `@<domain>.<condition>`, in camelCase like every other atom. A
code names a condition. It does not encode a severity or a number.

The registry below is the single authority. Every other mention of a code or
its level MUST agree with it.

| Code | Level |
| --- | --- |
| `@syntax.unmatchedDelimiter` | `@error` |
| `@syntax.missingSeparator` | `@error` |
| `@syntax.invalidCharacterLiteral` | `@error` |
| `@syntax.invalidStringLiteral` | `@error` |
| `@syntax.invalidNumericLiteral` | `@error` |
| `@syntax.invalidName` | `@error` |
| `@syntax.retiredForm` | `@error` |
| `@syntax.invalidForm` | `@error` |
| `@syntax.misplacedBinding` | `@error` |
| `@syntax.duplicateSection` | `@error` |
| `@syntax.invalidEncoding` | `@error` |
| `@syntax.invalidSection` | `@error` |
| `@name.unknownSymbol` | `@error` |
| `@name.wrongEntityKind` | `@error` |
| `@name.privateAccess` | `@error` |
| `@name.inaccessibleRepresentation` | `@error` |
| `@name.redeclaration` | `@error` |
| `@name.memberCollision` | `@error` |
| `@name.genericRedeclaration` | `@error` |
| `@name.reservedLabel` | `@error` |
| `@name.reservedDeclaration` | `@error` |
| `@name.reservedValueSpelling` | `@error` |
| `@module.fileDirectoryCollision` | `@error` |
| `@module.sourceIdCollision` | `@error` |
| `@module.caseCollision` | `@error` |
| `@module.testOutsideTests` | `@error` |
| `@module.assertionOutsideTests` | `@error` |
| `@module.unknownPath` | `@error` |
| `@module.missingRequiredImport` | `@error` |
| `@module.importCycle` | `@error` |
| `@module.invalidSegment` | `@error` |
| `@module.pathEscape` | `@error` |
| `@module.ioError` | `@error` |
| `@type.argumentMismatch` | `@error` |
| `@type.typeArgumentMismatch` | `@error` |
| `@type.redundantImplementation` | `@error` |
| `@type.defaultOverride` | `@error` |
| `@type.missingAbstractMember` | `@error` |
| `@type.functionNotEquatable` | `@error` |
| `@type.notApplicable` | `@error` |
| `@type.unknownRecordField` | `@error` |
| `@type.numericOutOfRange` | `@error` |
| `@type.notConstant` | `@error` |
| `@type.initializerCycle` | `@error` |
| `@type.undispatchableContractMember` | `@error` |
| `@type.overlappingImplementation` | `@error` |
| `@type.ambiguousImplementation` | `@error` |
| `@type.unsatisfiedBound` | `@error` |
| `@type.ambiguousDestination` | `@error` |
| `@type.redundantConversion` | `@error` |
| `@type.mismatch` | `@error` |
| `@type.ambiguousInference` | `@error` |
| `@type.infiniteSize` | `@error` |
| `@type.invalidDictKey` | `@error` |
| `@type.invalidTry` | `@error` |
| `@type.unhandledFallible` | `@error` |
| `@type.invalidReturn` | `@error` |
| `@type.missingReturn` | `@error` |
| `@type.invalidInvariant` | `@error` |
| `@type.suspiciousRow` | `@warning` |
| `@type.unreachableCode` | `@error` |
| `@pattern.refutableBinding` | `@error` |
| `@pattern.irrefutableLetElse` | `@error` |
| `@pattern.nonExhaustive` | `@error` |
| `@pattern.unreachableArm` | `@error` |
| `@pattern.unsupportedCoverage` | `@error` |
| `@effect.outsideCeiling` | `@error` |
| `@effect.invalidReference` | `@error` |
| `@effect.duplicateRoot` | `@error` |
| `@effect.invalidAwait` | `@error` |
| `@external.unknownSymbol` | `@error` |
| `@external.ownerMismatch` | `@error` |
| `@external.signatureMismatch` | `@error` |
| `@data.invalidExtension` | `@error` |
| `@data.invalidShape` | `@error` |
| `@data.invalidValue` | `@error` |
| `@data.duplicateField` | `@error` |
| `@data.duplicateKey` | `@error` |
| `@project.staleLock` | `@error` |
| `@project.dependencyCycle` | `@error` |
| `@project.entryOutsideTarget` | `@error` |
| `@project.entryOnLibrary` | `@error` |
| `@project.invalidEntrySignature` | `@error` |
| `@project.ambiguousDependencyTarget` | `@error` |
| `@project.reservedUnitName` | `@error` |
| `@project.overlappingTargetRoots` | `@error` |
| `@project.notFound` | `@error` |
| `@project.invalidTargetRoot` | `@error` |
| `@project.ioError` | `@error` |
| `@runtime.invalidHostValue` | `@error` |
| `@runtime.invalidCheckedProgram` | `@error` |
| `@runtime.memoryExhausted` | `@error` |
| `@style.casing` | `@warning` |
| `@contract.unusedEffect` | `@warning` |
| `@contract.unproved` | `@error` |
| `@contract.refuted` | `@error` |
| `@contract.unsupportedPredicate` | `@error` |
| `@contract.obligationLost` | `@error` |
| `@contract.invalidPredicate` | `@error` |
| `@contract.invalidConsequence` | `@error` |
| `@grant.invalidBinding` | `@error` |
| `@grant.bindingCycle` | `@error` |
| `@grant.requiredUnavailable` | `@error` |
| `@grant.invalidPolicy` | `@error` |
| `@grant.policyTooComplex` | `@error` |
| `@tool.unavailable` | `@error` |
| `@tool.stalePlan` | `@error` |
| `@tool.actionDenied` | `@error` |
| `@tool.limitExceeded` | `@error` |
| `@tool.unsupportedValue` | `@error` |
| `@tool.unsupportedTarget` | `@error` |
| `@tool.missingLinkTool` | `@error` |
| `@tool.linkFailed` | `@error` |
| `@tool.incompatibleArtifact` | `@error` |

The compiler owns this closed registry. Each entry has a level, a domain
(the code's first component), a summary, and a **fix capability**, which is
`@safe` or `@none`. Only `@safe` permits an automatic fix; there is no
"review needed" level between them. The summary's wording is compiler data,
not fixed text.

- A project cannot change a level. A command may fail on warnings by policy
  without relabelling them.
- No source declaration and no data file creates a code.
- A retired code is never reused for another meaning. Changing a level is a
  versioned change of the registry.
- `@command.*` results, `@test.*` results, document format atoms, and the
  cases of ordinary error types are not diagnostics and are not in the
  registry.

### Diagnostic documents and spans

**`DiagnosticDocument`, version 2**, is the closed JSON object
`{schemaVersion, code, level, message, primarySpan, related, notes, fixes}`.

| Field | Content |
| --- | --- |
| `schemaVersion` | `2` |
| `code`, `level` | the exact atoms, as strings |
| `message` | text for a person |
| `primarySpan` | a `SpanDocument` |
| `related` | array of `{span, message}` |
| `notes` | array of strings |
| `fixes` | array of `{description, safe, expectedRevision}`; empty when the code's fix capability is `@none` |

A diagnostic describes a fix and never applies it; the edit is a separate
[plan](05-tooling.md#transactional-edit-plans) that still checks revisions.

**`SpanDocument`** is `{sourceId, start, end, startPosition, endPosition}`.
`start` and `end` are half-open UTF-8 byte offsets on character boundaries,
with `start` ≤ `end`. A position is `{line, column}`, with a one-based line
and a column counted in Unicode scalars. Positions are for display; edits
use offsets.

- Every span names its source. A related span may be in another document. A
  producer given one unnamed source may use a `null` `sourceId`.
- A failure with no location has `sourceId` `null`, offsets 0 and 0, and
  position 1:1. A tool MUST NOT invent a location or blame an unrelated
  declaration.
- Diagnostics sort by source, then primary start and end, then registry
  order. Within one source, lexer diagnostics precede parser diagnostics at
  equal spans.
- One fault is reported once. A later phase does not report the same
  introduction again.
- Tests assert codes, levels, spans, related identities, defined notes, and
  fixes. They do not assert message punctuation.

### Availability, discovery and resolution

**Unavailable is not invalid.** A valid command or construct that the tool's
profile does not support is `@tool.unavailable`. Malformed input keeps its
ordinary code. A missing conformance handler is an unavailable observation,
not a passing one. A smaller profile never gives source a different meaning.

The span of an unavailable construct is the whole form, opener to closer. A
nested expression with its own span owns its own diagnostic. A nested type
with no node of its own uses its parameter or declaration. An existing form
is never reported at the empty location. An unavailable command uses the
command-line context.

| Condition | Code |
| --- | --- |
| no `project.vibon` at or above the workspace | `@project.notFound` |
| the nearest `project.vibon` is malformed (no search continues above it) | its data or syntax code |
| a file with the wrong extension | `@data.invalidExtension` |
| a target or test root that is missing, not a directory, absolute, or escapes | `@project.invalidTargetRoot` |
| target roots that overlap each other or `tests` | `@project.overlappingTargetRoots` |
| a target or dependency named `tests` | `@project.reservedUnitName` |
| other project input or output failure | `@project.ioError` |
| module enumeration or read failure, unreadable or dangling entry | `@module.ioError` |
| an invalid source path segment | `@module.invalidSegment`, at offset 0 of that source |
| two source paths that differ only by case | `@module.caseCollision` |
| a link that leaves the source root | `@module.pathEscape` |
| one source identity for two modules, including dependency inputs | `@module.sourceIdCollision` |
| an import cycle | `@module.importCycle`: primary span on the later edge, related spans along the cycle |
| a dependency cycle | `@project.dependencyCycle` |
| access to a private declaration | `@name.privateAccess` |
| a repeated module, import, local, or test name | `@name.redeclaration` |
| two members of one owner with one name | `@name.memberCollision` |
| a repeated type parameter | `@name.genericRedeclaration` |
| a test module without `use @std.assert enduse` | `@module.missingRequiredImport`, at its first test's name |
| a test outside `@tests`; an assertion used outside `@tests` | `@module.testOutsideTests`; `@module.assertionOutsideTests` |

Resolution reads only the captured source graph.

### Binding, return and pattern diagnostics

Every row is an error with no automatic fix.

| Condition | Code and primary span |
| --- | --- |
| a `let` where a statement is not allowed | `@syntax.misplacedBinding`, the `let` |
| a missing `be`, expression, or `endlet`; crossed or missing closers | `@syntax.invalidForm` or `@syntax.unmatchedDelimiter`, the form or token |
| a repeated section; a section the declaration does not admit | `@syntax.duplicateSection`; `@syntax.invalidSection` |
| a `let` or parameter pattern that can fail, with no `else` | `@pattern.refutableBinding`, the pattern |
| a `let … else` whose pattern cannot fail | `@pattern.irrefutableLetElse`, the pattern |
| an `else` of a `let` that can complete normally | `@type.mismatch`, the `else` body |
| a callable body that can run off its end | `@type.missingReturn`, the body |
| a `return` operand of the wrong type | `@type.mismatch`, the operand |
| a `return` outside a callable, in a module value, or directly in a test | `@type.invalidReturn`, the `return` |
| a statement after one that cannot complete | `@type.unreachableCode`, the first such statement, related to the earlier one |
| a name bound twice, or shadowing | `@name.redeclaration`, the later name, related to the earlier |
| a keyword or prelude name used as a binder | `@name.reservedDeclaration`, the name |
| a prelude spelling used for a module value, function, or import | `@name.reservedValueSpelling`, the introduction |
| a pattern name that resolves to a module value or function that is not constant | `@name.redeclaration`, the name, related to that declaration |
| a labelled default that is not constant | `@type.notConstant`, the expression |
| a `match` that does not cover its type | `@pattern.nonExhaustive`, the `match`, with one uncovered shape |
| coverage the checker cannot decide for this type | `@pattern.unsupportedCoverage`, the `match` |
| an arm that is repeated, already covered, or uninhabited | `@pattern.unreachableArm`, the arm's pattern |
| a type argument the call does not determine | `@type.ambiguousInference`, the application |
| a type argument that does not meet its bound | `@type.unsatisfiedBound`, the actual type, related to the bound |
| an `impl` for `Never` or for an anonymous type | `@name.wrongEntityKind`, the `impl` |

- An explicit `return` in tail position is the normal spelling and is never
  reported as redundant.
- One form yields at most one `@type.unreachableCode`.
- An invalid binding position is reported in place of the type errors it
  would cause downstream.

### Contract, effect and authority diagnostics

| Condition | Code and primary span |
| --- | --- |
| a bare guard is disproved | `@contract.refuted`, the predicate, related to the argument or return that refutes it |
| a bare guard is not established by the rules | `@contract.unproved`, the predicate or the failing application, with the declaration |
| a predicate outside the certified fragment | `@contract.unsupportedPredicate`, the predicate |
| a predicate of the wrong type or shape | `@contract.invalidPredicate`, the predicate |
| a consequence of the wrong type, not constant where required, or not pure | `@contract.invalidConsequence`, the consequence |
| a conversion to a plain callable type that would hide a bare `requires` | `@contract.obligationLost`, the conversion, related to the guard |
| a type invariant that is not a predicate over the representation | `@type.invalidInvariant`, the `where` |
| a row names one interface twice, under any spelling | `@effect.duplicateRoot`, the second entry, related to the first |
| a row entry that is not an operation interface | `@effect.invalidReference`, the entry |
| a body performs an interface outside its ceiling | `@effect.outsideCeiling`, the first `await` that contributes it |
| `await` outside a `proc` or test, or of a value that is not a plan | `@effect.invalidAwait`, the `await` |
| a ceiling names an interface the body cannot perform | `@contract.unusedEffect` (warning), the entry |
| a result type that is a tuple of operation interfaces | `@type.suspiciousRow` (warning), the type |
| an `external` section outside the embedded library | `@syntax.invalidSection`, the section |
| an external symbol not in the registry | `@external.unknownSymbol`, the symbol |
| an external member on the wrong interface | `@external.ownerMismatch`, the symbol, related to the owner |
| an external member whose signature or row differs from the registry | `@external.signatureMismatch`, the declaration, with the expected signature |
| a missing, extra, ill-typed, or unapproved grant binding | `@grant.invalidBinding`, the binding or target record |
| a cycle in `after` | `@grant.bindingCycle`, the later edge, related along the cycle |
| a required grant refused at startup | `@grant.requiredUnavailable`, the binding; a startup result, not a check result |
| an invalid policy, or a policy naming another root | `@grant.invalidPolicy`, the policy value |
| a policy over the size or depth limit | `@grant.policyTooComplex`, the policy |

An `@effect.outsideCeiling` is reported once per body and interface. Its
related spans are the shortest remaining chain of calls, ties in source
order, each with the note `performs <root> via <callee>`; a hop through a
callback or interface member is marked indirect. The query expansion
`effectWitnesses` returns the same chain.

A grant that denies a request at run time returns the operation's typed
error. That is never a diagnostic and never a trap. A check-time failure of
a guard is never turned into a run-time error value by a tool.

### Integer facts

The sixth discharge rule of [Guards](02-type-system.md#guards) uses exactly
the facts below and no other arithmetic reasoning. `x`, `y`, and `z` are
values of one sized integer type. The relations are the results of that
type's `Ordered.compare` and `Equatable.equal`, written here in mathematical
notation, which is not source syntax.

| ID | From | The checker may conclude |
| --- | --- | --- |
| INT-1 | x < y | not (x = y); x ≤ y; y > x |
| INT-2 | x > y | not (x = y); x ≥ y; y < x |
| INT-3 | x = y | y = x; x ≤ y; x ≥ y |
| INT-4 | x ≤ y and y ≤ x | x = y |
| INT-5 | x ≤ y and not (x = y) | x < y |
| INT-6 | x < y and y ≤ z, or x ≤ y and y < z | x < z |
| INT-7 | x ≤ y and y ≤ z | x ≤ z |
| INT-8 | two integer constants | their comparison, by evaluation |

No fact relates the result of an arithmetic operation to its operands:
`add` saturates, so `add(x 1) > x` is not a fact. No fact covers `F32`,
`F64`, or a user implementation of `Ordered`. Adding a fact is a versioned
change to this table, with a positive case and a nearest negative case.

### Runtime and tool failures

- **Defect traps.** The only traps are `@runtime.invalidCheckedProgram`
  and `@runtime.invalidHostValue`. Both mean the toolchain or runtime is
  wrong, not the program's input. A trap has a source origin only when the
  runtime can identify the failing operation; it never blames arbitrary
  source.
- **Memory exhaustion** is `@runtime.memoryExhausted`, a host event and not
  a trap.
- **Tool limits** are `@tool.limitExceeded`, with an unknown, incomplete
  status. The same event is never also reported as a false predicate.
- **Values that cannot be shown** (functions, plans, grants) give an
  encoder error or a structured test failure, with `@tool.unsupportedValue`
  where a diagnostic is shown.
- **Edits and tools.** A stale plan is `@tool.stalePlan`. A denied MCP or
  tool action is `@tool.actionDenied`.
- **Native build** failures are `@tool.unsupportedTarget`,
  `@tool.missingLinkTool`, `@tool.linkFailed`, and
  `@tool.incompatibleArtifact`, with the exact target and toolchain. They
  are tool outcomes, not exit codes of the program.

Checked division and remainder return typed errors. There is no trap for a
failed guard, for division by zero, or for an unobservable value. This is
the contract for supported source; it does not promise that an operating
system failure, a defective driver, or a program that does not terminate can
never end a process.

## Recovery

The parser keeps a lossless syntax tree and recovers after a malformed form,
so that diagnostics and queries stay useful in an unfinished file.

- A recovery step consumes input or returns. It MUST NOT build an
  unambiguous node from ambiguous tokens.
- Later phases use the valid subtrees and suppress errors that only repeat
  an earlier one. They never fill an unknown proof or effect with a
  successful value.
- Missing operands, crossed or missing closers, repeated or unknown labels,
  items with no whitespace between them, invalid tokens, and ambiguous
  applications are errors. A `:` in the wrong place is not reinterpreted.
- A malformed literal and a well-formed literal that is out of range are
  different errors. An unterminated quote is `@syntax.unmatchedDelimiter`
  only. An unknown numeric suffix is `@syntax.invalidNumericLiteral`. A
  malformed identifier is `@syntax.invalidName`. `_` is the only discard.
- Each fact is `exact`, `recovered`, or `unavailable` on its own. A valid
  node beside a recovery stays exact. A partial performed row is not an
  exact ceiling.
- The formatter keeps ambiguous bytes. A name-case convention gives
  `@style.casing`; formatting never renames and never reorders evaluation.

## Conformance corpus

The implementation MUST maintain a corpus of Vibra cases that is independent
of any backend and organized by the rule each case tests. A case has a
stable rule identity, an operation, its inputs, a profile, and the expected
verdict, diagnostics, and observations. Expected results are written from
this specification. They are never regenerated from an engine's output.

### Case operations and inputs

| Operation | Inputs | Observation |
| --- | --- | --- |
| `reader` | one `.vib` or `.vibon` document | the lossless tree, recovery, diagnostics; optionally a structural query or format |
| `projectDecode` | exactly one `project.vibon` | the typed decode and diagnostics, with no discovery |
| `sourceGraph` | one confined tree with its `project.vibon` | the source graph snapshot |
| `resolve` | the same | resolved identities, imports, and references |
| `workspaceCheck` | the same; no single-source input | the static result; optionally checked types and obligations |
| `format` | the same, plus one document | the formatted text and diagnostics |
| `query` | a captured workspace, a subject, an offset, options | the versioned query result |
| `interpret` | one pure module, checked as a one-module workspace; its first function with no parameters is called | the canonical result or an observation failure; no host world |
| `workspaceRun` | the same tree, one binary target | startup, the entry's result, and the ordered audit trace in the supplied world |
| `workspaceTest` | the same tree | one suite result with each test's trace, each test in a fresh world |

- A case with several kinds of input states its operation.
- Paths are relative to the case, confined, and checked by extension. The
  loader captures the tree first; a handler reads only that capture.
- `interpret`'s entry is a fixture for observing values. It is not an
  exception to the entry contract of a binary.
- `workspaceRun` and `workspaceTest` may name one `@hostWorld.v2`
  document, inside the case directory and outside the source tree. Without
  it the default deterministic world applies. Every test and every executor
  gets a fresh copy. A malformed world fails only its case, once, and never
  becomes an empty permissive world. World data describes inputs; the fake
  broker issues the grants.
- An expectation is `accepted`, `refuted`, `unknown`, or `unsupported`. Only
  the first is a pass. A case for a static rule includes a passing example
  and the nearest failing one.
- A handler that does not exist is unavailable and fails any gate that needs
  it.

### Persistent observations

Snapshots are canonical typed VIBON, never text dumps. Source bytes are
data inside them; source is never parsed as VIBON. A change of shape is a
new major format.

| Format | Shape |
| --- | --- |
| `@sourceGraph.v1` | `format`, `units`, `dependencies`. A unit has `name`, `kind`, `modules`; a module has `path`, `source`, `bytesHex`; a dependency has `alias`, `kind`, `source`, optional `target`, `status`, `sourceId`, `span`. |
| `@resolved.v2` | `format`, `package`, `modules`, `declarations`, `imports`, `references`. A declaration has its logical anchor and identity, kind, visibility, source, and span. |
| `@types.v2` | `format`, `moduleValues`, `callables`, `obligations`, with checked types, complete rows, and each obligation's status, rule identifiers, dependencies, and counterexample. |
| `@index.v2` | the [index](05-tooling.md#index-records) |
| `@value.v2` | the [canonical value](06-runtime.md#canonical-value-encoding) |
| `@auditTrace.v2` | `format`, `events`: the [audit trace](06-runtime.md#audit-trace) |
| `@testRun.v2` | `format`, `result`, `tests` |

- `@sourceGraph.v1` keeps its version because its meaning is unchanged:
  exact source bytes in lower-case hexadecimal, no parsed declarations.
  Every graph has the `@tests` unit, even when empty. Keeping `v1` does not
  mean old source is accepted.
- Field order is the order listed. Arrays are in identity order unless
  execution orders them.
- A test observation has `name` (the selector), `result`, an optional
  `failure` or `trap`, and `auditTrace`. A `failure` has `kind`,
  `assertion`, `expected`, `actual`, `primarySpan`, and an optional
  `observationError`. A `trap` has `trapCode` and an optional `origin`.
  Absent optional fields are omitted, not null. A stored span is
  `{source-id, start, end}`.
- Each test has its own complete trace; there is no suite-level trace. An
  invalid or unavailable test has an empty one. Diagnostics are not copied
  into test observations.
- An empty suite has result `@command.ok` and no tests.
- A host event is expected with `expect.host_event` and the event's atom. A
  case MUST NOT depend on the exact allocation at which memory runs out. A
  tool limit is expected as unknown, with the limit profile.
- An execution that traps records the code and origin and no value. Its
  completed events are kept. The absence of an event is never evidence that
  nothing external happened.

### Rule identities and example coverage

A rule identity is a section prefix and a descriptive suffix in lower case
with hyphens. The prefixes are `CHARTER`, `SRC-READER`, `SRC-CALLS`,
`SRC-DECL`, `SRC-EXPR`, `SRC-FMT`, `TYPE-NOMINAL`, `TYPE-NAMES`,
`TYPE-INFER`, `TYPE-GENERIC`, `TYPE-INTERFACE`, `TYPE-CONVERT`,
`TYPE-CONTROL`, `EFFECT`, `PROJECT`, `TOOL`, `RUNTIME`, `DIAG`, `CONTRACT`,
and `GRANT`, each under `V1-` or `V2-`. A `V1` identity is kept only for a
rule that did not change. A changed or new rule gets a `V2` identity. An
identity is never reused; a removed rule stays in the index as retired.

Every accepted source example in this specification is a corpus case, or is
marked schematic when it is not complete grammar. Every error rule has a
focused negative case. Marking a required behavior schematic does not
implement it.

## Required coverage

These are minimum obligations. None of these tests exists yet.

1. **Reader and format.** Every literal category and the Unicode boundaries;
   whitespace separation; unmatched and crossed closers; malformed names and
   retired forms; recovery followed by a valid declaration; source and data
   kept apart by extension; comments and doc strings; every byte kept in the
   tree; formatting is idempotent and renames and reorders nothing.
2. **Registry.** Every code is produced by a focused case and has exactly
   one entry; levels, domains, and fix capabilities match; result and format
   atoms are not entries; fixes respect revisions.
3. **Names and graphs.** Separate namespaces; field reads against paths;
   import names and `as`; private access; no shadowing; prelude
   reservations; source-identity and case collisions; link escapes; cycles
   of imports, packages, startup recipes, and module values rejected;
   recursive functions and finite recursive types accepted.
4. **Bindings and patterns.** `let` with and without a written type; names
   visible only after `endlet` and absent in `else`; sibling scopes reuse
   names; exact-length list and tuple patterns; omitted record fields;
   constants against binders; `_`; parameter patterns that cannot fail;
   coverage and unreachable arms for variants, `Bool`, products, and open
   literal types; no coverage inferred from a `where` predicate; a constant
   construction the checker evaluates, with unknown and refuted
   counter-cases.
5. **Control and inference.** `return` on every path; nested returns leave
   the nearest callable; tests reject `return` and `try`; `let … else` whose
   `else` cannot complete; `Never`; `try` on `Option` and `Result` with the
   exact error type; expected types flow inward only; an ignored fallible
   value against an explicit discard; ambiguous numbers, empty collections,
   and generics; no numeric promotion.
6. **Applications and values.** Any expression as callee; operands in
   written order; labels and defaults; captures; constructors and wrong
   arity, unknown, and repeated fields; projection and lookup add no effect;
   constructors and projections are not function values; same shape,
   different declared type; record identity ignores field order while
   evaluation keeps written order; dict keys, later duplicate wins, key
   order, canonical encoding; empty list and dict with and without context.
7. **Interfaces and generics.** Abstract and default members; missing
   member, overridden default, overlap, and orphan errors; one bound per
   parameter; receiver and destination dispatch; distinct identities for
   applied interface, receiver, and member; `From` and `TryFrom` overlap and
   the fixed conversion error; no call-site type arguments; the two
   widenings, with no chaining and no container covariance; an interface
   value implements no interface; `Self` results allowed and other `Self`
   inputs rejected. No algebraic law is assumed of a user `Ordered` or
   `Equatable`.
8. **Pure library.** Shared `Iter` algorithms; `next` returns a remainder of
   the same type; lazy `map`, `filter`, `skip`, `take`; strict left `fold`;
   short-circuit operations do not drain; explicit implementations for
   `List`, `Dict`, `Str`, `Option`, `Bytes`; `Ordered` defaults with one
   comparison and left-biased ties; the shared lexicographic helper; ASCII
   `trim`; eager `Bool` operations against `if`. `Result` is not iterable.
9. **Contracts.** Each discharge rule and each integer fact, with a passing
   and a nearest failing case; bare `requires`, `ensures`, and `where`;
   unproved and refuted; the certified fragment; every return, including
   through `try`; a rejected `requires … else` skips the body and `ensures`;
   an `ensures … else` consequence differs from the candidate, is evaluated
   once, and is not checked again; a constant `where … else` error, with the
   constructor's type unchanged when the check is folded; no circular proof,
   no proof from samples; conversion to a callable type keeps `else` guards
   and rejects a hidden bare `requires`.
10. **Effects and authority.** Complete rows through callbacks, records,
    closures, and plans; owner plus additive row on interface members; no
    `await` in a `fn`; building against running a plan; guards with `else`
    run on each `await`; each `await` is a new invocation, with no implicit
    retry; wrong, omitted, and duplicate rows; shortest witnesses; required
    and optional grants; brands; forged, stale, and foreign grants;
    narrowing against interface views; limits never reset.
11. **Policies and drivers.** Whole rules stay correlated; restriction keeps
    the parent; normalization and size limits; a read-only directory chosen
    at run time with a `.txt` rule; the real name of an entry; prefix by
    names, not by string; reparse points, links, mounts, renames, and a
    replaced root; no cached intermediate handle; hard-link and
    opened-object limits shown at approval; shared counters under siblings.
    The reserved HTTP rules: correlation, prefixes, queries, rebinding, and
    redirects. Passing in the fake world does not qualify a real adapter.
12. **Host registry and entry.** Every operation, with success and each
    typed error, in fresh worlds; standard input read twice; output captured
    under test; sorted directory entries; create and replace; invalid paths
    and text; present and absent variables; exact arguments; fake clock
    steps and random vectors. Events are recorded once, in order; a trap or
    exhaustion invents no event; projections restore no authority. Binding
    order, startup failure, optional absence, release order, and non-zero
    exit codes stay distinct from tool exits and traps.
13. **Runtime and native.** Tail calls through direct, indirect, and
    cross-module callees, closures, and interface members reuse their
    activation for at least 100,000 transfers. Non-tail recursion 100,000
    deep completes within context memory, with no native stack frame per
    activation. Unbounded recursion ends in the memory host event. Deep
    release is iterative; live and peak storage are accounted at counts that
    differ by ten; module values initialize once per run. A pending
    `ensures … else`, widening, or cleanup prevents a tail transfer.
14. **Tooling and approvals.** One engine and one revision across CLI and
    MCP; exact, recovered, and unavailable facts; roles by slot; stable
    implementation identities; bounded output; schema validation; stale and
    replaced plans, changed policy, and a dirty tree rejected; tampering
    with a protected test body, helper, fixture, or selection rejected,
    while the code under repair stays editable; no identity change on a body
    edit, and a visible one on rename or move; atomic edits across files;
    interrupted application; vendored and escaping paths denied; MCP
    permissions make no grant.

## Conformance profiles

A profile is a versioned statement of what a tool supports. It is not a
dialect and not a release number. A valid feature outside a profile is
reported unavailable; the same source never means something else under a
smaller profile.

| Profile | Covers |
| --- | --- |
| `readerV2` | the source and VIBON readers, recovery, canonical format, syntax diagnostics |
| `staticV2` | `readerV2` plus names, types, guards, effect rows, project and binding validation |
| `interpreterV2` | `staticV2` plus reference execution, the operation registry, fake and system worlds |
| `toolingV2` | `staticV2` plus the query, edit, schema, CLI, and MCP contracts in the reported inventory |
| `nativeV1` | `staticV2` plus verified execution IR, the native runtime, code generation, linking, launch, and native observations |
| `wasmTransitionV2` | the transitional WebAssembly path, until native execution is qualified |
| `fullV2` | everything a release requires: language, library, projects, tooling, interpreter, and native targets |

`nativeV1` does not include `interpreterV2`; a differential gate needs
both. The WebAssembly profile, its backend, and its dependencies are removed
together when native execution is qualified. The interpreter stays, for
`run`, `test`, `eval`, and compile-time evaluation.

### Differential execution

An executable case is an accepted `interpret`, `workspaceRun`, or
`workspaceTest` case. It has one expected result, written from the
specification, and not one per backend.

- Each required executor runs it in its own fresh, equivalent world. The
  comparison covers canonical values, exit codes, typed errors, assertion
  outcomes, trap codes and origins, startup results, and the ordered audit
  events. A mismatch names the executor and the first difference.
- Before the WebAssembly backend is removed, its supported cases also run
  there. Afterwards the pair is interpreter and native, with the
  execution-IR stepper used to isolate lowering defects.
- Every stepper and native case runs with an unbounded scheduler quota and
  with a quota of one. The observations MUST agree. Equal step counts
  between engines are not required.
- A memory or tool limit is recorded, and is never a portable threshold or
  a proof of termination.
- Native qualification is actual execution on Windows x64, Linux x86-64,
  and macOS arm64. Emitting an object file for a platform is not evidence.
- Every admitted source form lowers, verifies, and emits completely, or the
  profile fails. A program is never partly executed.
- The parity inventory lists every executable case once, with its owner. A
  listed match that fails, fails the gate.
- A new feature updates the IR instruction, its verification, the runtime,
  lowering, the stepper, the generator, the observations, and the cases
  together.
- For each natively accelerated library function, shared sample vectors
  compare the interpreter running its Vibra body, the interpreter and native
  runtime running the accelerator, and a native executable. Agreement on
  those vectors is sampled evidence, not proof.

The runner reports, per profile and executor, the counts of passed, failed,
unknown, and unavailable cases. There is no "skipped" count that reads as
success, and a missing executor is reported missing. Expected results are
never regenerated from native or stepper output to make a failure pass.

## Required implementation suites

Two independent suites are mandatory.

- **Host-language tests** cover algorithms, parsers, adapters, and failure
  paths.
- **Vibra conformance tests** cover observable language, project, tooling,
  and runtime behavior. Continuous integration runs the corpus through a
  real profile handler in its own job; a failed, unknown, or unavailable
  required case fails the job.

Both run offline once dependencies are acquired. Required jobs:

- formatter idempotence;
- every JSON producer against its schema;
- decode and canonical round-trip of every persistent format;
- every example of this specification;
- interpreter and native parity;
- reproducible builds from two clean checkouts at different paths, with
  pinned toolchains;
- a snapshot per registry major version that fails if an existing row
  changes;
- architecture tests that semantic, checking, lowering, and execution code
  reaches host effects only through the registry and provider path.

Fuzzing covers the lexer and parser recovery, formatting, the project, lock,
build, policy, approval, and world decoders, CLI and MCP inputs, the
external registry, the execution-IR verifier, and artifact admission. A
finding is closed by a reduced regression case, never by ignoring a seed or
loosening a decoder.

When concurrency is added, its suites add recorded schedules, backpressure,
exit, late replies, ownership stress, and evidence of real progress on two
workers. A replay that passes does not show that every interleaving was
explored.

## Full conformance gate

A `fullV2` claim requires all of the following.

- The linked evidence of every milestone the release requires.
- No contradiction between chapters and no placeholder decision.
- Every example classified and exercised.
- A complete registry with one entry per code.
- Every machine schema versioned and validated by its producer.
- Every persistent format validated and round-tripped.
- Both suites passing on the published platforms.
- Complete interpreter and native parity, unchanged by yielding.
- Reproducible offline builds from different paths, before signing.
- Canonical formatting of the repository's own source.
- An audit of target, driver, policy, and authority admission.
- A statement of what the release supports and excludes.

Passing tests is necessary and not sufficient. These properties need a
written review, each with a named owner: no ambient effects in source, sound
removal of discharged guards, no amplification of grants, private
representations kept private, a closed set of host operations, confinement
to the opened object, and no access to the native bridge from outside it.

A model-writing pilot of the syntax is not conformance, driver feasibility,
a security proof, or the clean-machine acceptance of the CLI.

## Exclusions and retired surfaces

Not part of this language, and not accepted under any compatibility mode:

- untagged unions and anonymous enums;
- type ascription and `as` narrowing;
- tuple indexing and variadic calls;
- loops and assignment;
- call-site type arguments;
- a source form for declaring diagnostics;
- traps for failed guards, for integer division, and for unobservable
  values;
- grants made from decoded data;
- proof by sample, a general solver, and cost guarantees;
- static identity of a resource instance.

Their codes are retired and never reused: the union, `as`, narrowing, and
tuple-index codes, the redundant-return code, `@style.argumentOrder`,
`@runtime.divisionByZero`, and `@runtime.unobservableFunction`.

Full conformance does not claim an operating-system sandbox for foreign
native code, freedom from toolchain or host failure, exploration of every
schedule, or recovery of every service from a crash.
