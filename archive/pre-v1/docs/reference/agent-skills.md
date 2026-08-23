# Agent skills

The two repository-owned skills live under [`skills/`](../../skills/):

- `vibra-coding` guides agents writing modules, tests, imports, and `=doc`
  documentation.
- `vibra-cli` guides agents through scaffolding, dependency management,
  validation, formatting, linting, testing, documentation, editor/agent
  protocols, and deterministic builds.

This location is the source of truth and the only skills distributed by this
repository. Agent hosts that discover a top-level `skills/` directory can load
these skills directly. For hosts that require a product-specific directory,
configure that host to import these folders or copy/symlink them into its
skills directory; do not maintain edited duplicate copies. Invoke the skills
explicitly as `$vibra-coding` or `$vibra-cli` when automatic discovery is not
available.

`.agents/` is a local agent-host directory and is fully gitignored. Do not add
third-party or machine-local skills there, and do not copy them into this
repository.

Cursor rules, Claude instructions, and similar repository instruction files
should point agents at these skills and at [`AGENTS.md`](../../AGENTS.md). The
skills contain no product-specific tool assumptions, so the same workflow can
be used by any agent able to read Markdown and execute the Vibra CLI.

Each skill follows the Agent Skills folder shape: a `SKILL.md` with trigger
metadata, optional `references/` loaded on demand, and `agents/openai.yaml`
metadata for compatible interfaces.
