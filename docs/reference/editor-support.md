# Editor support

`vibra lsp` is a standard input/output language server. It provides diagnostics,
completion, hover documentation, definitions, references, and whole-document
formatting. Start the editor from a directory containing `project.vib`, or
open that directory as the workspace root.

## Visual Studio Code

Until a packaged extension is published, install any extension that can launch
a generic stdio language server (for example, **LSP Client**) and add this
workspace setting:

```json
{
  "lsp-client.serverCommand": ["vibra", "lsp"],
  "lsp-client.languages": ["vibra"],
  "[vibra]": {
    "editor.formatOnSave": true
  }
}
```

Associate `*.vib` with the `vibra` language if the client does not do so
automatically:

```json
{ "files.associations": { "*.vib": "vibra" } }
```

The executable must be on VS Code's `PATH`. Alternatively, replace `vibra`
with an absolute path to the binary.

## Neovim

Neovim 0.11 and newer can start the server without an extra plugin. Put this
in `init.lua` (adjust `root_markers` if your workspace uses another marker):

```lua
vim.lsp.config.vibra = {
  cmd = { "vibra", "lsp" },
  filetypes = { "vibra" },
  root_markers = { "project.vib", ".git" },
}
vim.lsp.enable("vibra")

vim.api.nvim_create_autocmd("BufWritePre", {
  pattern = "*.vib",
  callback = function() vim.lsp.buf.format({ async = false }) end,
})
```

Add `vim.filetype.add({ extension = { vibra = "vibra" } })` if no syntax
plugin has registered the filetype.

## Multiple packages and workspace folders

Open the common directory that contains the application manifest and its local
packages. Vibra discovers every `.vib` source below that root and resolves
manifest `@package/path` imports. The checked-in
[`examples/lsp-workspace`](../../examples/lsp-workspace) fixture demonstrates this
layout. In an editor with multiple workspace folders, start one `vibra lsp`
process per folder; each server treats its initialization `rootUri` as an
independent workspace. This avoids symbols or compilation flags leaking between
projects.

Compilation flags may be supplied as
`initializationOptions.compilationFlags` and updated through
`settings.vibra.compilationFlags`.

## Performance expectations

The integration suite exercises a synthetic medium workspace of 250 source
files (roughly 2,000 lines) and requires initialization, opening a document,
and a workspace completion request to finish within 10 seconds on an
unoptimized test build. This is a deliberately conservative CI ceiling, not a
typical latency: on a developer machine, semantic requests over a workspace of
this size should normally complete in under one second.

Reproduce the contract with:

```sh
cargo test --test lsp medium_workspace_semantic_request_meets_performance_contract -- --nocapture
```

Diagnostics compile an overlay-safe temporary mirror, so their latency follows
a normal `vibra check` more closely than navigation latency. The server uses
full-document synchronization and recomputes its semantic snapshot per request;
very large monorepos may therefore benefit from opening a narrower workspace
root.
