# Milestone 1 fuzz evidence

`m1.toml` is the committed campaign configuration. It fixes the pinned
toolchain, deterministic seed, target list, worker count, input corpus, case
budgets, nesting limit, and log location. The harness is the
`vibra-conformance` binary `m1-fuzz`; it has no external runner dependency.

The short CI smoke command is:

```text
cargo run --locked --offline -p vibra-conformance --bin m1-fuzz -- --profile ci-smoke
```

The bounded gate campaign is the `campaign_command` in `m1.toml`. Capture its
stdout at `target/step11/m1-fuzz-campaign.log`. A smoke run is not evidence of
the configured campaign: the exit-gate record must include the separate
campaign command, all six target summaries, and the failure disposition.

The generator covers raw invalid UTF-8, valid source and VIBON text, deep
nesting, long tokens, truncated quoted escapes, Unicode scalars and combining
marks, CRLF, comments, mismatched delimiters, recovery, arbitrary structural
query offsets, and mutations of the checked-in conformance inputs. A caught
panic is a failed case; a nontermination outside the documented process bound
is a failed campaign run.
