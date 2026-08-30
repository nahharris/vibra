# Vibra v1 diagnostics and conformance

Status: normative target
Implementation status: milestone 1 step 4 in progress (reader-v1 profile only)

## Diagnostics are a language surface

Every rejected or normalized construct has a structured diagnostic. A
diagnostic code is a stable lowercase atom of the form `@<domain>.<thing>`.
Both components are kebab-case and the thing describes the condition rather
than its issuance order or severity.

The following table is the canonical registry of codes and their fixed levels.
Where a later paragraph restates a level it agrees with this table, and the
table governs.

| Code | Level |
| --- | --- |
| `@syntax.unmatched-delimiter` | `@error` |
| `@syntax.missing-separator` | `@error` |
| `@syntax.invalid-character-literal` | `@error` |
| `@syntax.invalid-numeric-literal` | `@error` |
| `@syntax.retired-form` | `@error` |
| `@name.unknown-symbol` | `@error` |
| `@name.wrong-entity-kind` | `@error` |
| `@name.member-collision` | `@error` |
| `@name.generic-redeclaration` | `@error` |
| `@name.reserved-label` | `@error` |
| `@name.reserved-declaration` | `@error` |
| `@name.reserved-value-spelling` | `@error` |
| `@module.file-directory-collision` | `@error` |
| `@module.unknown-path` | `@error` |
| `@type.argument-mismatch` | `@error` |
| `@type.type-argument-mismatch` | `@error` |
| `@type.redundant-implementation` | `@error` |
| `@type.default-override` | `@error` |
| `@type.missing-abstract-member` | `@error` |
| `@type.function-not-equatable` | `@error` |
| `@type.not-applicable` | `@error` |
| `@type.invalid-tuple-index` | `@error` |
| `@type.unknown-record-field` | `@error` |
| `@type.numeric-out-of-range` | `@error` |
| `@type.anonymous-type-body` | `@error` |
| `@type.undispatchable-contract-member` | `@error` |
| `@type.union-too-few-members` | `@error` |
| `@type.union-member-overlap` | `@error` |
| `@type.union-member-not-concrete` | `@error` |
| `@type.overlapping-implementation` | `@error` |
| `@type.ambiguous-implementation` | `@error` |
| `@type.ambiguous-destination` | `@error` |
| `@type.invalid-ascription` | `@error` |
| `@type.narrowing-non-union` | `@error` |
| `@type.not-a-union-member` | `@error` |
| `@type.redundant-conversion` | `@error` |
| `@pattern.refutable-binding` | `@error` |
| `@effect.outside-ceiling` | `@error` |
| `@effect.invalid-reference` | `@error` |
| `@external.unknown-symbol` | `@error` |
| `@data.invalid-extension` | `@error` |
| `@project.stale-lock` | `@error` |
| `@project.entry-outside-target` | `@error` |
| `@project.entry-on-library` | `@error` |
| `@project.invalid-entry-signature` | `@error` |
| `@project.ambiguous-dependency-target` | `@error` |
| `@project.overlapping-target-roots` | `@error` |
| `@runtime.invalid-host-value` | `@error` |
| `@style.argument-order` | `@warning` |
| `@contract.unused-effect` | `@warning` |

Codes are stable within the v1 line and are atoms in Vibra data. JSON output
serializes the exact atom spelling as a string. A code need not be renamed if
its registered level changes in a future language version.

The compiler owns a closed diagnostic registry that maps each code to one
fixed level, `@error` or `@warning`, plus its domain, summary, and fix
capability. The level is not encoded in the code and cannot be configured by a
project. `vibra query @type.argument-mismatch --include diagnostic` exposes the
registry entry, and every emitted diagnostic carries the same level. A command
may fail on warnings by policy without changing their registered level.

