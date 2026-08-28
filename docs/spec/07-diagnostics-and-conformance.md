# Vibra v1 diagnostics and conformance

Status: normative target
Implementation status: not started

## Diagnostics are a language surface

Every rejected or normalized construct has a structured diagnostic. A
diagnostic code is a stable lowercase atom of the form `@<domain>.<thing>`.
Both components are kebab-case and the thing describes the condition rather
than its issuance order or severity:

```text
@syntax.unmatched-delimiter
@syntax.invalid-character-literal
@syntax.invalid-numeric-literal
@name.unknown-symbol
@type.argument-mismatch
@type.not-applicable
@type.invalid-tuple-index
@type.unknown-record-field
@type.numeric-out-of-range
@pattern.refutable-binding
@effect.outside-ceiling
@effect.invalid-reference
@external.unknown-symbol
@data.invalid-extension
@project.stale-lock
@runtime.invalid-host-value
@style.argument-order
@contract.unused-effect
```

Codes are stable within the v1 line and are atoms in Vibra data. JSON output
serializes the exact atom spelling as a string. A code need not be renamed if
its registered level changes in a future language version.

The compiler owns a closed diagnostic registry that maps each code to one
fixed level, `@error` or `@warning`, plus its domain, summary, and fix
capability. The level is not encoded in the code and cannot be configured by a
project. `vibra query @type.argument-mismatch --include diagnostic` exposes the
registry entry, and every emitted diagnostic carries the same level. A command
may fail on warnings by policy without changing their registered level.

V1 has no language form or `.vibon` catalog for declaring diagnostics. The
registry is compiler data: queryable and covered by schemas and conformance
tests, but not loaded from or executed as Vibra source.

A diagnostic contains schema version, code, level, message, primary source
span, related spans, notes, and zero or more fixes. Spans are half-open UTF-8
byte ranges with one-based line and Unicode-scalar column as derived display
data. Fixes declare whether they are safe and carry expected document
revisions.

Messages help people; codes and fields help tools. Tests assert codes, levels,
spans, related identities, and fix results, not incidental English punctuation.

## Recovery

The parser retains a lossless concrete syntax tree and recovers after malformed
forms so context queries and diagnostics remain useful in incomplete files. It
MUST NOT manufacture a typed node for an ambiguous recovery.

Presentation that has one unambiguous semantic binding may parse with a style
diagnostic and a safe formatter fix. Missing operands, duplicate labels,
unknown labels on resolved forms, unmatched delimiters, invalid tokens, odd map
key/value tails, and ambiguous applications remain errors.

Later phases operate on explicitly marked valid subtrees and suppress cascades
that add no new information. A tool response identifies which facts are exact,
recovered, or unavailable.

## Conformance corpus

The implementation will maintain a backend-independent corpus organized by
specification rule, not compiler module. Each case records:

- normative rule ID;
- source/project/data inputs;
- expected acceptance or diagnostics;
- expected canonical formatting;
- expected resolved identities, types, and effects where relevant;
- interpreter result and ordered audit trace where executable;
- Wasm result and ordered audit trace where executable; and
- deterministic build hashes for artifact cases.

Cases use the following stable section IDs, followed by a descriptive
kebab-case suffix such as `V1-SRC-CALLS-labelled-after-variadic`:

| Prefix | Normative section |
| --- | --- |
| `V1-CHARTER` | v1 mission, boundary, commitments, and exclusions |
| `V1-SRC-READER` | reader, tokens, names, and trivia |
| `V1-SRC-CALLS` | general application, labels, variadics, and binding |
| `V1-SRC-DECL` | declarations, nested methods and impls, and visibility |
| `V1-SRC-EXPR` | expressions, patterns, collections, and control flow |
| `V1-SRC-FMT` | canonical formatting |
| `V1-TYPE-NOMINAL` | primitive and nominal type constructors |
| `V1-TYPE-NAMES` | namespaces, imports, and forbidden shadowing |
| `V1-TYPE-INFER` | inference and public checking |
| `V1-TYPE-GENERIC` | flat generic bounds |
| `V1-TYPE-INTERFACE` | interfaces and nested implementations |
| `V1-TYPE-CONTROL` | control flow, failure, and checked operations |
| `V1-EFFECT` | declarations, rows, target ceilings, and reports |
| `V1-PROJECT` | data forms, targets, modules, dependencies, and tests |
| `V1-TOOL` | CLI, workspace queries, edit plans, schemas, and MCP |
| `V1-RUNTIME` | evaluation, host ABI, failures, and determinism |
| `V1-DIAG` | diagnostics, recovery, profiles, and release evidence |

IDs are never reassigned to a different rule. A removed rule leaves a retired
ID so test reports and external tooling remain interpretable.

