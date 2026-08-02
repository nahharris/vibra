# Vibra MCP server

`vibra mcp` exposes a closed, structured subset of the Vibra CLI to agents and
IDE integrations over the Model Context Protocol (MCP) stdio transport:

```sh
vibra mcp --workspace /path/to/project
```

The default server is read-only and does not execute project tests. Grant only
the narrow modes a client needs:

```sh
vibra mcp --workspace /path/to/project --allow-test
vibra mcp --workspace /path/to/project --allow-write
```

`--allow-write` permits `vibra.fmt` with `write: true` and `vibra.build`.
`--allow-test` permits `vibra.test`. These flags do not grant Vibra host
capabilities: MCP tests run without filesystem, environment, network, process,
clock, randomness, or system-information approvals. Tests that require an
isolated writable workspace remain skipped because MCP does not expose
`--allow-test-workspace`.

## Protocol

The server accepts one UTF-8 JSON-RPC 2.0 message per line on stdin and writes
one response per line on stdout. It negotiates MCP protocol version
`2025-06-18` and implements:

- `initialize`
- `notifications/initialized`
- `ping`
- `tools/list`
- `tools/call`

Requests are processed serially, and `tools/list` has a fixed order. Tool input
and output schemas are returned by `tools/list`. The shared call-result envelope
is also published as
[`schemas/mcp-tool-result.schema.json`](../../schemas/mcp-tool-result.schema.json).

## Tools

| Tool | CLI behavior | Default authority |
| --- | --- | --- |
| `vibra.project.inspect` | Reads manifest, targets, and dependencies | read-only |
| `vibra.tests.list` | Discovers tests without executing them | read-only |
| `vibra.check` | `vibra check --format json` | read-only |
| `vibra.docs` | `vibra docs --format json` | read-only |
| `vibra.fmt` | `vibra fmt --format json` | check-only |
| `vibra.lint` | `vibra lint --format json` | read-only |
| `vibra.test` | `vibra test --jobs 1 --format json` | requires `--allow-test` |
| `vibra.build` | `vibra build --format json` | requires `--allow-write` |

CLI-backed successful results place the CLI JSON value under
`structuredContent.output`. Introspection tools return their records directly.
The same structured value is rendered as JSON in the MCP text content for
clients that do not consume `structuredContent`.

## Filesystem and process safety

Every client path is resolved relative to `--workspace`. Existing paths are
canonicalized before use, preventing both `..` and symlink escapes. Build
outputs require an existing canonical parent inside the workspace. Formatter
globs are intentionally not accepted because they would make path confinement
ambiguous. Recursive CLI tools reject symlinks anywhere in the workspace tree,
which avoids both escape paths and directory cycles.

The server starts only its own resolved `vibra` executable with a fixed command
and validated arguments. Clients cannot provide an executable, arbitrary CLI
subcommand, environment entry, shell fragment, or report path.

## Error model

Malformed JSON-RPC and unknown protocol methods use standard JSON-RPC errors
(`-32700`, `-32600`, and `-32601`). A valid `tools/call` always returns an MCP
tool result. Failures set `isError: true` and use this stable shape:

```json
{
  "code": "permission-denied",
  "message": "vibra.test requires starting `vibra mcp` with --allow-test"
}
```

Stable tool error codes are enumerated in the result schema. A normal CLI
nonzero exit becomes `command-failed`; its `data` includes the exit code and
any parsed structured CLI output. This distinguishes lint, formatting, build,
and test findings from protocol failures.
