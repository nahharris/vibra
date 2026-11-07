# `vibra` CLI machine contract (pre-alpha)

The normative text is `docs/spec/05-tooling.md` in the Vibra repository. This
page summarizes what an agent needs in order to parse results.

## JSON envelope

`vibra --format json <command>` writes exactly one JSON document to stdout.
Diagnostics are also written to stderr in human form.

```json
{
  "schemaVersion": 1,
  "command": "check",
  "result": "@command.ok",
  "diagnostics": [],
  "payload": {"accepted": true}
}
```

`result` is one of `@command.ok`, `@command.diagnostics`,
`@command.test-failed`, `@command.invalid-input`,
`@command.operational-failure`, `@command.trap`, `@command.unavailable`, or
`@command.program-failed`. Invalid input, such as a bad flag or an unknown
target or selector, has an empty `diagnostics` array and exits with code 2.

## Diagnostic document

```json
{
  "schemaVersion": 1,
  "code": "@effect.outside-ceiling",
  "level": "@error",
  "message": "this body performs @std.io.stdout, which its written ceiling does not contain",
  "primarySpan": {
    "sourceId": "src/app/main.vib",
    "start": 379, "end": 411,
    "startPosition": {"line": 10, "column": 3},
    "endPosition": {"line": 10, "column": 35}
  },
  "related": [{"span": {"sourceId": "...", "...": "..."}, "message": "performs @std.io.stdout via @std.io.stdout.text"}],
  "notes": [],
  "fixes": []
}
```

`level` is `@error` or `@warning`. Only errors block `check`, `run`, and
`test`. `start` and `end` are byte offsets. Lines and columns are 1-based.
`sourceId` is `project.vibon` for project-file problems. `related` usually
explains *why*: for an effect error, it points at the call that performs the
effect. For a `@tool.unavailable` diagnostic, the form is valid Vibra that
the pre-alpha cannot run yet. Rewrite the code to avoid it instead of hunting
for a syntax error.

## Payloads

| Command | `payload` |
| --- | --- |
| `help` | `{"usage": string}` |
| `project init` | `{"workspace": string, "created": string[]}` |
| `fmt` | `{"path": string, "changed": bool, "written": bool, "text": string \| null}`. The preview carries `text`. A successful `--write` has `text: null` |
| `check` | `{"accepted": bool}` |
| `run` | `{"target": "@app", "programResult": string \| null, "stdout": string, "stderr": string, "auditTrace": AuditEvent[], "trap": {"trapCode": string, "origin": Span \| null} \| null}` |
| `test` | `{"selected": int, "passed": int, "failed": int, "tests": [TestItem]}` |

`TestItem` has these fields:
`{"name": "@tests.mod::\"test name\"", "result": "@test.passed", "failure": {"assertion", "expected", "actual", "primarySpan"} | null, "trap": {...} | null, "auditTrace": [...], "diagnostics": [...]}`.

The test results are `@test.passed`, `@test.assertion-failed`,
`@test.invalid` (static errors blocked the suite, so nothing ran),
`@test.unavailable`, and `@test.trap`.

`AuditEvent` is `{"kind": string, "op": string, "args": string[], "result": string}`.
It records one completed host call, such as `io.stdout-text`, in call order.
For `run` and `test`, the audit trace shows exactly which effects happened.

## `run` specifics

- In human mode, the program's stdout passes straight through, and command
  messages go to stderr.
- In JSON mode, the program's stdout and stderr are captured into the payload,
  so the process stdout stays one JSON document.
- If `main` returns `err`, the result is `@command.program-failed` (exit 6).
  `programResult` holds the canonical encoding of the returned `result`, and
  human mode prints that encoding to stderr. This outcome is neither a trap nor
  a diagnostic.
- A trap is `@command.trap` (exit 5), with `trapCode`, for example
  `@runtime.invalid-checked-program`.
- `vibra run` uses the real host: the files, environment, arguments, and
  clock of the machine. `vibra test` never does.

## Selectors and targets

- `TARGET` is an atom equal to a target `name`, for example `@app`. It is
  never a path.
- `TEST` is `@tests.<module.path>::"<exact name>"`, written as a canonical
  string literal (escape `"` and `\`), with no surrounding whitespace.
- An unknown target or test is `@command.invalid-input` (exit 2).
