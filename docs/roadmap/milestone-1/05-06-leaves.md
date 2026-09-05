# Steps 5 and 6 — literal and name surfaces

Prerequisite: landed Step 4 and a passing baseline from [validation](validation.md).
Read source-language **Reader**, **Canonical format**, and diagnostics
**Recovery** and the registry. For Step 6 also read type-system **Namespaces
and resolution**; lexical validity and permitted binding roles are different.

## Step 5 implementation sequence

1. Write a table from each literal production to its classifier, decoded
   representation, invalid cases, and diagnostic. Close missing string-error
   contracts identified in [the implementation map](implementation.md).
2. Extend the shared lexer with character-aware token boundaries before generic
   delimiter/comment handling. Exercise direct delimiter characters and quotes
   after a backslash; do not split a character into a delimiter or comment.
   Preserve the complete malformed token and its span.
3. Add a proposed `literal` module to syntax for classification/decoding. Keep
   integer sign, decimal digits, and suffix without parsing into a host `i64`;
   arbitrarily long lexically valid numerics must not overflow the reader.
   Keep float spelling and exact suffix without prematurely rounding through
   host `f64`. Source range checking belongs to a later phase.
4. Recognize maximal numeric candidates before symbols. Validate the entire
   candidate, including exponent and suffix. Recognize booleans and `void`
   before generic names, but retain context so `void` can appear as a type.
5. Decode supported string escapes and exactly one Unicode scalar per character.
   Validate scalar boundaries and reject surrogate escapes. Retain raw source
   independently of decoded values for lossless CST and recovered formatting.
6. Extend formatting only where canonical spelling is specified. Add canonical
   character rendering and suffix preservation. Do not invent numeric rounding
   or an unspecified string-escape preference. Preserve entire recovered
   documents byte-for-byte, including CR/CRLF interiors.
7. Wire the classifier into both document modes and the real reader handler.
   Update provisional opaque-leaf status prose to describe the actual boundary.

| Matrix | Required coverage |
| --- | --- |
| Characters | Direct ASCII and astral scalar; all four names; upper/lower hex input; surrogate; too few/many hex digits; `\ab`; `\newline-x`; bare backslash; whitespace after backslash; delimiter characters |
| Numerics | Each of eight integer and two float suffixes; unsuffixed integer/decimal/exponent; exponent sign; `2f64`; `-1u8` accepted lexically; very long digits |
| Invalid numerics | Unknown suffix, float with integer suffix, trailing text, separator/base spellings, incomplete exponent, `1a`; standalone `-` remains distinct |
| Strings | Each supported escape; embedded escaped quote/backslash; astral scalar; invalid escape/scalar; unfinished escape and quote; Unicode before diagnostic span |
| Formatting | Decode/format/decode value equivalence, suffix retained, canonical character bytes, idempotence, 87/88/89-column boundaries with nested indentation |

Use `@syntax.invalid-character-literal` and
`@syntax.invalid-numeric-literal` with their fixed levels. Do not emit
`@type.numeric-out-of-range` from lexical validation.

## Step 6 implementation sequence

1. Add one name validator shared by symbols, labels, and atoms. Validate each
   dot-separated segment, then classify the exact three discard spellings
   before their ordinary roles. Avoid three subtly different regexes.
2. Keep a lexical category separate from the role selected by the AST context.
   `any` and primitive type names remain ordinary reader symbols. A valid
   dotted name is not a field-access node or resolved identity.
3. Expose classification to the formatter without changing valid spelling.
   Keep role rejection in the contextual parser as Steps 8–9 add those slots;
   Step 6 alone cannot claim binder or resolution validation.
4. Add the same positive/negative lexical cases in source and data leaf paths;
   data shape restrictions arrive in Step 7. Retain exact spans and recovery.

| Accept lexically | Reject lexically |
| --- | --- |
| `a`, `a1`, `a-`, `a--b`, `a.b2-c` | `-a`, `a..b`, `a.1b`, uppercase, underscore, slash, `?`, `!` |
| `@some.name`, `some.name:` | Empty atom/label components, malformed dotted segments |
| `-`, `@-`, `-:` as discards | `-.name`, `@-.name`, `-:.name` |

For both steps, pair every negative family with a following valid sibling to
prove progress and useful recovery. Add host tests inspecting category/value,
span, and retained bytes; add rule-addressed corpus cases with explicit
acceptance, ordered diagnostics, and formatter snapshots. Run the focused
syntax/formatter tests, then the complete [validation sequence](validation.md).
Each step is done only when its whole matrix and contract decisions are closed.
