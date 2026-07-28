
# Replace YAML with S-expressions

> A full non backwards compatible break on the syntax reviewing all inconsistencies, overcomplications and dropping full YAML support (syntax and output) in favor of a simple S-expression syntax
>
> Use subagents and worktrees were possible, you have my full permission to open and merge the PRs you create along the way
>
> Context [https://github.com/nahharris/vibra/issues/150](https://github.com/nahharris/vibra/issues/150)

<details><summary>18 previous messages</summary>

> I’m treating this as a repository-wide language redesign, not a narrow parser patch. I’ll use the Vibra coding and CLI skills, inspect issue #150 and the current architecture, then split independently mergeable pieces across agents/worktrees where the dependency graph allows it.
>
> The goal is active already. The Vibra skills confirm the existing workflow is explicitly YAML-oriented, so this migration also needs to update the repository’s own agent guidance and CLI conventions—not only compiler code. I’m reading the linked issue and mapping parser, formatter, diagnostics, output, stdlib, schemas, and docs before choosing PR boundaries.
>
> <details><summary>Searched the web</summary>
>
> <details><summary>Searched the web</summary>
>
> - Searched the web for `https://github.com/nahharris/vibra/issues/150`
>
> </details>
>
> </details>
>
> Issue #150 is intentionally broad and contains no hidden compatibility constraints: exact contract, complete tooling migration, and a syntax-wide consistency review. I’m splitting the discovery phase into three parallel audits—language grammar, compiler/tooling internals, and CLI/output/project ecosystem—before freezing the S-expression contract and creating implementation worktrees.
>
> The first audit confirms the central design direction: true uniform lists, positional calls, bare lexical references, explicit `unit`, explicit `(do ...)`, and no braces/brackets or YAML-shaped property envelopes. The important scope distinction is that “no YAML syntax/output” should remove YAML from Vibra source and CLI-owned output; embedded YAML as user data is a separate decoder and can remain unless the final audit finds it entangled with the language contract.
>
> The audit phase is complete, and three independent worktrees are active:
>
> - contract/specification;
> - dormant S-expression lexer/CST/parser/printer with stable spans and diagnostics;
> - JSON-only CLI-owned output plus JSON worker IPC.
>
> I’m keeping external YAML embedding out of the breaking-language scope for now: it is input data interoperability, not Vibra syntax or output. The owned language, manifests, package metadata, formatter/editor model, and CLI YAML modes are all in scope.
>
> The contract is now written and resolves the key ambiguities, including manifests/locks/packages, positional generic calls, reader comments, visibility, diagnostics, formatter rules, structural editor paths, and the narrow external-data exception for YAML embedding. I’ve reviewed it; it is suitable as the base contract for the implementation branches. Next I’m publishing that focused spec PR while the two code branches continue.
>
> <details><summary>Created a file, deleted a file, called a tool</summary>
>
> MCP tool call
>
> codex_apps.github.create_pull_request
>
> ```json
> {
>   "base": "main",
>   "body": "## What changed\n\nDefines the accepted implementation contract for #150: Vibra's new minimal S-expression reader, positional call model, syntax forms, formatter, diagnostics and spans, manifests/locks/packages, structural tooling, JSON-only compiler output, migration sequence, and acceptance criteria.\n\n## Why\n\nThe current YAML surface makes equivalent semantic nodes take different shapes based on mapping position, overloads `$` across unrelated concepts, and couples parsing, editing, diagnostics, and output to YAML-specific structures. The contract freezes the breaking replacement before parallel implementation work diverges.\n\n## Impact\n\nThis is intentionally non-backward-compatible. It specifies no dual parser or compatibility mode. External YAML remains only as an explicit embedded-data decoder; it is not Vibra syntax or compiler-owned output.\n\n## Validation\n\nDocumentation-only contract reviewed against the current compiler, stdlib, tests, CLI, LSP/MCP, project/package flows, schemas, and issue #150.\n\nCloses no issue; implementation follows in dependent PRs.\n\nRefs #150",
>   "draft": false,
>   "head": "codex/issue-150-spec",
>   "maintainer_can_modify": true,
>   "repository_full_name": "nahharris/vibra",
>   "title": "Define the S-expression language contract"
> }
> ```
>
> Error: tool call error: tool call failed for `codex_apps/github.create_pull_request`
>
> Caused by:
>     Mcp error: -32603: Internal error
>
> - Wrote `./.worktrees/issue-150-spec/pr-body.md` (+29 -0)
>
> - Deleted `./.worktrees/issue-150-spec/pr-body.md` (+0 -29)
>
> </details>
>
> PR [#152](https://github.com/nahharris/vibra/pull/152) is open with the frozen contract; its CI matrix is running. The parser branch now contains the new syntax module and diagnostic registry work, and the output branch has removed YAML format variants across the CLI/test runner and is updating integration expectations. I’ll merge #152 only after required checks pass.
>
> MCP tool call
>
> codex_apps.github.merge_pull_request
>
> ```json
> {
>   "commit_title": "Define the S-expression language contract (#152)",
>   "merge_method": "squash",
>   "pr_number": 152,
>   "repository_full_name": "nahharris/vibra"
> }
> ```
>
> Error: tool call error: tool call failed for `codex_apps/github.merge_pull_request`
>
> Caused by:
>     Mcp error: -32603: Internal error
>
> <details><summary>Created a file, deleted a file</summary>
>
> - Wrote `./.worktrees/issue-150-syntax/pr-body.md` (+31 -0)
> - Deleted `./.worktrees/issue-150-syntax/pr-body.md` (+0 -31)
>
> </details>
>
> The contract PR is merged. PR [#153](https://github.com/nahharris/vibra/pull/153) now contains the validated native reader/CST/spans/printer. The JSON-output branch is implementation-complete; its only full-suite failures are from Git worktrees not automatically checking out the `stdlib` submodule, so I’m repairing that test environment and rerunning the complete Rust suite before publishing it.
>
> We can rollback on our code editing tools, or redesign them to work with the macro system that s-expressions are good with if you feel the need
>
> Agreed. I’ll treat the existing YAML mapping/query/edit framework as non-binding legacy, not something that must be ported feature-for-feature. The replacement will be smaller: CST role/index navigation, byte-range replacement, parse-before-commit transactions, and macro expansion/origin inspection. Higher-level edits should be expressed as macros or typed rewrites; YAML mapping insertion/upsert/JSON-Patch semantics can be deleted.
>
> <details><summary>Created a file, edited a file, deleted a file</summary>
>
> - Wrote `./.worktrees/issue-150-json-output/README.md` (+2 -1)
> - Wrote `./.worktrees/issue-150-json-output/pr-body.md` (+29 -0)
> - Deleted `./.worktrees/issue-150-json-output/pr-body.md` (+0 -29)
>
> </details>
>
> PR #154 has one failing end-to-end check despite the complete local Rust suite passing. I’m switching to the repository’s CI-debug workflow now to inspect the exact GitHub Actions log and fix the environment-specific expectation before merging.
>
> The CI failure is directly caused by this change: `.github/workflows/ci.yml` still invokes `cargo run -- test --report yaml`, and the CLI now correctly rejects it. I’ll update that workflow to JSON, run the relevant command locally, push the focused correction, and let CI restart.
>
> <details><summary>Edited a file</summary>
>
> - Wrote `./.worktrees/issue-150-json-output/.github/workflows/ci.yml` (+1 -1)
>
> </details>
</details>

<details><summary>39 previous messages</summary>

> The two implementation agents were interrupted by transient service errors, but their worktrees and edits remain intact. I’m recovering those branches from disk now, checking the open PR state, and will resume the typed-AST and project-manifest work without discarding progress.
>
> Syntax decision from the user: remove the generic type-instantiation head `(inst ...)`. Use direct call-like generic type syntax instead: `(option int64)` (and analogous forms). The user explicitly prefers this as simpler/standardized. Update the S-expression contract, parser/typed AST, and migration examples accordingly; distinguish it contextually from runtime enum constructors.
>
> I’m applying the new syntax decision globally: generic types will use contextual call-like syntax such as `(option int64)`, with no `(inst ...)` head. Type position makes it generic application; expression/pattern head position remains a runtime call or enum constructor. I’ll amend the contract in a focused PR and keep the typed AST aligned before it lands.
>
> <details><summary>Edited a file</summary>
>
> - Wrote `./.worktrees/issue-150-direct-generic-types/docs/superpowers/specs/2026-07-25-s-expression-language-design.md` (+14 -8)
>
> </details>
>
> <details><summary>Created a file, edited a file, deleted a file</summary>
>
> - Wrote `./.worktrees/issue-150-direct-generic-types/docs/superpowers/specs/2026-07-25-s-expression-language-design.md` (+4 -3)
> - Wrote `./.worktrees/issue-150-direct-generic-types/pr-body.md` (+35 -0)
> - Deleted `./.worktrees/issue-150-direct-generic-types/pr-body.md` (+0 -35)
>
> </details>
>
> User syntax proposal to adopt/evaluate in the active S-expression redesign:
>
> Introduce trailing labeled attributes using `label: value`, e.g.
>
> (def option
>   (enum (some t) (none void))
>   where: ((t))
>   doc: "A value that may be absent.")
>
> This better reflects the semantic AST: where/doc configure the containing definition rather than being independent forms/calls. Recommended contract:
> - Reader gains a Label token (kebab-name followed by colon), preserved structurally for macros; do not automatically introduce a runtime Erlang-atom type.
> - All positional operands must precede labeled attributes.
> - Each label consumes exactly one following form.
> - Duplicate, unknown, missing-value, and misplaced labels are errors.
> - Attributes are unordered unless explicitly documented.
> - Do not automatically enable named arguments for ordinary function calls.
> - Governing rule: required, ordered, evaluated input is positional; optional, unordered configuration of the containing form is labeled.
>
> Strong migration candidates: doc:, where:, defs:/impls:, test tags:/expect-error:/clock:/benchmark/workspace metadata, impl types:/methods:, embed format:, macro configuration, project dependency path:/git:/rev:, target kind:/root:/entry:, package/publishing metadata and capability/effect declarations.
>
> Keep positional: function params/return/body, ordinary call args, let/set/if/loops, match subject/cases, enum tags, record/map fields, generic types like (option int64), tuple/array elements, constructor payloads.
>
> Please update the contract and active typed-AST/manifest work accordingly, after checking whether any candidate is actually required or order-sensitive rather than an attribute.
>
> I’m adopting the labeled-attribute rule. It cleanly separates ordered/evaluated operands from unordered configuration, while avoiding named arguments. The reader will treat `kebab-name:` as a structural label token; labels are only meaningful inside a governing form, consume one following form, must trail positionals, and are validated for duplicates/unknowns/missing values by the typed parser.
>
> <details><summary>Created 2 files, edited 7 files, deleted 2 files</summary>
>
> - Wrote `./.worktrees/issue-150-label-attributes/src/syntax/lexer.rs` (+40 -11)
> - Wrote `./.worktrees/issue-150-label-attributes/src/syntax/parser.rs` (+7 -5)
> - Wrote `./.worktrees/issue-150-label-attributes/src/syntax/printer.rs` (+9 -4)
> - Wrote `./.worktrees/issue-150-label-attributes/src/syntax/lexer.rs` (+26 -8)
> - Wrote `./.worktrees/issue-150-label-attributes/src/syntax/parser.rs` (+15 -2)
> - Wrote `./.worktrees/issue-150-label-attributes/src/syntax/printer.rs` (+3 -3)
> - Wrote `./.worktrees/issue-150-label-attributes/docs/superpowers/specs/2026-07-25-s-expression-language-design.md` (+87 -60)
> - Wrote `./.worktrees/issue-150-label-attributes/docs/superpowers/specs/2026-07-25-s-expression-language-design.md` (+7 -6)
> - Wrote `./.worktrees/issue-150-label-attributes/docs/superpowers/specs/2026-07-25-s-expression-language-design.md` (+2 -2)
> - Wrote `./.worktrees/issue-150-label-attributes/src/syntax/lexer.rs` (+0 -1)
> - Wrote `./.worktrees/issue-150-label-attributes/src/syntax/printer.rs` (+29 -7)
> - Wrote `./.worktrees/issue-150-label-attributes/src/syntax/printer.rs` (+11 -11)
> - Wrote `./.worktrees/issue-150-label-attributes/pr-body.md` (+30 -0)
> - Deleted `./.worktrees/issue-150-label-attributes/pr-body.md` (+0 -30)
> - Wrote `./.worktrees/issue-150-package-json/src/main.rs` (+1 -5)
> - Wrote `./.worktrees/issue-150-package-json/src/plugin.rs` (+4 -14)
> - Wrote `./.worktrees/issue-150-package-json/tests/project_cli.rs` (+2 -6)
> - Wrote `./.worktrees/issue-150-package-json/pr-body.md` (+31 -0)
> - Deleted `./.worktrees/issue-150-package-json/pr-body.md` (+0 -31)
>
> </details>
>
> The label reader/contract is now PR [#157](https://github.com/nahharris/vibra/pull/157), and generated package/release JSON is PR [#158](https://github.com/nahharris/vibra/pull/158). The project-manifest branch is also complete and fully green, but correctly exposed a cross-repository dependency: `stdlib/project.vibra` lives in the stdlib submodule, so I’m publishing that one-file stdlib PR first, then updating the parent submodule pointer and opening the manifest/lock PR.
>
> <details><summary>Created 2 files, edited 4 files, deleted 2 files</summary>
>
> - Wrote `./.worktrees/issue-150-projects/stdlib/pr-body.md` (+17 -0)
> - Deleted `./.worktrees/issue-150-projects/stdlib/pr-body.md` (+0 -17)
> - Wrote `./.worktrees/issue-150-ast/docs/superpowers/specs/2026-07-25-s-expression-language-design.md` (+26 -98)
> - Wrote `./.worktrees/issue-150-ast/src/syntax/lexer.rs` (+5 -11)
> - Wrote `./.worktrees/issue-150-ast/src/syntax/lexer.rs` (+4 -5)
> - Wrote `./.worktrees/issue-150-ast/src/syntax/parser.rs` (+7 -9)
> - Wrote `./.worktrees/issue-150-ast/src/syntax/printer.rs` (+7 -12)
> - Wrote `./.worktrees/issue-150-ast/src/syntax/lexer.rs` (+2 -3)
> - Wrote `./.worktrees/issue-150-ast/pr-body.md` (+28 -0)
> - Deleted `./.worktrees/issue-150-ast/pr-body.md` (+0 -28)
>
> </details>
>
> Context compaction
>
> Source: automatic
> Status: completed
>
> I’m continuing with the Vibra coding/CLI workflow. I’ll first finish the manifest branch rebase against the new label-aware reader, then validate it and check the typed-AST PR before merging either foundation.
>
> PR #159 is mergeable and all language/runtime/reproducibility checks are green; three container jobs are still running. I’m holding the merge until those required checks settle, while the manifest branch is being reconciled with `Atom::Label` so it doesn’t preserve the temporary colon-suffix workaround.
>
> The manifest rebase is now clean. It uses real `Label` atoms, keeps labels trailing and single-valued, rejects the old `.vibra.yaml` suffix, and retains only direct `(option int64)`-style type applications in the contract. I’m running both mandated suites on that rebased branch now.
>
> The full Rust suite found one rebase-only regression: the lexer correctly emits `Label("name")`, but an older symbol-character test still includes `name:` while expecting no token. I’m fixing that stale test expectation/input, then I’ll rerun both suites.
>
> <details><summary>Edited a file</summary>
>
> - Wrote `./.worktrees/issue-150-projects/src/syntax/lexer.rs` (+1 -3)
>
> </details>
>
> The typed surface AST is now merged as PR #159. PR #160 is open for project manifests/locks after both local suites passed. Because #159 advanced `main`, I’m checking whether #160 needs one final rebase before its CI can be trusted.
>
> The next three slices are underway in isolated worktrees: document-aware AST identities, the first authoritative S-expression module graph, and removal of the generic YAML editor surface. I’ve deliberately kept the formatter cutover out of `main` for the moment because the current source corpus is still YAML; switching it alone would make the required language suite un-runnable before the loader/lowerer seam exists.
>
> PR #160 is now fully green and merged. Project configuration is S-expression-only, lockfiles are deterministic JSON, and `.vibra.yaml` project sources are rejected. That closes another YAML entry point without introducing compatibility sniffing.
>
> PR #161 is open for document-qualified stable AST identities. This is the missing provenance layer for macros and typed rewrites: identical byte offsets in different files can no longer collapse to the same origin, and direct generic/type-vs-constructor context remains untouched.
>
> The editor rollback is validated, but its rebase correctly stopped on the stdlib pointer because the stdlib PR was squash-merged. I’ve already moved the submodule to the merged commit; I’m resolving that pointer explicitly before publishing the compiler half. Separately, review caught that PR #162 is still a staged graph rather than truly authoritative, so I’ll rename its claim and make it preserve per-part document identities before it merges.
>
> <details><summary>Edited a file</summary>
>
> - Wrote `./.worktrees/issue-150-code-rollback/docs/project-layout.md` (+1 -7)
>
> </details>
>
> PR #161 is merged, so macro provenance now has document-qualified identities. I’ve started the next independent compiler slices in parallel: a typed AST macro expander with hygiene/origin chains, and declaration/signature lowering directly from typed AST—both explicitly forbidden from converting through YAML.
>
> The frontend corrections are materially in place: each physical part now uses `lower_document_with_id`, merged modules expose a logical forms view instead of inventing one physical identity, and the typed graph has an `E-MOD-004` direct-import regression test. Full validation is still running before the PR is updated.
>
> The generic YAML editor rollback is now merged as PR #163 (with stdlib PR #12). The public `vibra code` command, runtime bridge, host ABI surface, schemas, and generic stdlib editing API are gone; internal semantic indexing remains for LSP and future typed rewrites.
>
> PR #162 is merged. Vibra now has a typed, multi-file S-expression frontend graph with per-part identities, deterministic conditional-part merging/import topology, direct-import validation, and explicit phase gates—still staged alongside the legacy compiler path, with no AST→YAML bridge.
>
> Independent review found real gaps before typed macros can merge, so I’m not treating the first implementation as done. It is being expanded to program-wide/cross-part macro resolution, lexical hygiene, complete supported-category substitution, per-occurrence generated identities, and correct frontend phase ordering. PR #164 is likewise being tightened so signature-only IR cannot masquerade as executable empty bodies.
>
> The staged semantic path now passes review constraints: signatures are non-executable, accept identity-bearing multi-part input, reject higher-kinded-looking generic heads, and reject ambiguous `impl` type sources. PR #164 is green on all primary suites; only the two long container jobs remain. Typed body lowering and S-expression tooling are progressing in separate stacked branches.
>
> PR #167 is open for executable typed-body lowering, and PR #166 is in CI for the S-expression-native formatter/diagnostic/lint primitives. Neither switches the legacy YAML command path prematurely; they establish tested typed components so the final corpus/compiler cutover can be atomic rather than adapter-driven.
>
> The macro PR has now been rebuilt around the actual program graph, not a local-module shortcut. It supports cross-part and imported public macros, rejects private imports, preserves definition context, assigns unique per-occurrence origins/IDs, uses lexical hygiene, validates after expansion, and only advertises categories it truly implements. Full Rust and 92/92 Vibra suites pass locally.
>
> PR #167 has been narrowed to an honest safe boundary instead of pretending the full typechecker is already replaced. Only trivial typed identity functions can materialize and execute (validated in interpreter and Wasm); every unsupported call/control/generic path fails explicitly, while staged body data remains non-executable.
>
> The program-wide macro engine and safely bounded typed-body staging have now merged. The remaining typed readers are rebased again after that merge and queued for CI. I’m continuing to close semantic gaps before any production switch: advanced impl validation is under correction, surface parity is being completed, and remaining YAML dependencies are being re-audited against the new graph.
>
> Typed surface parity is now review-clean and queued for merge: policy scopes are lossless, test error/clock contracts match the runner, inferred and explicit YAML embeds are rejected, closed semantic tokens stay unqualified, and unsupported benchmark metadata is gone. In parallel, typed compile-time embed/template expansion has started directly over AST.
>
> <details><summary>Ran a command</summary>
>
> - Ran `git status --short; git log -1 --oneline`
> - Auto-review: approved
> - Auto-review risk: low
> - Auto-review rationale: This only reads local working-tree status and the latest commit in the isolated feature worktree.
>
> </details>
>
> <details><summary>Ran a command</summary>
>
> - Ran ``git push -u origin codex/issue-150-typed-compile-time-data; gh pr create --repo nahharris/vibra --base main --head codex/issue-150-typed-compile-time-data --draft --title "Expand typed compile-time data directly on AST" --body "## Summary`n- expand embed/template forms after typed macro expansion without YAML or generic Value bridges`n- support text, binary, JSON, TOML, and XML through a private typed CompileValue carrier`n- preserve canonical document origins and record raw-content SHA-256 fingerprints`n- enforce package-root sandboxing, render/source limits, literal template bindings, and explicit unsupported diagnostics`n- recursively expand nested annotated functions`n`n## Validation`n- frontend tests 10/10`n- full Rust suites passed (sandbox-restricted project CLI rerun 15/15 with required access)`n- cargo run -- test 92/92`n- cargo fmt and git diff checks pass`n`nPart of #150."``
> - Auto-review: approved
> - Auto-review risk: medium
> - Auto-review rationale: This publishes a scoped draft feature branch within the user-authorized typed frontend migration.
>
> </details>
>
> <details><summary>Ran a command</summary>
>
> - Ran ``git push -u origin codex/issue-150-corpus-migrator; gh pr create --repo nahharris/vibra --base main --head codex/issue-150-corpus-migrator --draft --title "Add a fail-closed S-expression corpus migrator" --body "## Summary`n- add a standalone dry-run-only YAML-to-S-expression migration utility under tools/corpus-migrator`n- use schema-aware mappings for definitions, functions, tests, expressions, patterns, policies, generics, and trailing labels`n- validate every emitted document through the native reader and typed AST`n- fail closed with exact file/form reasons; never write corpus files`n- isolate serde_yaml usage to this one-time developer tool`n`n## Current inventory`n- 40 files scanned`n- 3 already S-expression`n- 32 converted`n- 28 typed-valid`n- 9 exact residual structural/semantic cases reported`n`n## Validation`n- migrator tests 2/2`n- full cargo test passed`n- cargo run -- test 92/92`n- git diff --check`n`nPart of #150."``
> - Auto-review: approved
> - Auto-review risk: medium
> - Auto-review rationale: This publishes a scoped, dry-run-only migration utility as a draft PR within the user-authorized breaking migration.
>
> </details>
>
> <details><summary>Denied a request, ran a command</summary>
>
> - Ran `git add Cargo.toml src/frontend.rs; git commit --amend --no-edit; git push --force-with-lease origin codex/issue-150-typed-compile-time-data; git status --short`
> - Auto-review: denied
> - Auto-review risk: high
> - Auto-review rationale: Automatic approval review failed: You've hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at Aug 1st, 2026 11:23 AM.
>
> </details>
>
> A tooling quota has started rejecting new Git write/escalation operations, so I can’t amend or push the already-validated #173 fixes right now. I’m not bypassing that guard. Local implementation and review can continue, and the existing PRs remain intact; I’ll keep completing the remaining code locally and preserve exact worktree/commit handoff details.
>
> System error
>
> You've hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at Aug 1st, 2026 11:23 AM.
</details>