A code's domain is the first component of its spelling. Its fix capability is
one of exactly two atoms: `@safe`, meaning the compiler can offer a fix that
`vibra edit fix` may apply, and `@none`, meaning it never offers one. V1 has
no capability for a fix that requires human review, because it has no command
that would apply one. A code's summary is compiler data and is not fixed by
this chapter.

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
unknown labels on resolved forms, missing required trivia between sibling
forms, unmatched delimiters, invalid tokens, odd map key/value tails, and
ambiguous applications remain errors.

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
| `V1-TYPE-INTERFACE` | interfaces, default methods, nested implementations, and `iter` |
| `V1-TYPE-CONVERT` | ascription, widening, narrowing, and conversion |
| `V1-TYPE-CONTROL` | control flow, failure, recursion, tail calls, and checked operations |
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

Reader recovery cases also cover a missing required separator between sibling
forms with `@syntax.missing-separator`, whose fixed level is `@error`.

Registry coverage proves that the queryable registry and the canonical table
above agree. Every code in the table has exactly one registry entry, every
registry entry appears in the table, each entry's level equals the table's,
each entry's domain equals the first component of its code, and each entry's
fix capability is `@safe` or `@none`. These cases belong to `V1-DIAG`.

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
`let`, positional parameters, lambdas, and `match`, duplicate-name and
no-shadowing rejection, all three repeating discards, and irrefutability
checking against the expected type. The removed `(bind name)` spelling MUST be
rejected with no compatibility bridge. Retired loop and return forms MUST be
rejected with `@syntax.retired-form`. `@type.not-applicable`,
`@type.invalid-tuple-index`, `@type.unknown-record-field`, and
`@pattern.refutable-binding` all have fixed level `@error`.

Interface coverage includes abstract and default contract members, rejection of
`@type.default-override` and `@type.missing-abstract-member`, the canonical
`iter` contract and default-method semantics, closed registry `iter`
conformance for `(array t)`, `(map k v)`, `str`, and `(option t)`, explicit
`impl (iter item)` on `mapped-iter`, `filtered-iter`, `skipped-iter`, and
`taken-iter`, pure `iter` default methods with `effects: ()` callbacks only,
and effectful walks written as tail-recursive module-level functions over
`iter.next`.
Tail-call cases belong to `V1-RUNTIME`. `@name.reserved-value-spelling` and
`@type.function-not-equatable` have fixed level `@error`.

Union coverage includes a two-member declaration, rejection of a one-member
body, rejection of a union, interface, and bare generic member, and rejection of
`(union (array t) (array i32))` as overlapping under instantiation. It proves
that no member method or implementation is lifted to the union, and that a union
used as a `(map k v)` key without explicit `hashable`, `equatable`, and
`ordered` implementations is rejected.

Declaration-body coverage rejects each of `record`, `enum`, `union`, and
`newtype` written in a parameter, a result, a record field, a `def` annotation,
an `as` type, an `impl` target, and a `types:` argument, with
`@type.anonymous-type-body` naming the form it found. The `types:` case is
written separately for each form so the call-site type-argument parsing and
binding path is covered rather than assumed. Positive cases confirm that
`tuple`, `array`, `map`, and `fn` remain admissible in all seven positions.

The applied-type head is covered separately, because removing the four forms
from `type-expr` does not by itself stop them returning as applications. Each of
`(record ...)`, `(enum ...)`, `(union ...)`, and `(newtype ...)` outside a
`deftype` body is rejected as `@type.anonymous-type-body` and MUST NOT be
reported as an unknown type application, and an ordinary applied type such as
`(option t)` is accepted in the same position.

The reserved-head reservation is covered on both sides. A `deftype`, a `defint`,
and a `where:` generic name spelled with a reserved type head are each rejected
with `@name.reserved-declaration`. A nested method named `map` on an owner other
than the associative `map` type is accepted, together with the `iter` contract's
own default `map` member, proving the reservation does not reach members; the
existing `@name.reserved-value-spelling` cases for module-level `map`, `array`,
and `tuple` values are unaffected.

Unification coverage fixes the bound-agnostic reading: a union whose members are
`(array t)` and `(array i32)` where `t` is bound by an interface `i32` does not
implement is still rejected with `@type.union-member-overlap`, proving that a
declared bound does not make two members disjoint.

