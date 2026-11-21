# Reader fuzz campaign

The harness is the `vibra-conformance` binary `fuzz`. It has no external
runner dependency: a seed and an iteration number fix every input, so a run
is reproduced by its printed seed.

The CI smoke command is:

```text
cargo run --locked --offline -p vibra-conformance --bin fuzz -- --profile ci-smoke
```

The full campaign runs every target at its campaign budget:

```text
cargo run --locked --offline --release -p vibra-conformance --bin fuzz -- --target all
```

| Target | Campaign | Smoke | Input | Property |
| --- | --- | --- | --- | --- |
| `lexer` | 100,000 | 2,000 | random bytes | no panic; an encoding fault lies inside the input; the token texts of valid UTF-8 concatenate to it |
| `source` | 100,000 | 2,000 | keyword soup from the reserved words, corpus mutations, deep nesting | no panic; every byte in exactly one leaf; every diagnostic span inside the input |
| `data` | 100,000 | 2,000 | data-token soup, corpus mutations, deep nesting | as `source` |
| `format` | 20,000 | 500 | corpus inputs and their mutations | a document with a diagnostic is returned unchanged; any other formats to a text that reads clean, formats to itself, and keeps its tokens |

Options:

- `--target lexer|source|data|format|all` selects the targets; all by
  default.
- `--iterations N` replaces each target's budget.
- `--seed N` (decimal or `0x` hexadecimal) moves the campaign.
- `--deep-limit N` bounds the generated nesting: 256 in the smoke, 20,000 in
  the campaign.
- `--corpus-root PATH` names the directory holding `fuzz/corpus` and
  `conformance/cases`, whose `.vib` and `.vibon` files seed the mutations.

The format target keeps the tokens up to what the canonical format may
change: an empty `takes {}`, `labelled {}`, `given {}`, or `performs {}` is
removed, and the sections and members of a declaration move to their
canonical order. When the trivia-free token sequence differs, the input
must hold a section keyword and the two sequences must hold the same
tokens.

Each finding prints its target, iteration, seed, and input, and the run
exits 1. A finding is closed by a reduced `V2-SRC-READER-*` regression case
in `conformance/cases`, never by skipping its seed.
