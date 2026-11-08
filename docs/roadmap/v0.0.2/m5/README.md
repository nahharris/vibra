# Milestone 5 — Editor integrations

Ship maintained Vibra integrations for Vim, Neovim, Zed, VS Code, and Cursor.
A developer can inspect and edit code, send forms to a persistent REPL, run a
selected test, and debug it from the editor.

## Delivery order

1. Inventory the current and recoverable editor branches. Record each candidate
   revision and its compatibility with the active specification. Start from the
   active workspace service and the current `.vib`, `.vibon`, and
   `project.vibon` contracts. Treat work predating the soft reboot as archaeology.
2. Add one local LSP adapter over M2. Include diagnostics, completion, hover
   with type and effect facts, go to definition, references, formatting, and
   code actions with revision-checked edits. Close the contract for unsaved
   buffers, position encoding, stale responses, and publishing edits.
3. Connect editor actions to M3 and M4. Send the selection or enclosing
   top-level form to a named project session, show its value and output, rerun
   a selected test against the session revision, and launch the shared DAP
   adapter for an entry or test. Carry revision and source identity into
   result locations and breakpoints.
4. Ship syntax and filetype configuration, toolchain discovery, and tested
   setup and launch instructions for each client. Share configuration and
   extension code where clients support it.

| Clients | Integration path to qualify |
| --- | --- |
| Vim and Neovim | Maintain the filetype and form-evaluation commands. Document a supported LSP and DAP setup for Vim and exercise Neovim's LSP client with nvim-dap. [Vimspector](https://github.com/puremourning/vimspector) · [nvim-dap](https://github.com/mfussenegger/nvim-dap) |
| Zed | Package language registration and launch configuration for the shared LSP and DAP services. [Zed debugger integration](https://zed.dev/docs/debugger) |
| VS Code and Cursor | Qualify one shared extension or package for both clients. Register language tooling, session commands, and launch configurations. [VS Code language server guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide) |

## Gates

- **Demo.** On a clean install in each supported editor, open the same project,
  follow a definition, preview and apply a lint action, evaluate a changed form
  in a persistent session, rerun its test, and stop at a breakpoint to inspect
  locals. Include an unsaved edit and a stale code action in the fixture.
- **Exit.** Record tested client versions, platform coverage, and install
  steps. Exercise Windows without WSL, including editor-to-tool launch and a
  project path with spaces. Run protocol conformance for LSP, DAP, and session
  operations independently of the editor smoke runs.