Widening coverage proves that each written expected type in the type chapter's
list admits all three relations — member-to-union, concrete-to-interface, and
atom-singleton-to-`atom` — that no widening occurs where no expected type is
written, that `if` branches typed `i32` and `f32` are rejected rather than
unified into a union, and that `(array i32)` does not widen to `(array number)`
under invariance. Atom widening is covered on both sides of its boundary rule:
`(array.of @ok @err)` is rejected for having no single element type, and
`(as (array atom) (array.of @ok @err))` is accepted.

The no-chaining rule has its own focused rejections, so widening cannot silently
become transitive: a member value written where an interface implemented by its
union but not by the member is expected, and an atom singleton written where
`any` is expected, are both rejected even though each step would be legal alone.
Reaching the far type requires two written boundaries.

Ascription coverage includes the legal no-op, the empty-collection and
generic-result constraints, and the two rejected forms written in the type
chapter's example block: `(as i64 3i32)` for implicit numeric widening and
`(as i32 some-number)` for narrowing, both `@type.invalid-ascription`. It proves
that `as` reaches the typed IR as its operand alone and adds no runtime check.

Narrowing coverage includes an exhaustive union `match`, a binder-covered
remainder, a rejected non-exhaustive arm set, an unreachable duplicate arm, and
an `as` pattern in `let` and in a positional parameter rejected with
`@pattern.refutable-binding`. It also includes one `as` pattern over a
non-union scrutinee rejected with `@type.narrowing-non-union` and one naming a
type outside the union's member set rejected with
`@type.not-a-union-member`.

Conversion coverage includes a `from` implementation on a destination `deftype`,
a `try-from` implementation returning `conversion-error`, destination selection
through a written parameter and result type, destination selection through `as`,
a bare call with no expected type rejected with `@type.ambiguous-destination`,
one receiver implementing `(from i16)` and `(from i8)` together, and a
`from`/`try-from` pair on one source rejected with
`@type.redundant-conversion`. The two-target receiver is an identity case as
well as a dispatch case: query and index output for it MUST contain two distinct
block identities and two distinct `convert` member identities, differing only in
applied target, and completeness MUST be checked once per block rather than once
per nominal interface. That two-target receiver is also called with an
unsuffixed integer literal, whose source type both implementations satisfy, and
the call is rejected with `@type.ambiguous-implementation`. `(from t)` and
`(from i32)` on one generic receiver are rejected at the declaration with
`@type.overlapping-implementation`, and `(from t)` with `(try-from i32)` on one
generic receiver is rejected with `@type.redundant-conversion` alongside the
same-written-source pair. Two contract members are rejected with
`@type.undispatchable-contract-member`: one never naming `self`, and one naming
it only in a variadic tail that may arrive empty. A third member, naming `self`
in both its result and a variadic tail, is accepted and shown to be
destination-dispatched rather than rejected as a mixed shape.

Destination dispatch is covered beyond conversion by a non-conversion contract
member of the same shape, such as a `(defn empty () self)` factory, selected
from a written expected type and rejected with `@type.ambiguous-destination`
where none is written.

`@type.anonymous-type-body`, `@type.undispatchable-contract-member`,
`@type.union-too-few-members`,
`@type.union-member-overlap`, `@type.union-member-not-concrete`,
`@type.overlapping-implementation`, `@type.ambiguous-implementation`,
`@type.ambiguous-destination`, `@type.invalid-ascription`,
`@type.narrowing-non-union`, `@type.not-a-union-member`, and
`@type.redundant-conversion` all have fixed level `@error`. Union declaration
cases belong to `V1-TYPE-NOMINAL`; ascription, widening, narrowing, and
conversion cases belong to `V1-TYPE-CONVERT`.

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

The reader-v1 slice runs its checked-in corpus through the real syntax and
formatter handler using the internal `vibra-conformance` entrypoint. CI invokes
that entrypoint as a separate job from the host-language test suite; it exits
nonzero for a failed or unavailable case. No user-facing `vibra` binary is
introduced by milestone 1. Host-language integration tests use synthetic
temporary corpora for the runner and handler; checked-in cases are loaded only
by this dedicated entrypoint in CI (or when invoked directly).

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
