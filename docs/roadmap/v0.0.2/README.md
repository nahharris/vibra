# Vibra v0.0.2

**Planned pre-alpha release.** An agent can inspect, change, evaluate, and
test a Vibra project through structured tools. The release also prepares the
checked inputs and runtime boundary for [v0.0.3](../v0.0.3/README.md).

| Order | Milestone | Evidence |
| --- | --- | --- |
| M1 | [Content-addressed codebase](m1/README.md) | Import a project, reopen the store, and read old and new checked definitions by hash. |
| M2 | [Queries, edits, formatting, and lint](m2/README.md) | Follow a reference to its definition, preview an explained code action, and reject a stale publish. |
| M3 | [Interactive evaluation and tests](m3/README.md) | Keep a project REPL open, evaluate forms from a file, redefine a function, and rerun selected tests in that session. |
| M4 | [Editor integrations](m4/README.md) | Install the supported integrations for Vim, Neovim, Zed, VS Code, and Cursor and run the editor acceptance fixture in each. |
| M5 | [Native foundations and release](m5/README.md) | Verify portable inputs and backend-neutral execution evidence against the v0.0.1 baseline. |

## Scope and priorities

The existing workspace snapshot, position facts, formatter, test runner,
interpreter, and deterministic test host are the starting point. The content
graph adds durable identity for checked definitions. Queries, edits, and
sessions use those revisions through one workspace service. Agents use CLI
and MCP to inspect, change, evaluate, and test code. Editors use LSP for
language tooling and the shared session service for sending forms and
running tests.

Settle definition identity and publication first. They govern the query,
edit, and session contracts. Exercise one declaration edit and
one selected test through the full loop before widening rule coverage or
adapter capabilities. The main design risks are deterministic addressing of
recursive components and safe redefinition across an interpreter session.

The minimum release fixture is an offline project with multiple modules and
targets in one package and the trusted embedded standard library. Full exact
Git/path dependency delivery, vendor sync, lock generation, and an external
stdlib trust model move to the
[dependency scratchpad](../scratchpad/foundations/dependencies.md). Capture and
verify every input admitted by this release. Keep ordinary `.vib` projects
usable with the current CLI and define explicit import/export for the store.

The release uses the interpreter for interactive execution. It carries the
existing Wasm conformance path through the handoff to v0.0.3. Wasm build
products, native code emission, and native release packaging are outside this
release. The editor milestone scopes the supported LSP capabilities.
[Live debugging](../scratchpad/agent-tooling/debugger.md) follows the Nexus
cutover and the local actor runtime. A distributed code store and stronger
types remain separate work.

## Release gate

Use one checked-in project and one repeatable agent workflow over MCP, with
equivalent machine-mode CLI evidence. Start from a failing test. Resolve a
callee and read its definition and effects by hash. Through MCP, run the test
and use its structured failure, source location, and query facts to diagnose
the issue. Stage an S-expression repair in a persistent session,
preview formatting and a lint action with its reason, evaluate the changed
definition, and rerun the selected test before publishing. Race a stale
publish against a successful one and show that only the valid revision becomes
the head. Reopen the store and retrieve both versions by hash.

Pass the editor acceptance fixture on Vim, Neovim, Zed, VS Code, and Cursor.
From an editor buffer, evaluate a form in a persistent session, rerun the
selected test, and navigate from a failure to its source location.

Run both the host suite and the independent Vibra corpus for the shipped
tooling surface and existing supported profiles. Validate machine outputs
against their schemas and include denied, stale, malformed, interrupted, and
resource-exhausted paths. Publish the supported command and profile inventory,
the M5 handoff fixtures, and the release audit before tagging. The
[full conformance gate](../../spec/07-diagnostics-and-conformance.md#full-conformance-gate)
is a separate target from this tooling release gate.

Begin with the M1 specification prerequisite. Each milestone needs an
approved contract and a step plan under the
[execution model](../execution.md) before its implementation begins.
