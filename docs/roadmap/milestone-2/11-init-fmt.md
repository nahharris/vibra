# Step 11 — project initialization and formatting commands

Requires Step 10 and C2/C6/C8/C10. Read tooling **V1 CLI**, **One workspace
engine**, **Project operations**, **Transactional edit plans**, projects
**Packages and targets**, source **Canonical format**:
[tooling](../../spec/05-tooling.md), [projects](../../spec/04-programs-and-packages.md),
[source](../../spec/01-source-language.md).

## Implementation sequence

1. Add `vibra-cli` with a binary named `vibra`. Implement only the reviewed
   `project init` and `fmt` command grammar. Return typed service results;
   keep parsing, checking, file planning and formatting in libraries.
2. `init` produces the canonical minimum layout, an admitted pure entry and
   C8's exact offline stdlib bootstrap where required. Show planned destinations
   interactively and reject nonempty destinations. Recheck conflicts at apply.
3. `fmt` selects mode by extension and previews by default; `--write` applies
   the reviewed revision-checked atomic plan. Recovered documents retain bytes.
4. For source calls, use snapshot binding facts to normalize only proven label
   order, preserving evaluation order and attached comments. Missing semantic
   facts cannot authorize a guessed reordering.
5. Implement only the necessary format-plan foundation, honoring all applicable
   transaction guarantees: confined paths, stale revision refusal, complete
   preflight, reparse/recheck postconditions and no partial writes on failure.
   The later M6 semantic edit system consumes this foundation.
6. Publish versioned init/fmt JSON contracts and stable exit/result mapping from
   C10. Add actual binary tests and corpus operation observations from Step 1.

| Positive | Negative / boundary |
| --- | --- |
| Init into empty temporary destination, then decode/check generated project via service | Nonempty destination, race-created conflicting file, escaping destination |
| Source and project VIBON preview and explicit write | Preview changes no bytes; unknown options/extensions; no content sniffing |
| Idempotent output; labelled call equivalence | Stale revision, I/O failure, failed postcondition: no partial write |
| Comments and 87/88/89-column boundaries | Invalid/recovered input preserved, including CR/CRLF interior bytes |
| Human/JSON outputs with schema validation | Logs on wrong stream, invalid JSON, silently ignored errors |

Run [common validation](validation.md), CLI process tests, and fmt/workspace
plan tests. Use the actual newly built `vibra` binary; in-process service tests
alone cannot prove command grammar or exit behavior. Record exact init/fmt
invocations in validation documentation. No project sync, lint, build, MCP,
rename, fix, or public query command.
