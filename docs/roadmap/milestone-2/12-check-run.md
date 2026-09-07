# Step 12 — checking and pure interpreter run

Requires Step 11 and C1/C3/C10/C11. Read tooling **V1 CLI**, **Schemas and
errors**, projects **Packages and targets**, runtime **Semantic reference**,
**Determinism and observability**: [tooling](../../spec/05-tooling.md),
[projects](../../spec/04-programs-and-packages.md), [runtime](../../spec/06-runtime.md).

## Implementation sequence

1. Add `check` and `run` argument/target selection adapters over the same
   workspace snapshot. Honor C3's checking scope, not only reachable entry code.
2. Validate entry using the shared atom walker, then separately its entity kind
   and signature. Only the C1-admitted entry subset executes; valid deferred
   `result void e` is not reported as malformed syntax.
3. Reject all errors and unavailable execution before starting the interpreter.
   `check` does not execute bodies or initialization, invoke providers, sync
   dependencies, or mutate project data.
4. Execute the admitted pure target through the existing checked IR/interpreter.
   Keep command result, program result and trap distinct using C10/C11. Handle
   `--format json` exactly as specified without stealing program-owned stdout.
5. Report unsupported later forms explicitly even when they occur in otherwise
   parsed M1 syntax. Preserve `@syntax.retired-form` for retired language and
   registry errors for illegal externals; availability is not a catch-all error.

| Positive | Negative / boundary |
| --- | --- |
| Init output checks/runs from nested project directory | Missing project; missing/ambiguous target per C2; library selected for run |
| Pure multi-module program; private non-main entry | Outside-target entry; unknown path; wrong entity kind; invalid signature |
| Named/lambda calls, constants, recursion and stdlib | Type error anywhere in required checking scope prevents execution |
| Stable values/results/empty program output and events | Deferred effects/dependencies, unknown providers, Wasm/WASI source FFI |
| JSON and human classification of same failure | Distinguish availability, source errors, operational failure and trap |

Pure execution's empty host trace does not forbid the compiler reading source
files or the CLI writing its own diagnostics. Test those boundaries separately.
V1 defines no runtime language budget; a process timeout is external evidence,
not a portable program result.

Run [common validation](validation.md), actual CLI process tests, and static
and interpreter corpus profiles for the supported subset. Record exact check/run
demo commands, process exit codes, structured atoms, and stream bytes. No Wasm
build, network fallback, effect execution, or advertising a supported v1 release.
