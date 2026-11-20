# `vibra` CLI machine contract (pre-alpha)

The normative text is `docs/spec/05-tooling.md` in the Vibra repository, and
the schemas are `crates/vibra-schema/schemas/v2/`. This page summarizes what
an agent needs in order to read results.

## JSON envelope

`vibra --format json <command>` writes exactly one JSON document to stdout,
for every command and every outcome. Diagnostics are also written to stderr
in human form.

```json
{
  "schemaVersion": 2,
  "toolVersion": "0.0.1",
  "command": "check",
  "workspaceRevision": "sha256:…",
  "status": "passed",
  "result": "@command.ok",
  "diagnostics": [],
  "limits": {"profile": "m1Default", "reached": [], "complete": true},
  "payload": {"accepted": true, "obligations": []}
}
```

- `result` is one of `@command.ok`, `@command.diagnostics`,
  `@command.testFailed`, `@command.invalidInput`,
  `@command.operationalFailure`, `@command.unavailable`, `@command.trap`, or
  `@command.programFailed`. It fixes the exit code.
- `status` is `passed`, `refuted`, `unknown`, or `unsupported`. The result
  fixes it: `passed` for `@command.ok`, `refuted` for diagnostics and failed
  tests, `unsupported` for `@command.unavailable`, and `unknown` for the rest.
  A reached limit makes it `unknown`.
- `workspaceRevision` identifies the snapshot the command read. It is `null`
  for `help`, for invalid input, and when no project was found.
- Invalid input (a bad flag, an unknown target, an unknown or malformed test
  selector) has no diagnostics, a `null` revision, and the payload `{}`.
- `limits.reached` lists the limits a command hit, such as `"memory"`, and
  `complete` is then `false`.

## Diagnostic document

```json
{
  "schemaVersion": 2,
  "code": "@type.mismatch",
  "level": "@error",
  "message": "expected `Str`, found `I32`",
  "primarySpan": {
    "sourceId": "src/hello/greeting.vib",
    "start": 230, "end": 235,
    "startPosition": {"line": 14, "column": 9},
    "endPosition": {"line": 14, "column": 14}
  },
  "related": [],
  "notes": [],
  "fixes": []
}
```

`level` is `@error` or `@warning`; only errors block `check` and `test`.
`start` and `end` are byte offsets; lines and columns are 1-based.
`sourceId` is `project.vibon` for a problem in the project file. `related`
spans usually explain *why*. Codes are camelCase atoms in the closed
registry of `docs/spec/07-diagnostics-and-conformance.md`.

## Payloads

| Command | `payload` |
| --- | --- |
| `help` | `{"usage": string, "capabilities": [{"command", "supported", "operations", "profiles", "schemaIds"}]}` |
| `fmt` | `{"path": string, "changed": bool, "written": bool, "text": string \| null}`. A preview carries the canonical `text`; a successful `--write` has `text: null` |
| `check` | `{"accepted": bool, "obligations": [ObligationRecord]}` |
| `test` | `{"selected": int, "passed": int, "failed": int, "tests": [TestItem]}` |

`TestItem`:

```json
{
  "name": "@tests.greeting::\"greets by name\"",
  "result": "@test.assertionFailed",
  "status": "refuted",
  "failure": {
    "kind": "assertion",
    "assertion": "assert.equal",
    "expected": "\"Hi, Ada!\"",
    "actual": "\"Hello, Ada!\"",
    "primarySpan": {"sourceId": "tests/greeting.vib", "…": "…"},
    "observationError": null
  },
  "trap": null,
  "auditTrace": [],
  "diagnostics": []
}
```

The test results are `@test.passed`, `@test.assertionFailed`,
`@test.invalid` (a static error in the selection, so nothing ran),
`@test.unavailable` (the test uses a form this build cannot run),
`@test.trap`, and `@test.exhausted` (the test ran out of memory, which makes
the command `@command.operationalFailure`). An item's `status` is that of the
corresponding command result, and its `diagnostics` lists the codes of the
envelope's diagnostics that concern the item.

## Selectors and targets

- `TARGET` is an atom equal to a target `name`, for example `@hello`. It is
  never a path.
- `TEST` is `@tests.<module.path>::"<exact name>"`: the test module's atom,
  `::`, and the test's name as one canonical string literal (escape `"` and
  `\`), with no other whitespace.
- An unknown target or test is `@command.invalidInput` (exit 2).
