# Vibra diagnostics and conformance

Status: normative target
Implementation status: this chapter defines the diagnostics and the evidence
a release needs. None of that evidence exists yet.

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
| `@type.unsupportedSelfPosition` | `@error` |
| `@type.overlappingImplementation` | `@error` |
| `@type.ambiguousImplementation` | `@error` |
| `@type.unsatisfiedBound` | `@error` |
| `@type.ambiguousDestination` | `@error` |
| `@type.redundantConversion` | `@error` |
| `@type.mismatch` | `@error` |
| `@type.ambiguousInference` | `@error` |
| `@type.infiniteSize` | `@error` |
| `@type.depthExceeded` | `@error` |
| `@type.expandingInstantiation` | `@error` |
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
| `@project.duplicateName` | `@error` |
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
| `@grant.acquisitionFailed` | `@error` |
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
not fixed text. Every code in the registry has at least one trigger row in
the tables of this chapter, naming its condition and its primary span, and
every code any chapter cites is in the registry.

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
  order.
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
| a file with the wrong extension | `@data.invalidExtension`, at offset 0 of that document |
| a target or test root that is missing, not a directory, absolute, or escapes | `@project.invalidTargetRoot`, the target's `root` value; for the test root, offset 0 of the source `tests` |
| target roots that overlap each other or `tests` | `@project.overlappingTargetRoots`, the later target's `root` value, or the `root` value of the target that overlaps `tests`; no related span |
| a target or dependency named `tests` or `std` | `@project.reservedUnitName`, the target's `name` value or the dependency's key |
| two targets with one name, or a target and a dependency with one name | `@project.duplicateName`, the later name, a target's `name` value or a dependency's key, related to the earlier one |
| other project input or output failure | `@project.ioError` |
| module enumeration or read failure, unreadable or dangling entry | `@module.ioError`, at offset 0 of the entry's project-relative path |
| an invalid source path segment | `@module.invalidSegment`, at offset 0 of that source |
| two source paths that differ only by case | `@module.caseCollision`, at offset 0 of the later path in source-identity order; no related span |
| a link that leaves the source root | `@module.pathEscape`, at offset 0 of the entry's project-relative path |
| one source identity for two modules, including dependency inputs | `@module.sourceIdCollision`, at offset 0 of the project module |
| an import cycle | `@module.importCycle`: primary span on the later edge, related spans along the cycle |
| a dependency cycle | `@project.dependencyCycle`, the later dependency entry in the project file, related along the cycle |
| a `.vib` file beside a directory of the same name | `@module.fileDirectoryCollision`, at offset 0 of that source |
| a path whose unit is not visible to the importing module, or that reaches no module | `@module.unknownPath`, the reference |
| a lock whose `project` digest or dependency set differs from the project; a missing vendor directory or vendored content whose content hash differs from the lock | `@project.staleLock`, the lock's `format` field or the lock entry |
| a target `entry` whose first part is not the target's name | `@project.entryOutsideTarget`, the `entry` field |
| an `entry` on a library target | `@project.entryOnLibrary`, the `entry` field |
| an `entry` that is not a module-level `proc` with an entry signature, or that has a bare `requires` | `@project.invalidEntrySignature`, the `entry` field, related to the declaration |
| a dependency without `target` on a package with several library targets | `@project.ambiguousDependencyTarget`, the dependency entry |
| access to a private declaration | `@name.privateAccess`, the reference, related to the declaration |
| use of a hidden constructor, pattern, or field read | `@name.inaccessibleRepresentation`, the use, related to the type |
| a name that resolves to nothing: a path part, a `given` key, an `impl` member or choice that is no member of the interface, an entity reference slot | `@name.unknownSymbol`, the name |
| an entity of the wrong kind: an import of a member, an `impl` target of the wrong kind, a bound that is not an interface, a typed data slot | `@name.wrongEntityKind`, the reference |
| a repeated module, import, local, test, or `given` key name; an associated type chosen twice in one `impl` | `@name.redeclaration`, the later name, related to the earlier |
| two members of one owner with one name, an associated type and another member of its interface included; a callable member supplied twice in one `impl` | `@name.memberCollision`, the later name, related to the first |
| a repeated type parameter; an associated type with the name of a visible type parameter | `@name.genericRedeclaration`, the later parameter, the associated type, or the `impl` head |
| a reserved word in a label position | `@name.reservedLabel`, the word |
| a test module without an import that resolves to `@std.assert` | `@module.missingRequiredImport`, at its first test's name |
| a test outside `@tests`; outside `@tests`, a use of a declaration bound to an [assertion row](06-runtime.md#assertion-operations) | `@module.testOutsideTests`; `@module.assertionOutsideTests`, the use |

Resolution reads only the captured source graph.

### Syntax and data diagnostics

| Condition | Code and primary span |
| --- | --- |
| a byte-order mark, invalid UTF-8, or NUL | `@syntax.invalidEncoding`, the offending bytes |
| two adjacent tokens, each of the classes identifier, reserved word, number, atom, string, character, or doc string, with nothing between them; a bracket or `:` beside a token never triggers it | `@syntax.missingSeparator`, a zero-width span at the second token |
| an empty, multi-scalar, or surrogate character literal; one with no closing quote on its line | `@syntax.invalidCharacterLiteral`, the token, which ends before the line break |
| a terminated string with a malformed or unsupported escape | `@syntax.invalidStringLiteral`, the token |
| an unknown numeric suffix, an integer suffix on a float body, or trailing token text | `@syntax.invalidNumericLiteral`, the token |
| a malformed identifier or atom | `@syntax.invalidName`, the token; the token gets no `@syntax.invalidForm` |
| a form the grammar does not admit; a doc string outside `doc`; a tag outside a section's closed set; a VIBON form outside the data grammar | `@syntax.invalidForm`, the smallest enclosing construct |
| a `(` where a top-level form or a statement is expected | `@syntax.invalidForm`, once, the balanced parenthesized form |
| `as` outside a `use` | `@syntax.invalidForm`, the `as` |
| a `use` after another top-level form | `@syntax.invalidForm`, the `use` |
| a `fn` or `proc` without `do` that is neither an abstract interface member nor an external definition | `@syntax.invalidForm`, its closer |
| an identifier other than `true` and `false`, a doc string, or a character that starts no token in a VIBON document | `@syntax.invalidForm`, the token |
| an application in a VIBON document | `@syntax.invalidForm`, once, its first token |
| a reserved word as a VIBON field name | `@syntax.invalidForm`, the word |
| a VIBON field or entry with no value | `@syntax.invalidForm`, the name or key and its `:` |
| a VIBON document that holds no value | `@syntax.invalidForm`, the whole document |
| a section before the declaration's name, or after `do` | `@syntax.invalidForm`, that section |
| a reserved word in a declaration or binder position; a prelude name declared as a type, interface, type parameter, or binder; a reserved type head declared | `@name.reservedDeclaration`, the name |
| a repeated record field in VIBON | `@data.duplicateField`, the later field |
| a repeated dict key in VIBON, after decoding | `@data.duplicateKey`, the later key |
| a VIBON `format` the reader does not implement; a literal of the right form whose value a schema slot does not admit; fields that relate wrongly | `@data.invalidValue`, the value |
| a missing required field, an unknown field, or a value of the wrong form in VIBON | `@data.invalidShape`, the field or the value |

### Type and interface diagnostics

| Condition | Code and primary span |
| --- | --- |
| an argument that does not fit its slot, a wrong arity, an unknown, duplicate, or missing label; a constructor pattern of the wrong shape; an argument for a `Self` parameter outside the receiver slot that does not have the receiver's type, or that is supplied through an interface value | `@type.argumentMismatch`, the argument, the application, or the pattern |
| a wrong type-argument count, an unknown or repeated label, a label in an `impl` target, or an interface value type with an open associated type | `@type.typeArgumentMismatch`, the label or the type application |
| application of a value that is not applicable | `@type.notApplicable`, the callee |
| a field read of a value without that field | `@type.unknownRecordField`, the field name |
| a literal that does not fit its type | `@type.numericOutOfRange`, the literal |
| a module value whose initializer reaches itself | `@type.initializerCycle`, the `let`, related along the cycle |
| a declared type whose expansion repeats | `@type.infiniteSize`, the declaration whose expansion first repeats |
| a type nested more than 64 levels | `@type.depthExceeded`, the written type, or the expression whose type it is |
| a generic callable that reaches itself at a type built around one of its own type parameters | `@type.expandingInstantiation`, the call or callable reference with the expanding type argument, related along the cycle |
| an inadmissible dict key type | `@type.invalidDictKey`, the key type |
| a callable as a dict key, or an `assert.equal` operand whose type contains a callable, plan, or grant | `@type.functionNotEquatable`, the type or the operand |
| `try` on a wrong carrier, in a callable with another carrier, or directly in a test | `@type.invalidTry`, the `try` |
| a fallible value left unused as a statement | `@type.unhandledFallible`, the statement |
| an `impl` inside an interface that targets a type of the same module | `@type.redundantImplementation`, the `impl` head |
| a default member supplied again by an implementation | `@type.defaultOverride`, the member's name, related to the default |
| an abstract member or an associated type an implementation does not supply | `@type.missingAbstractMember`, the `impl` head, related to the contract member |
| two implementations of one receiver whose targets unify | `@type.overlappingImplementation`, the later `impl` head, related to the earlier |
| two implementations still candidates at a call | `@type.ambiguousImplementation`, the application |
| a contract member with no receiver slot whose written result type does not contain `Self` | `@type.undispatchableContractMember`, the member's name |
| a contract member whose written result type places `Self` outside the [`Self` map](02-type-system.md#interfaces), found when the interface declaration is checked, whether or not the interface is used as a value | `@type.unsupportedSelfPosition`, the result type |
| a destination-dispatched call with no written expected type | `@type.ambiguousDestination`, the application |
| `From` and `TryFrom` sources of one receiver that unify | `@type.redundantConversion`, the later `impl` head, related to the earlier |
| a receiver or destination with no implementation for the selected key; a call on a type parameter of a member its bound does not give; a label-bound parameter that fails its own bound | `@type.unsatisfiedBound`, the operand or the expected type, related to the interface or bound |
| an implementation member whose signature differs from the contract; a widening whose label differs from the implementation's choice; a label-bound parameter fixed to two types; a deferred associated equality or a complete result that disagrees after selection; a destination-dispatched call whose expected type has no position for `Self` or gives `Self` two types, at the application | `@type.mismatch`, the differing type or expression |

### Binding, return and pattern diagnostics

Every row is an error with no automatic fix.

| Condition | Code and primary span |
| --- | --- |
| a `let` where a statement is not allowed | `@syntax.misplacedBinding`, the `let` |
| a missing `be`, expression, or `endlet`; crossed or missing closers | `@syntax.invalidForm` or `@syntax.unmatchedDelimiter`, the form or token |
| a repeated section; a section the declaration does not admit | `@syntax.duplicateSection`, the keyword of the later section; `@syntax.invalidSection`, the section |
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
| a type argument the call does not determine, including one still undetermined when no bound is resolvable | `@type.ambiguousInference`, the application |
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
| an entry row or startup step row outside the target's `effects` | `@effect.outsideCeiling`, the target's `effects` field in the project file, related to the entry's `performs` entry or to the recipe |
| an `effects` entry that neither the entry nor a startup step performs | `@contract.unusedEffect` (warning), that entry of the list |
| a missing, extra, ill-typed, or unapproved grant binding; an unknown, repeated, or missing `after` name; a `source` leaf of the wrong kind | `@grant.invalidBinding`, the binding, its field, or the `grants` record |
| a cycle in `after` | `@grant.bindingCycle`, the later edge, related along the cycle |
| a required grant refused at startup, in a case its issuer lists as a refusal | `@grant.requiredUnavailable`, the leaf's binding recipe in the project file; a startup result, not a check result |
| an acquisition that fails at startup, in a case its issuer lists as a failure, for a required or an optional grant | `@grant.acquisitionFailed`, the leaf's binding recipe in the project file; a startup result, not a check result |
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
notation, which is not source syntax. For the sized integer types, and for
them only, the checker reads each of these default members of `Ordered` as
its defining relation, so the facts apply to a predicate written with one:
`less(x y)` is x < y, `lessOrEqual(x y)` is x ≤ y, `greater(x y)` is x > y,
`greaterOrEqual(x y)` is x ≥ y, and `inRange(x low high)` is low ≤ x and
x ≤ high. `min` and `max` are not relations and are not covered.

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

| Condition | Code and primary span |
| --- | --- |
| impossible typed IR, descriptor, or continuation; an unknown engine trap; a defect of the embedded standard library found at load | `@runtime.invalidCheckedProgram`, the source origin when known, otherwise none |
| an invalid, stale, foreign, or wrong-kind boundary value; an unsealed access; a malformed trusted result | `@runtime.invalidHostValue`, the source origin when known, otherwise none |
| memory exhaustion | `@runtime.memoryExhausted`, no location; a host event, not a trap |
| a name that departs from the casing convention | `@style.casing` (warning), the name |
| a valid command or construct outside the tool's profile; a driver the platform does not support, at admission | `@tool.unavailable`, the whole form, or the command-line context |
| a plan whose workspace or document revision no longer matches | `@tool.stalePlan`, no location |
| an MCP or tool action the server's permissions deny | `@tool.actionDenied`, no location |
| a tool limit reached, with an unknown, incomplete status | `@tool.limitExceeded`; the predicate when the limit stopped an obligation, whose status is then `unknown` and which gets no `@contract.unproved`; otherwise no location |
| a value that cannot be shown: a function, a plan, or a grant, including one an interface value holds | `@tool.unsupportedValue`, the observed expression when one exists |
| a build for a platform the toolchain does not emit | `@tool.unsupportedTarget`, no location |
| a missing platform linker | `@tool.missingLinkTool`, no location |
| a failed link step | `@tool.linkFailed`, no location |
| an artifact whose ABI, registry, or identities the runtime cannot admit | `@tool.incompatibleArtifact`, no location |

- Both traps mean the toolchain or runtime is wrong, not the program's
  input. A trap has a source origin only when the runtime can identify the
  failing operation; it never blames arbitrary source.
- A tool limit is never also reported as a false predicate.
- A value that cannot be shown gives an encoder error or a structured test
  failure, with `@tool.unsupportedValue` where a diagnostic is shown.
- Native build failures name the exact target and toolchain. They are tool
  outcomes, not exit codes of the program.

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
- A closer that does not match the innermost open construct is resolved by
  kind. If a construct of the closer's kind is open, every construct inside
  it closes as missing and the closer closes that construct, with one
  `@syntax.unmatchedDelimiter` at the closer, related to the innermost
  opener. So an `endfn` met while a `match` is open inside a `fn` closes
  the `match` as missing and then the `fn`. If no construct of the closer's
  kind is open, nothing closes: the closer is kept as its ordinary token
  leaf inside an `@error` node of the
  [tree](#persistent-observations), and the same code is reported at it.
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
- A case names its inputs with the keys `source` (a source document),
  `project` (a project document), `data` (a list of data documents), `tree`
  (a confined tree, whose project document is `<tree>/project.vibon`), and
  `hostWorld` (a host world document). The keys of a `query` case's subject,
  offset, and options are not defined.
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
- An expected verdict is `accepted`, `refuted`, `unknown`, or `unsupported`.
  A case passes when its observed verdict equals the expected one and every
  other expectation is met. A case for a static rule includes a passing
  example and the nearest failing one.
- A handler that does not exist is unavailable and fails any gate that needs
  it.

### Persistent observations

Snapshots are canonical typed VIBON, never text dumps. Source bytes are
data inside them; source is never parsed as VIBON. A change of shape is a
new major format. Every schema below follows the
[schema rules](04-programs-and-packages.md#schemas): absent optional fields
are omitted, lists are in the order stated, and a **span record** is
`{sourceId: Str start: U64 end: U64}`.

| Format | Defined in |
| --- | --- |
| `@tree.v2`, `@sourceGraph.v1`, `@resolved.v2`, `@types.v2`, `@testRun.v2` | below |
| `@index.v2` | the [index](05-tooling.md#index-records) |
| `@type.v2`, `@value.v2` | the [canonical value encoding](06-runtime.md#canonical-value-encoding) |
| `@auditTrace.v2` | the [audit trace](06-runtime.md#audit-trace) |

**`@tree.v2`** holds the lossless syntax tree that a `reader` case compares.

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@tree.v2` | required |
| `root` | node | the one document node |

A node is one of two closed records. An interior node is
`{kind: K children: C}`, with `C` a list of nodes in source order that may
be empty. A leaf is `{kind: K text: Str}`, holding the exact text of one
token. The texts of the leaves, concatenated in order, equal the input, so
whitespace and comments are leaves.

One rule decides which productions of the
[source grammar](01-source-language.md#grammar) and the
[data grammar](04-programs-and-packages.md#vibon-data-documents) are nodes:

- The root is the one **document node**, of kind `@source` for a `.vib`
  document and `@data` for a `.vibon` document. It stands for the whole
  document, and no node stands for `module`.
- A **choice production**, one whose body is only alternatives, is never a
  node: `top`, `statement`, `expr`, `unary`, `primary`, `literal`,
  `argument`, `pattern`, `type-expr`, and `data` among them. What a choice
  production matches takes its place: a node, when the chosen alternative
  is a node production, and otherwise the tokens of an alternative written
  inline, which become children of the nearest enclosing node.
- Every other production is an interior node whose `kind` is the
  production's name in camelCase: `@fnDecl`, `@postfix`, `@list`, `@entry`.
- A lexical production is a leaf. A leaf `kind` is `@identifier`,
  `@reservedWord`, `@integer`, `@float`, `@atom`, `@string`, `@docString`,
  `@character`, `@punctuation`, `@whitespace`, `@comment`, or `@missing`.
  The discard `_` is a `@punctuation` leaf, and so is each character that
  starts no token.
- Recovery is recorded with two kinds, never with flags. An `@error` node is
  interior and holds, as their ordinary token leaves, the tokens a recovery
  step consumed without building a form; an unmatched closer is its
  `@punctuation` or `@reservedWord` leaf inside an `@error` node. A
  `@missing` leaf has empty text and stands where a required element was
  not written.
- A document that is not valid UTF-8 has no tree; its observation is its
  diagnostic.

The valid document `[1u8]`, five bytes with no newline, has this tree. The
element is matched by the choice production `data`, so the integer leaf is
a child of the list node:

```vibon
{
  format: @tree.v2
  root: {
    kind: @data
    children: [
      {
        kind: @list
        children: [
          {kind: @punctuation text: "["}
          {kind: @integer text: "1u8"}
          {kind: @punctuation text: "]"}
        ]
      }
    ]
  }
}
```

The malformed document `[1u8}` recovers as follows. No brace form is open,
so the `}` closes nothing and is kept in an `@error` node; the list then
ends without its closer, which a `@missing` leaf marks. Both faults are
`@syntax.unmatchedDelimiter`, at the `}` and at the `[`:

```vibon
{
  format: @tree.v2
  root: {
    kind: @data
    children: [
      {
        kind: @list
        children: [
          {kind: @punctuation text: "["}
          {kind: @integer text: "1u8"}
          {kind: @error children: [{kind: @punctuation text: "}"}]}
          {kind: @missing text: ""}
        ]
      }
    ]
  }
}
```

**`@sourceGraph.v1`** holds exact source bytes and no parsed declaration.
Every graph has the `@tests` unit, even when empty.

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@sourceGraph.v1` | required |
| `units` | list of unit records sorted by `name` | every unit of the snapshot |
| `dependencies` | list of dependency records sorted by `alias` | every declared dependency |

A unit record has `name` (atom), `kind` (`@bin`, `@lib`, `@tests`, or
`@dependency`), and `modules` (list of `{path: Atom sourceId: Str
bytesHex: Str}` sorted by `path`, where `bytesHex` is the module's exact
bytes in lower-case hexadecimal). A dependency record has `alias` (`Str`),
`kind` (`@path` or `@git`), `source` (`Str`, the path or URL), `target`
(optional atom), `status` (`@captured`, `@verified`, `@unavailable`, or
`@refuted`), and `span` (the span record of its project entry).

```vibon
{
  format: @sourceGraph.v1
  units: [
    {
      name: @hello
      kind: @bin
      modules: [{path: @hello.main sourceId: "src/hello/main.vib" bytesHex: "70726f63"}]
    }
    {name: @tests kind: @tests modules: []}
  ]
  dependencies: []
}
```

**`@resolved.v2`** holds every declaration, import, and reference.

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@resolved.v2` | required |
| `package` | record `name: Str`, `version: Str` | the local package |
| `modules` | list of `{path: Atom unit: Atom sourceId: Str}` sorted by `path` | every module |
| `declarations` | list of declaration records sorted by `identity` | every declaration, members included |
| `imports` | list of import records sorted by `sourceId`, `start`, `end` of `span` | every `use` |
| `references` | list of `{span: span record to: Str}` sorted by `span` | every resolved reference, `to` an identity |

A declaration record has `identity` (`Str`); `anchor` (record `package: Str`,
`version: Str`, `module: Atom`, `owner: Str` optional, `kind: Atom`,
`name: Str`, the fields the [type chapter](02-type-system.md#declared-types)
hashes); `access` (`@public` or `@private`); and `span`. An import record
has `span`, `path` (atom), `name` (`Str`, the local name), and `to`
(optional `Str`, the identity of the module or declaration, absent when
resolution failed).

```vibon
{
  format: @resolved.v2
  package: {name: "hello" version: "0.1.0"}
  modules: [{path: @hello.main unit: @hello sourceId: "src/hello/main.vib"}]
  declarations: [
    {
      identity: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
      anchor: {
        package: "hello"
        version: "0.1.0"
        module: @hello.main
        kind: @proc
        name: "main"
      }
      access: @private
      span: {sourceId: "src/hello/main.vib" start: 0u64 end: 68u64}
    }
  ]
  imports: []
  references: []
}
```

**`@types.v2`** holds checked types and obligations.

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@types.v2` | required |
| `moduleValues` | list of `{declaration: Str typeInfo: T}` sorted by `declaration` | the type of every module value |
| `callables` | list of `{declaration: Str signature: G}` sorted by `declaration` | for every `fn`, `proc`, and member, `G` is its [signature record](06-runtime.md#open-signatures): its binders with their bounds, its receiver slot, and its complete callable type with its row, open where the declaration is |
| `implementations` | list of [implementation records](06-runtime.md#open-signatures) sorted by the canonical bytes of `contract`, then of `receiver` | every implementation, with its positional key and its choices |
| `obligations` | list of obligation records sorted by `declaration`, then `span` | every bare guard obligation |

An obligation record has `declaration` (`Str`); `kind` (`@requires`,
`@ensures`, or `@where`); `span` (the predicate's span record); `status`
(`@passed`, `@refuted`, `@unknown`, or `@unsupported`); `rules` (list of
`Str`, the identifiers of the discharge rules used, in order: `GUARD-1` to
`GUARD-7` of [Bare guards](02-type-system.md#bare-guards), and after a
`GUARD-6` the `INT` identifiers of the [integer facts](#integer-facts) it
used); `dependencies` (list
of `Str` declaration identities, sorted); and `counterexample` (optional
`Str`, the canonical VIBON text of the refuting values). A module value is
never generic, so its `typeInfo` is a closed type encoding.

The example is shape only; the identity of `ExitCode` is a placeholder:

```vibon
{
  format: @types.v2
  moduleValues: []
  callables: [
    {
      declaration: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
      signature: {
        binders: []
        typeInfo: {
          kind: @proc
          parameters: []
          labels: [:]
          result: {
            kind: @declared
            path: @std.process.ExitCode
            seed: "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad"
            arguments: []
          }
          effects: {roots: []}
        }
      }
    }
  ]
  implementations: []
  obligations: []
}
```

**`@testRun.v2`** holds one suite result.

| Field | Type | Meaning |
| --- | --- | --- |
| `format` | `@testRun.v2` | required |
| `result` | atom | the suite's `@command.*` result |
| `tests` | list of test records in discovery order | every selected test |

A test record has `name` (`Str`, the selector); `result` (a `@test.*` atom);
`failure` (optional record `kind: @assertion` or `@observation`,
`assertion: Str`, `expected: Str` optional, `actual: Str` optional,
`primarySpan`: span record, `observationError` optional record
`code: Atom`, `typeInfo: T` optional); `trap` (optional record
`trapCode: Atom`, `origin: U64` optional); and `auditTrace` (the test's
[`@auditTrace.v2`](06-runtime.md#audit-trace) document).

```vibon
{
  format: @testRun.v2
  result: @command.ok
  tests: [
    {
      name: "@tests.notes::\"counts the lines of a note\""
      result: @test.passed
      auditTrace: {
        format: @auditTrace.v2
        revision: "sha256:5fdc01744c45544c5765d4ec386b7a45bb990bc4b4c0ca415493546c7f36c8ec"
        drivers: [{provider: @hostWorld.v2 version: "0.0.2"}]
        policies: [:]
        redaction: @none
        events: []
      }
    }
  ]
}
```

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
   member and choice, unknown and repeated member and choice, overridden
   default, overlap, and orphan errors; one bound per parameter; applied
   and labelled bounds, a label bound to a parameter with its inferred
   choice and its agreement failure; receiver and destination dispatch with
   the complete substituted result, `TryFrom` with two sources and two
   error choices included; distinct identities for applied interface,
   receiver, and member; `From` and `TryFrom` overlap and the fixed
   conversion error; no call-site type arguments; the two widenings, with
   no chaining, no container covariance, a differing label rejected, and
   never from an interface value; an interface value implements no
   interface; `Self` results in the `Self` map allowed, `Self` elsewhere in
   a result rejected at the declaration, and other `Self` inputs rejected
   through an interface value; a label in an `impl` target rejected; a
   choice naming an associated type of its own `impl` rejected. No
   algebraic law is assumed of a user `Ordered` or `Equatable`.
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
    exhaustion invents no event; projections restore no authority. The
    startup order on a permuted `grants` dict in which acquiring the first
    ready leaf makes a lexically earlier leaf ready; startup failure;
    optional absence, including an absent `source` leaf; release order; and
    non-zero exit codes stay distinct from tool exits and traps.
15. **Persistent data.** Every schema of this specification decodes its
    example and round-trips it to identical canonical bytes; every hash
    vector; both float widths round-trip through `@value.v2`, including
    `-0.0`, the three non-finite values, and a NaN with a payload
    canonicalized to `@nan`; decoded-key duplicate detection across
    spellings; an unsuffixed number typed by its slot and rejected by a slot
    without a number type; the validation order, one fault reported once;
    the source-preserving formatter keeping comments and order where
    canonical serialization does not.
13. **Runtime and native.** Tail calls through direct, indirect, and
    cross-module callees, closures, interface members, and `return await`
    reuse their activation for at least 100,000 transfers. Non-tail
    recursion 100,000 deep completes within context memory, with no native
    stack frame per activation. Unbounded recursion ends in the memory host
    event. Deep release is iterative; live and peak storage are accounted at
    counts that differ by ten; module values initialize once per run. A
    pending `ensures … else`, widening, or cleanup prevents a tail transfer.
    A built executable on Windows exits with the four fixed statuses for its
    own endings and passes application codes through; invocation ordinals
    and origin ordinals agree across executors; an interface value round
    trips through `@value.v2` with its held type, and one holding a callable
    gives the observation error.
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

| `fullV2` | everything a release requires: language, library, projects, tooling, interpreter, and native targets |

`nativeV1` does not include `interpreterV2`; a differential gate needs
both. The interpreter serves `run`, `test`, `eval`, and compile-time
evaluation.

### Differential execution

An executable case is an accepted `interpret`, `workspaceRun`, or
`workspaceTest` case. It has one expected result, written from the
specification, and not one per backend.

- Each required executor runs it in its own fresh, equivalent world. The
  comparison covers canonical values, exit codes, typed errors, assertion
  outcomes, trap codes and origins, startup results, and the ordered audit
  events. A mismatch names the executor and the first difference.
- The executors are the interpreter and native code, with the execution-IR
  stepper used to isolate lowering defects.
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
unknown, and unavailable cases. A case is unknown when its observed verdict
is `unknown` and it expects another verdict. There is no "skipped" count
that reads as success, and a missing executor is reported missing. Expected
results are never regenerated from native or stepper output to make a
failure pass.

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
`@runtime.divisionByZero`, `@runtime.unobservableFunction`, and
`@syntax.retiredForm`, because the toolchain never detects a retired
surface.

Full conformance does not claim an operating-system sandbox for foreign
native code, freedom from toolchain or host failure, exploration of every
schedule, or recovery of every service from a crash.
