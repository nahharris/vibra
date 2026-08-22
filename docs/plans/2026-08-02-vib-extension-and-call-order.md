# Vibra Source Extension and Call Argument Order Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `.vib` the only Vibra source-module extension and require calls to place fixed positional operands before labelled operands before variadic positional operands.

**Architecture:** Use `project.vib` for the project source file as well as all other Vibra source files; its `(project ...)` root is interpreted by project commands and there is no separate manifest extension. Update the typed frontend, test discovery, tooling discovery, generated fixtures, source imports, and tracked source filenames to use `.vib`. Use the existing AST call representation to identify fixed positional, labelled, and variadic arguments for validation while preserving source order; reject invalid order with `E-SYN-013` and keep formatting limited to layout.

**Tech Stack:** Rust compiler and tooling, native S-expression parser/printer, Vibra source modules, Rust unit/integration tests, Vibra-language tests, JSON linter-code registry.

---

### Task 1: Lock down the two regressions with failing tests

**Files:**
- Modify: `src/tooling.rs:625-645` for `.vib` discovery assertions.
- Modify: `src/frontend.rs` extension tests near `canonical_module_path` and import-graph tests.
- Modify: `src/sexpr_tooling.rs:37-50` and `src/sexpr_tooling.rs:681-784` for parser, formatter, and lint regression tests.

- [x] **Step 1: Change the discovery test to require `.vib` and reject `.vibra`**

```rust
assert!(is_vibra_file(Path::new("main.vib")));
assert!(!is_vibra_file(Path::new("main.vibra")));
assert!(!is_vibra_file(Path::new("main.vib.yaml")));
```

- [x] **Step 2: Add a frontend regression proving `.vibra` is rejected**

```rust
#[test]
fn rejects_legacy_vibra_source_extension() {
    let temp = tempfile::tempdir().unwrap();
    let entry = temp.path().join("main.vibra");
    write(&entry, "(defn main () void (do unit))\n");
    let error = load_surface_program(&entry, &CompilationFlags::default())
        .unwrap_err()
        .to_string();
    assert!(error.contains("`.vib` extension"), "{error}");
}
```

- [x] **Step 3: Add parser, formatter, and linter regressions for call order**

```rust
#[test]
fn rejects_labels_after_variadic_arguments() {
    let source = "(defn caller () void (do (target 1 2 3 label-1: 4 label-2: 5)))\n";
    assert!(staged_sexpr_diagnostics(Path::new("call.vib"), source)
        .iter()
        .any(|diagnostic| diagnostic.code == "E-SYN-013"));
}

#[test]
fn formatter_preserves_valid_labelled_argument_order() {
    let source = "(defn target (first int64 second int64 label-1 int64 label-2 int64 rest... int64) void (do unit))\n\
(defn caller () void (do (target 1 2 label-1: 4 label-2: 5 3)))\n";
    let formatted = staged_format_sexpr(Path::new("call.vib"), source).unwrap();
    assert!(formatted.contains("(target 1 2 label-1: 4 label-2: 5 3)"));
    assert_eq!(formatted, staged_format_sexpr(Path::new("call.vib"), &formatted).unwrap());
}

#[test]
fn linter_warns_when_labelled_argument_follows_variadic_argument() {
    let source = "(defn target (first int64 second int64 label-1 int64 rest... int64) void (do unit))\n\
(defn caller () void (do (target 1 2 3 label-1: 4)))\n";
    let diagnostics = staged_lint_sexpr(Path::new("call.vib"), source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "E-SYN-013"
            && diagnostic.message.contains("labelled arguments")
    }));
}
```

- [x] **Step 4: Run the focused tests and confirm they fail for the missing behavior**

Run: `cargo test tooling::discovery_tests::only_discovers_sexpression_vibra_sources --lib` and `cargo test sexpr_tooling::tests::formatter_preserves_valid_labelled_argument_order --lib`.

Expected: FAIL because the current discovery accepts `.vibra`, the formatter preserves the original call order, and the new lint code does not yet exist.

### Task 2: Implement `.vib` source-only loading and rename tracked source modules

**Files:**
- Modify: `src/frontend.rs`, `src/load.rs`, `src/test_runner.rs`, `src/tooling.rs`, `src/mcp.rs`, and all Rust fixture strings that refer to source modules.
- Rename: every tracked source/test/example file ending in `.vibra`, including
  project files, to `.vib`; leave `project.lock.vibra` as the legacy lock name.
- Modify: source imports and manifest `entry:` values in `examples/`, `tests/`, and the `stdlib` submodule.

- [x] **Step 1: Update the extension constants and path checks**

