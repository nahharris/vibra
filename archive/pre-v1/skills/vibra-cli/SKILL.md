---
name: vibra-cli
description: Operate Vibra projects through the CLI. Use to scaffold projects, add or vendor dependencies, format, lint, test, inspect docs, run LSP or MCP integrations, validate manifests, build deterministic `.vapp` packages, or automate Vibra CI workflows.
---

# Vibra CLI

Use the installed `vibra` binary. In a compiler checkout without an installed
binary, replace `vibra` with `cargo run --`.

## Workflow

1. Run `vibra --help` and the relevant subcommand `--help` before relying on
   optional flags; the toolchain is evolving.
2. Read [references/cli-workflows.md](references/cli-workflows.md) for command
   selection, dependency operations, and the end-to-end workflow.
3. Prefer JSON output for agent workflows.
4. Run format checks, lint, tests, and build in that order.
5. Grant host capabilities only when the program or test declares and needs
   them; keep allowed paths, commands, hosts, and environment names narrow.
6. Report the exact commands and results.
7. Read `docs/index.md` before changing language or repository behavior; update
   the canonical document in the same change and store implementation plans in
   `docs/plans/`.

## Command map

- Scaffold: `vibra init`
- Resolve exact-revision dependencies: `vibra sync`
- Validate project structure: `vibra check`
- Format: `vibra fmt`
- Diagnose source: `vibra lint`
- Run tests: `vibra test`
- Browse source documentation: `vibra docs`
- Serve editor protocol: `vibra lsp`
- Serve agent protocol: `vibra mcp`
- Build deterministic package: `vibra build`
- Inspect or verify a package: `vibra package`
- Run source or a package: `vibra run`

Run `vibra mcp` over stdin/stdout for MCP clients. Prefer its typed tools over
shell-text scraping; use `vibra code` for transactional structural edits and
`vibra lsp` for editor navigation and diagnostics.
