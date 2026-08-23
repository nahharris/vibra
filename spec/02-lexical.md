# 2. Lexical structure

Status: draft

## Files

A Vibra source file is UTF-8, has the extension `.vib`, and contains zero or
more top-level forms. There is no byte-order mark, no shebang, and no
alternative extension. A file that is not valid UTF-8 is `E-LEX-001`.

## Reader grammar

```ebnf
module          = trivia, { top-form, trivia }, EOF ;

form            = atom | list ;
list            = "(", trivia, form, { required-trivia, form }, trivia, ")" ;

atom            = string | integer | float | boolean | unit
                | atom-literal | label | symbol ;

boolean         = "true" | "false" ;
unit            = "unit" ;

symbol          = segment, { ".", segment } ;
segment         = lower, { lower | digit | "-" } | "_" ;
lower           = "a".."z" ;
digit           = "0".."9" ;

atom-literal    = "@", segment, { ".", segment } ;
label           = segment, ":" ;

integer         = [ "-" ], digit, { digit | "_" } ;
float           = integer, ".", digit, { digit | "_" }, [ exponent ]
                | integer, exponent ;
exponent        = ( "e" | "E" ), [ "+" | "-" ], digit, { digit } ;

string          = '"', { string-char | escape }, '"' ;
escape          = '\\"' | "\\\\" | "\\n" | "\\r" | "\\t" | "\\0"
                | "\\u{", hex, { hex }, "}" ;

trivia          = { whitespace | line-comment } ;
required-trivia = ( whitespace | line-comment ), trivia ;
line-comment    = ";", { any-char-except-newline }, ( newline | EOF ) ;
```

The lexer recognizes numbers before symbols. `(` and `)` are the only
delimiters. Square brackets, braces, commas, quote and quasiquote characters,
dotted pairs, keyword-prefix syntax such as `:name`, and raw or multi-line
string forms are all `E-LEX-002`.

Strings are always double-quoted. There is no interpolation, no raw form, and
no heredoc; Unicode may appear directly in the literal. An unterminated string,
an unrecognized escape, or an escape naming a value that is not a Unicode
scalar is `E-LEX-006`.

## Symbols

Symbols are lowercase kebab-case, optionally dotted:

```text
main            greet-user      option.some      fs.read.open
```

Rules:

- A segment starts with a lowercase letter, or is exactly `_`.
- A segment contains only lowercase letters, digits and `-`. Uppercase letters,
  `?`, `!`, `/`, `*`, `+` and every other punctuation character are
  `E-LEX-003`. A name that reads naturally with a trailing `?` or `!` is spelled
  `is-empty`, `close-or-fail`.
- A dot separates **namespace segments**. It is not member access and it is not
  an operator. `option.some` is one symbol, resolved as a whole.
- `_` is the wildcard. It is legal only in pattern position and as a binder
  that is never read. Using `_` as a value reference is `E-LEX-004`.
- `true`, `false`, `unit`, `self`, and the special form heads listed in the
  grammar sections may not be defined as user symbols (`E-LEX-005`).

There is no lexical distinction between type names, value names and effect
names. Position disambiguates them: type expressions appear only in type
positions, which are structurally determined by the enclosing form.

> **Rejected alternative.** A sigil or PascalCase convention marking type and
> effect symbols was considered. It was rejected because type and value
> positions are already disjoint, so the marker carries no information the
> reader lacks, while costing an entire naming convention and a second casing
> rule. See [Deferred and rejected](10-deferred.md).

## Atoms

An atom is a self-naming constant written with a leading `@`:

```text
@ok    @not-found    @http.gateway-timeout
```

Atoms compare by identity, may be matched, and may key a map. Each atom is also
a singleton type that widens to the type `atom`.

Atoms are how contextual keywords are written. Every position that takes a
fixed vocabulary takes an atom, never a bare symbol and never a string:

```text
visibility: @private    kind: @bin    profile: @core    tags: (@slow)
```

A bare symbol in such a position is `E-ATOM-001`.

## Labels

A label is a segment followed by `:`. It attaches optional, unordered
configuration to its containing form and consumes exactly one following form.

```text
where: ((t))    doc: "..."    effects: (fs.read)    implements: (display)
```

Labels are reader-level structure. A label is not a value, cannot be stored,
and cannot appear in expression position (`E-SYN-001`).

## Operand order

Inside any form, operands appear in exactly three groups, in this order:

1. **Fixed positional operands** — required, ordered, semantically distinct.
2. **Labeled operands** — optional configuration, in source order.
3. **Body or variadic operands** — the remaining forms.

A label appearing after a body or variadic operand is `E-SYN-002`. An unknown
label is `E-SYN-003`, a duplicate label is `E-SYN-004`, and a label with no
following form is `E-SYN-005`.

This order is uniform across every form in the language — definitions, calls,
type expressions and manifests alike. It exists so that an author reading a
partially written form always knows which group the cursor is in.

## Comments and documentation

`;` starts a comment that ends at the newline. Comments are trivia: they never
enter the semantic tree, they may appear wherever whitespace may, and no tool
attaches meaning to their content.

Documentation that tools retain is a `doc:` label with a string value. There is
no doc-comment syntax, and a comment is never promoted to documentation.

```vibra
(import io "@std/io.vib")

(defn greet ((name str)) void
  doc: "Write a greeting for the given name."
  effects: (io.stdout)
  ; the trailing newline is intentional
  (io.stdout.println name))
```

## Canonical form

Exactly one rendering of an accepted program is canonical, and `vibra fmt`
produces it. The formatter changes whitespace and nothing else: it never
reorders operands, never inserts or removes forms, and never rewrites a token
into a different spelling.

Rules, in priority order:

1. **Indentation** is two spaces per nesting level. Tabs never appear in
   formatter output.
2. **A form fits on one line** if its rendering is at most 88 columns and it
   contains no comment. Otherwise it breaks.
3. **When a form breaks**, the head stays on the opening line together with its
   fixed positional operands, each labeled operand starts its own line, and each
   body operand starts its own line, all indented one level from the head.
4. **Trailing parentheses collapse.** A run of closing parentheses is written
   with no whitespace between them, on the line of the last operand.
5. **Blank lines** between top-level forms are preserved up to a maximum of
   one. Blank lines inside a form are removed.
6. **Comments** are attached to the form that follows them at the same
   indentation, and are preserved verbatim including their leading `;`.
7. Numbers keep their written digits; `_` separators are preserved. Strings
   keep their written escapes.

`vibra fmt` is idempotent: formatting formatted output is a no-op, byte for
byte. This is a conformance requirement, not an aspiration, because
style-normalized retrieval depends on it and because a non-idempotent formatter
produces spurious diffs in agent-authored changes.

## Reserved for the implementation

The compiler may generate internal names containing characters that the reader
rejects. Such names never appear in source, never round-trip through the
formatter, and are not part of the language.
