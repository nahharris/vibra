# Vibra CLI workflows

## Canonical references

- CLI guide: [`../../../../README.md`](../../../../README.md)
- Manifest, target, import, and dependency contract:
  [`../../../../docs/project-layout.md`](../../../../docs/project-layout.md)
- Project manifest schema:
  [`../../../../schemas/project-manifest.schema.json`](../../../../schemas/project-manifest.schema.json)
- Package format: [`../../../../docs/vapp-format.md`](../../../../docs/vapp-format.md)
- Package schema:
  [`../../../../schemas/package-manifest.schema.json`](../../../../schemas/package-manifest.schema.json)
- MCP protocol and tool contract:
  [`../../../../docs/mcp.md`](../../../../docs/mcp.md)
- Test runner contract:
  [`../../../../tests/README.md`](../../../../tests/README.md)

## Scaffold and validate

Choose the target shape at creation time:

```sh
vibra init hello
vibra init math --template lib
vibra init workspace --template workspace
```

The default creates a binary project. `lib` creates a library target.
`workspace` creates a library plus binary. Validate metadata, targets,
dependencies, locks, and imports without building:

```sh
vibra check .
```

## Format, lint, test, and build

```sh
vibra fmt
vibra fmt src tests --write
vibra lint . --deny-warnings
vibra test
vibra build . --bin hello --output hello.vapp
vibra package verify hello.vapp
```

`vibra fmt` is check-only unless `--write` is present. A bare `vibra test`
selects the `core` profile. Use `--filter`, repeatable `--profile` and `--tag`,
`--jobs`, `--timeout-ms`, `--fail-fast`, and `--deny-skips` to narrow or
strengthen runs. Selection flags never grant capabilities.

## Add dependencies

Add a local library during development:

```yaml
dependencies:
  local-utils:
    path: ../local-utils
```

Add and vendor a published dependency by immutable revision:

```yaml
dependencies:
  math:
    git: https://github.com/example/vibra-math.git
    rev: 0123456789abcdef0123456789abcdef01234567
```

Then run:

```sh
vibra sync .
vibra check .
```

Commit `project.lock.vibra` and the generated `dep/<name>` source graph.
Git revisions must be full 40-hex commit IDs. Do not edit `dep/` directly.
Published dependencies cannot contain path dependencies.

Import the dependency through its manifest alias:

```yaml
math:
  $import: "@math/lib.vibra"
```

## End-to-end binary example

```sh
vibra init hello-agent
cd hello-agent
vibra check .
vibra fmt
vibra lint . --deny-warnings
vibra test
vibra build . --bin hello-agent --output hello-agent.vapp
vibra package verify hello-agent.vapp
vibra run hello-agent.vapp
```

For a newly scaffolded project with no tests, `vibra test` succeeds with an
empty discovery result. Add behavior tests under `tests/` as soon as the
project exposes logic worth testing.

## Docs, LSP, MCP, and agents

Read `=doc` annotations without executing code:

```sh
vibra docs .
vibra docs . symbol-name --format json
```

Run `vibra lsp` over stdin/stdout and configure the editor as a standard LSP
client. Run `vibra mcp` over stdin/stdout for MCP clients; it exposes typed
project inspection, diagnostics, docs, effects, tests, builds, and
transactional structural-code tools. See the MCP contract above for
initialization and mutation opt-in. Structured JSON CLI responses are
preferable to scraping human text.
