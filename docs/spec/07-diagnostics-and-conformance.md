# Vibra v1 diagnostics and conformance

Status: normative target
Implementation status: not started

## Diagnostics are a language surface

Every rejected or normalized construct has a structured diagnostic. Diagnostic
codes are stable within the v1 line and use:

```text
E-SYN-nnnn     reader and grammar errors
E-NAME-nnnn    namespace and resolution errors
E-TYPE-nnnn    type errors
E-EFFECT-nnnn  effect errors
E-AUTH-nnnn    project/runtime authority errors
E-RESOURCE-nnnn resource-scope errors
E-PROJECT-nnnn project and dependency errors
E-RUNTIME-nnnn runtime semantic failures
W-STYLE-nnnn   canonical presentation
W-CONTRACT-nnnn suspicious but valid declarations
```

A diagnostic contains schema version, code, severity, message, primary source
span, related spans, notes, and zero or more fixes. Spans are half-open UTF-8
byte ranges with one-based line and Unicode-scalar column as derived display
data. Fixes declare whether they are safe and carry expected document
revisions.

Messages help people; codes and fields help tools. Tests assert codes, spans,
related identities, and fix results, not incidental English punctuation.

## Recovery

The parser retains a lossless concrete syntax tree and recovers after malformed
forms so context queries and diagnostics remain useful in incomplete files. It
MUST NOT manufacture a typed node for an ambiguous recovery.

Presentation that has one unambiguous semantic binding may parse with a style
diagnostic and a safe formatter fix. Missing operands, duplicate labels,
unknown labels on resolved forms, unmatched delimiters, invalid tokens, and
ambiguous calls remain errors.

Later phases operate on explicitly marked valid subtrees and suppress cascades
that add no new information. A tool response identifies which facts are exact,
recovered, or unavailable.

## Conformance corpus

The implementation will maintain a backend-independent corpus organized by
specification rule, not compiler module. Each case records:

- normative rule ID;
- source/project inputs;
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
| `V1-SRC-CALLS` | labels, parameters, calls, and evaluation binding |
| `V1-SRC-DECL` | top-level declarations and visibility |
| `V1-SRC-EXPR` | expressions, patterns, control flow, and resources |
| `V1-SRC-FMT` | canonical formatting |
| `V1-TYPE-NOMINAL` | primitive and nominal type constructors |
| `V1-TYPE-NAMES` | namespaces, imports, and lexical binding |
| `V1-TYPE-INFER` | inference and public checking |
| `V1-TYPE-GENERIC` | generics and bounds |
| `V1-TYPE-INTERFACE` | interfaces and implementations |
| `V1-TYPE-CONTROL` | control flow, failure, and checked operations |
| `V1-TYPE-RESOURCE` | lexical host-resource typing |
| `V1-EFFECT` | effect declarations, rows, inference, and reports |
| `V1-AUTH` | runtime grants and constraint resolution |
| `V1-PROJECT` | manifests, targets, modules, dependencies, and tests |
| `V1-TOOL` | CLI, workspace queries, edit plans, schemas, and MCP |
| `V1-RUNTIME` | evaluation, ABI, resources, budgets, and determinism |
| `V1-DIAG` | diagnostics, recovery, profiles, and release evidence |

IDs are never reassigned to a different rule. A removed rule leaves a retired
ID so test reports and external tooling remain interpretable.

Every accepted source example in `docs/spec/` becomes a corpus case or is
marked illustrative. Every error rule has at least one focused negative case.
The corpus includes Unicode spans, parser recovery, stale edit plans, grant
denial, budget exhaustion, cleanup on every exit, and interpreter/Wasm parity.

## Conformance profiles

An implementation may report a development profile, but only `full-v1` may be
called Vibra v1:

| Profile | Required surfaces |
| --- | --- |
| `reader-v1` | Reader, recovery CST, formatter, syntax diagnostics |
| `static-v1` | Reader plus names, types, effects, project checking |
| `interpreter-v1` | Static profile plus reference execution and host registry |
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
2. Vibra conformance tests for observable source, project, tooling, interpreter,
   and Wasm behavior.

Both run from a clean checkout without network access after dependencies are
synced. Formatter idempotence, schema validation, documentation examples,
interpreter/Wasm differential tests, and deterministic rebuilds are mandatory
CI jobs.

Fuzzing covers the reader, formatter round trips, project parser, schema
decoders, host registry boundary, and typed IR deserialization. A fuzz finding
is not closed until reduced into a regression case.

## V1 release gate

The `full-v1` claim requires:

- every roadmap milestone complete with linked evidence;
- no unresolved contradiction or placeholder marker in normative documents;
- all normative examples classified and tested;
- all JSON schemas versioned and validated by producer/consumer tests;
- both suites green on every supported platform;
- reference interpreter and Wasm parity across the executable corpus;
- byte-identical rebuilds across two clean environments;
- formatter idempotence across stdlib, examples, and conformance source;
- no host operation outside the closed registry and no authority bypass;
- resource and budget cleanup verified on success, typed error, denial,
  exhaustion, and trap; and
- a release audit that states supported claims and exclusions without calling
  archived behavior compatible.

Passing tests is necessary but not sufficient: review-only invariants such as
the absence of ambient authority and terminal placement of hardening stages
must have an explicit audit checklist.
