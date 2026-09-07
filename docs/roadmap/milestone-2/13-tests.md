# Step 13 — pure tests and assertions

Requires Step 12 and C8/C9/C10/C11. Read projects **Tests**, source
**Declarations**, tooling **V1 CLI**, runtime **Evaluation** and
**Determinism and observability**: [projects](../../spec/04-programs-and-packages.md),
[source](../../spec/01-source-language.md), [tooling](../../spec/05-tooling.md),
[runtime](../../spec/06-runtime.md).

## Implementation sequence

1. Discover and identify `tests/` modules using C9's reviewed relationship to
   targets. Reuse graph/resolver/checker services; do not add a test-only parser,
   hidden prelude, private-visibility escape, or executable test in a data file.
2. Check unique module-local string names and empty effects for admitted tests.
   Reject malformed/type-invalid tests before execution according to C9's scope.
3. Complete C9's pure assertion library and structured test-failure path. It must
   use the admitted language/registry contract; no hidden host event, general
   exception mechanism, generic escape hatch, or fake nominal `result`.
4. Isolate values/initialization state and audit trace for each test. Use the same
   interpreter as `run`; order discovery, selection, execution and reporting
   deterministically. Define empty selection exactly as C9 specifies.
5. Add `test` CLI output and result/schema adapters. One failed assertion, static
   error, unavailable selected test, or trap cannot be reported as suite success.
   Keep their structured outcome classes distinct.

| Positive | Negative / boundary |
| --- | --- |
| Multiple files and identical test names in different modules | Duplicate name within a module; damaged/type-invalid test body |
| Explicit assertion imports and supported primitive assertions | Missing import; spoofed trusted assertion external; deferred operand types |
| Passing assertion and a deliberately failing assertion | Failure must not become success, generic trap, or host-effect operation |
| Repeated isolated execution with empty event lists | Values or results leaking across tests, order-dependent success |
| Selection and empty-suite behavior from C9 | Unknown filter/target, nonempty test effects, unavailable selected form |

The illustrative generic `assert.equal` example is not authority to implement
M3 generics in this step. Use the exact M2 assertion signatures frozen in C9.
Do not claim nondeterministic host providers or M4 effectful tests are supported.

Run [common validation](validation.md), test-runner host tests, actual binary
tests and independently authored `V1-PROJECT-*` / `V1-RUNTIME-*` cases. Done
includes schema validation, process exits, empty traces, and a clean initialized
multi-module project with both passing and intentionally failing test runs.