Every accepted source example in `docs/spec/` becomes a corpus case or is
marked illustrative. Every error rule has at least one focused negative case.
The corpus includes Unicode spans, parser recovery, stale edit plans, forbidden
shadowing, incomplete interface methods, static target-effect mismatch,
transitively closed performed rows through effect-operation additive rows,
host-error propagation, interpreter/Wasm parity, all three equivalent discard
spellings, all character spellings, exact numeric suffix typing and range
boundaries, rejection of the retired `unit` literal, and rejection of malformed
names such as `_.x`, `a?.b`, `-.x`, and `@-.x`. Invalid character tokens use
`@syntax.invalid-character-literal`; malformed or unknown numeric suffixes use
`@syntax.invalid-numeric-literal`; and a syntactically valid literal outside
its suffixed type's range uses `@type.numeric-out-of-range`. All three have the
fixed level `@error`.

Source/data separation coverage accepts `.vib` only as source and `.vibon`
only as VIBON, rejects `project.vib`, `project-lock.vib`, and
`<target>.build.vib` with no compatibility fallback, and canonically round
trips `project.vibon`, `project-lock.vibon`, and `<target>.build.vibon`.
Reference-role coverage proves that equal atom spellings remain values in
ordinary expressions, resolve only in explicit import or VIBON schema slots,
and never appear in source effect rows. Effect rows resolve lexical symbols
through imports to the same canonical identities used by target metadata.
`@effect.invalid-reference` and `@data.invalid-extension` have fixed level
`@error`.

Effect-propagation coverage includes an effect operation with a non-empty
additive row implemented in Vibra, and proves that the additive root appears
in the performed row of a transitive caller, that a caller naming only the
owner root is rejected with `@effect.outside-ceiling`, and that the root
appears in the binary target array even though no source row of the entry
module writes it. `@effect.outside-ceiling` MUST report, for every root missing
from the offending ceiling, the call witness path that introduced it whenever
that root is not written at the reported position.

Application coverage includes arbitrary callee expressions, every closed
applicable category, constructor applications, rejection of atom and numeric
callees, tuple literal-index bounds, record selector resolution, optional
collection lookup, and proof that pure projections and lookups add neither an
effect nor a function-call edge. Collection construction covers heterogeneous
`tuple.of`, homogeneous and expected-empty `array.of`, even and duplicate-key
`map.of`, and rejection of source `(tuple ...)`, `(array ...)`, and `(map ...)`
value construction.

Pattern coverage includes direct bare-name binders, nested destructuring in
`let`, `for`, positional parameters, lambdas, and `match`, duplicate-name and
no-shadowing rejection, all three repeating discards, and irrefutability
checking against the expected type. The removed `(bind name)` spelling MUST be
rejected with no compatibility bridge. `@type.not-applicable`,
`@type.invalid-tuple-index`, `@type.unknown-record-field`, and
`@pattern.refutable-binding` all have fixed level `@error`.

## Conformance profiles

An implementation may report a development profile, but only `full-v1` may be
called Vibra v1:

| Profile | Required surfaces |
| --- | --- |
| `reader-v1` | Reader, recovery CST, formatter, syntax diagnostics |
| `static-v1` | Reader plus names, types, effects, project checking |
| `interpreter-v1` | Static profile plus reference execution and external registries |
| `tooling-v1` | Static profile plus schemas, queries, plans, CLI, MCP |
| `wasm-v1` | Static profile plus deterministic Wasm and runtime |
| `full-v1` | All profiles, stdlib, projects, and release gates |

Profiles are capability statements, not source dialects. The same source is
never reinterpreted differently by a smaller profile; unsupported execution is
reported as unavailable.

## Required implementation suites

The future implementation has two independent suites:

1. host-language tests for parser data structures, algorithms, filesystem
   safety, runtime adapters, and failure paths; and
2. Vibra conformance tests for observable source, project, tooling,
   interpreter, and Wasm behavior.

Both run from a clean checkout without network access after dependencies are
synced. Formatter idempotence, JSON-schema validation, `.vibon` data-decoder
validation, documentation examples, interpreter/Wasm differential tests, and
deterministic rebuilds are mandatory CI jobs.

Fuzzing covers the reader, formatter round trips, project/lock/build data
decoders, CLI/MCP schema decoders, external-registry boundary, and typed IR
deserialization. A fuzz finding is not closed until reduced into a regression
case.

## V1 release gate

The `full-v1` claim requires:

- every roadmap milestone complete with linked evidence;
- no unresolved contradiction or placeholder marker in normative documents;
- all normative examples classified and tested;
- all CLI and MCP JSON schemas versioned and validated by producer/consumer
  tests;
- all persistent `.vibon` data formats validated and canonically round-tripped;
- both suites green on every supported platform;
- reference interpreter and Wasm parity across the executable corpus;
- byte-identical rebuilds across two clean environments;
- formatter idempotence across stdlib, examples, and conformance source;
- every diagnostic code is unique and its queryable registered level matches
  emitted results;
- every compiler-generated host operation owned by the declared nominal effect
  in the closed registry; and
- a release audit that states supported claims and exclusions without calling
  archived behavior compatible or effectful programs host-safe.

Passing tests is necessary but not sufficient: review-only invariants such as
the absence of ambient host operations from effect-free accepted source must
have an explicit audit checklist.
