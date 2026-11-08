# Milestone 4 — Editor integrations

Ship maintained Vibra integrations for Vim, Neovim, Zed, VS Code, and Cursor.
A developer can inspect and edit code, send forms to a persistent REPL, and
run a selected test from the editor.

## Delivery order

1. Inventory the current and recoverable editor branches. Record each candidate
   revision and its compatibility with the active specification. Start from the
   active workspace service and the current `.vib`, `.vibon`, and
   `project.vibon` contracts. Treat work predating the soft reboot as archaeology.
2. Add one local LSP adapter over M2. Include diagnostics, completion, hover
   with type and effect facts, go to definition, references, formatting, and
   code actions with revision-checked edits. Close the contract for unsaved
   buffers, position encoding, stale responses, and publishing edits.
3. Connect editor actions to M3. Send the selection or enclosing
   top-level form to a named project session, show its value and output, rerun
   a selected test against the session revision. Carry revision and source
   identity into evaluation results and test diagnostics.
4. Ship syntax and filetype configuration, toolchain discovery, and tested
   setup and session instructions for each client. Share configuration and
   extension code where clients support it.

| Clients | Integration path to qualify |
| --- | --- |
| Vim and Neovim | Maintain filetype and form-evaluation commands. Qualify a supported LSP setup for Vim and exercise Neovim's LSP client. |
| Zed | Package language registration and connect the shared LSP and evaluation services. |
| VS Code and Cursor | Qualify one shared extension or package for both clients. Register language tooling and session commands. [VS Code language server guide](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide) |

## Gates

- **Demo.** On a clean install in each supported editor, open the same project,
  follow a definition, preview and apply a lint action, evaluate a changed form
  in a persistent session, rerun its test, and follow a failure to its
  source location. Include an unsaved edit and a stale code action in the fixture.
- **Exit.** Record tested client versions, platform coverage, and install
  steps. Exercise Windows without WSL, including editor-to-tool launch and a
  project path with spaces. Run protocol conformance for LSP and session
  operations independently of the editor smoke runs.