Use `.vib` in `frontend::module_self_alias`, `frontend::module_part_paths`, `frontend::canonical_module_path`, `test_runner::is_vibra_file`, `test_runner::is_conditional_module_part`, `tooling::is_vibra_file`, and the MCP source-extension filter. Update the inline synthetic entry in `load.rs` to `__vibra_exec__.vib`; project commands consume the `(project ...)` root from `project.vib`.

- [x] **Step 2: Rename source files and update all source/import/entry references**

Run the equivalent of `git mv path/file.vibra path/file.vib` for every source, test, and example module. Update relative imports, `@namespace/...` imports, conditional filenames, generated manifest entries, fixture paths, and source-extension documentation so no runnable source path still names `.vibra`.

- [x] **Step 3: Re-run the focused extension tests**

Run: `cargo test tooling::discovery_tests --lib` and `cargo test frontend::tests --lib`.

Expected: PASS with `.vib` accepted and `.vibra` rejected.

### Task 3: Canonicalize call order in formatting and linting

**Files:**
- Modify: `src/sexpr_tooling.rs` for local call-signature collection, AST-aware validation, and `E-SYN-013` diagnostics.
- Modify: `src/syntax/printer.rs` only if the formatter normalization needs a printer helper for stable node output.
- Modify: `src/tooling.rs:306-376` to describe `E-SYN-013` in structured lint output.
- Modify: `schemas/linter-codes.json` to register `E-SYN-013` as an error.

- [x] **Step 1: Collect local call signatures from the lowered module**

Record each local function’s fixed parameter count and whether its final parameter is variadic. Use that information when a call’s callee resolves to a local function; reject a label that appears after the remainder or variadic segment with `E-SYN-013`.

- [x] **Step 2: Preserve formatter nodes without changing parser order**

For each AST call, preserve source node order while requiring fixed positional arguments first, labelled arguments (including `types:`) second, and variadic positional arguments last. The formatter must not reorder syntax nodes.

- [x] **Step 3: Add the style lint diagnostic**

When a call contains a labelled argument after the remainder or variadic segment, emit:

```text
E-SYN-013: labelled arguments must precede the remainder or variadic values
```

Use `Severity::Error` and `Category::Syntax`, anchored to the later label. Do not reorder the source as a compatibility bridge.

- [x] **Step 4: Run the call-order focused tests and the formatter twice**

Run: `cargo test sexpr_tooling::tests --lib` and `cargo test syntax::printer::tests --lib`.

Expected: strict ordering rejects the old trailing-label form directly with `E-SYN-013`.

### Task 4: Update current documentation, schemas, and repository tooling

**Files:**
- Modify: `README.md`, `tests/README.md`, `stdlib/README.md`, `docs/index.md`, `docs/decisions/s-expression-language.md`, `docs/reference/conditional-compilation.md`, `docs/reference/editor-support.md`, `docs/reference/project-layout.md`, `schemas/package-manifest.schema.json`, and current status/reference pages that describe source filenames.
- Modify: repository-owned `skills/vibra-coding/SKILL.md`, `skills/vibra-coding/references/language-conventions.md`, and `skills/vibra-cli/references/cli-workflows.md` where they describe source extensions.
- Modify: `docs/plans/2026-08-02-vib-extension-and-call-order.md` and `docs/index.md` to track this plan.

- [x] **Step 1: Replace current source-extension examples with `.vib`**

Document `project.vib` as the project source file and use `.vib` for source
modules, conditional parts, imports, editor associations, and package entry
patterns.

- [x] **Step 2: Document the strict call-order rule**

State that parsing requires fixed positional → labelled → variadic order and reports `E-SYN-013` for trailing labels; formatting preserves valid source order and only changes layout.

- [x] **Step 3: Add the plan link to `docs/index.md`**

Add the dated plan under the existing Plans section.

### Task 5: Full verification and integration

**Files:**
- Review: complete repository diff, renamed-file inventory, and submodule status.

- [x] **Step 1: Run formatting/lint checks for changed Vibra files**

Run the repository’s `.vib` formatter/linter commands on the changed source paths, using the built CLI where necessary.

- [x] **Step 2: Run both required test suites**

Run: `cargo test`.

Run: `cargo run -- test`.

Expected: both exit successfully with zero failures; record any pre-existing failure separately instead of claiming completion.

- [x] **Step 3: Verify no runnable `.vibra` source remains**

Run: `rg -n --hidden --glob '!target/**' --glob '!.git/**' '\.vibra' src tests examples stdlib schemas README.md docs/reference docs/decisions skills` and inspect each remaining match. Remaining matches must be the explicit legacy-extension regression, legacy lock names, or historical/archive text—not source or project paths accepted by the loader.

- [x] **Step 4: Review the final diff and integrate the changes**

Run: `git status --short`, `git diff --check`, and `git diff --stat`. Confirm all requested behavior is represented, no unrelated user changes were touched, and the root repository plus the `stdlib` submodule are in a deliverable state.
