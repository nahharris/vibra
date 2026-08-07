# Vibra S-expression language design

Date: 2026-07-25  
Status: accepted implementation contract for issue #150  
Compatibility: intentionally breaking; there is no YAML compatibility mode

## Decision

Vibra source becomes a small, regular S-expression language. YAML is removed
from source parsing, source formatting, manifests, locks, package metadata,
and compiler-owned output. The implementation must not retain a second source
parser or silently select a source parser from file contents. YAML may remain
only as an explicitly requested external-data decoder for `embed`; decoded data
immediately becomes ordinary Vibra values and is never exposed as Vibra syntax
or compiler-owned YAML output.

This change preserves Vibra's static types, value semantics, module visibility,
generics, nominal implementations, tests, and WebAssembly runtime behavior. It
changes their spelling and removes syntax that exists only to fit semantic
nodes into YAML mappings.

Calls accept fixed positional, labelled, and variadic arguments. The reader
accepts mixed source order, and unlabelled values fill the next unbound fixed
parameter, so a misplaced label remains parseable and semantically checkable.
Canonical formatting emits fixed positional arguments first, labelled arguments
second, and variadic values last. `W-STYLE-002` reports a labelled argument
that follows a variadic value as a warning; it is not a hard parse or type error.
Generic type arguments use the reserved final `types:` call attribute.

## Reader contract

A source file is UTF-8 and contains zero or more forms. Whitespace separates
tokens. `(` and `)` delimit lists.

```ebnf
module       = trivia, { top-form, trivia }, EOF ;
top-form     = import | definition | constant | function | macro | test
             | private ;
form         = atom | list ;
list         = "(", trivia, symbol, { required-trivia, form }, trivia, ")" ;

atom         = string | boolean | integer | float | unit | label | symbol ;
boolean      = "true" | "false" ;
unit         = "unit" ;
label        = kebab-name, ":" ;
symbol       = symbol-start, { symbol-rest } ;
symbol-start = letter | "_" | "-" ;
symbol-rest  = symbol-start | digit | "." | "/" | "?" | "!" ;

string       = '"', { string-char | escape }, '"' ;
escape       = '\"' | '\\' | '\n' | '\r' | '\t' | '\u{', hex+, '}' ;

trivia          = { whitespace | line-comment } ;
required-trivia = (whitespace | line-comment), trivia ;
line-comment    = ";", { any-character-except-line-ending },
                  (line-ending | EOF) ;
```

The lexer recognizes numbers before symbols. Integers are signed decimal
numbers. Floats require a decimal point or exponent. Numeric widths remain a
type-system concern. `nan`, `inf`, and `-inf` are symbols and may only acquire
numeric meaning in APIs that explicitly define it.

Symbols are case-sensitive and must be kebab-case except `_`, which is reserved
for wildcard patterns. A leading `-` has no visibility meaning. Dotted symbols
are ordinary qualified names such as `io.stdout.println` and `option.some`; dots are
not member-access syntax. `/` is reserved for compiler-defined names and must
not appear in user definitions. `true`, `false`, and `unit` cannot be defined
as symbols.

Strings are always double quoted. There are no bare strings, raw strings,
heredocs, interpolation, YAML-style block scalars, or `$` escaping. Unicode may
appear directly. Invalid escapes, invalid Unicode scalar values, and unclosed
strings are syntax errors.

Semicolon comments end at a line ending. Comments are trivia, do not enter the
semantic tree, and may appear wherever whitespace is accepted. Documentation
that tools retain uses `doc: "..."`; comments are not attributes.

Square brackets, braces, commas, colons, quote-reader macros, dotted pairs,
prefix keywords such as `:name`, and YAML document markers are invalid. A colon
is valid only as the final character of a canonical kebab-case label token.
Vibra does not implement a general-purpose Lisp reader.

## Labeled attributes

Trailing labeled attributes configure their containing form:

```vibra
(def option
  (enum (some t) (none void))
  where: (t any)
  doc: "A value that may be absent.")
```

The governing rule is: required, ordered, evaluated input is positional;
optional, unordered configuration of the containing form is labeled. A label
is a reader-level structural atom, not a runtime value or Erlang-style atom.
Each label consumes exactly one following form.

All positional operands must precede the first labeled attribute. Attributes
are semantically unordered unless a form explicitly documents otherwise. The
typed grammar rejects unknown, duplicate, missing-value, and misplaced labels.
Ordinary function calls remain positional; labels do not create named
arguments. The lossless CST preserves labels and source order for macros and
formatting.

## Names, values, and calls

A bare symbol in expression position is a lexical, module, imported, or
compiler-provided value reference. Function arguments are ordinary lexical
names; `$args.name` is removed.

### Lexical names and scoping

Lexical shadowing is a hard compiler error, not a suppressible style lint.
Every `let` or `let-as` binder, function parameter, match `(bind name)`, and
`for` binder must have a name that is not already visible in an enclosing
lexical scope. The compiler reports `E-SCOPE-001` with the shadowing binder as
the primary span and the original binding as related information. Names in
separate sibling scopes may be reused.

Module names and import aliases are not part of this rule. `_` is a wildcard
and is exempt, so repeated `_` binders remain legal. Scope validation runs on
the resolved, post-expansion surface; hygienic macro binders therefore use
their compiler-resolved identities rather than being compared by raw source
text.

Every non-special executable list is a call:

```vibra-expr
(ready)
(echo-bool true)
(second false true)
(io.stdout.println "Hello, World!")
```

Nullary invocation is `(ready)` and the function value is `ready`. Calls are
checked against the resolved signature.

Generic calls normally infer types. When inference is impossible, `types:`
supplies all generic arguments in declaration order:

```vibra-expr
(identity true types: (bool))
(first-of true "ignored" types: (bool str))
```

Partial type application and named type arguments are invalid.

## Top-level forms and visibility

```ebnf
import     = "(", "import", symbol, string, ")" ;
definition = "(", "def", symbol, type-expr, { attribute }, ")" ;
constant   = "(", "const", symbol, type-expr, expr, { attribute }, ")" ;
function   = "(", "defn", symbol, parameters, type-expr, body,
             { attribute }, ")" ;
inline-function = "(", "fn", parameters, type-expr, body,
                  { attribute }, ")" ;
test       = "(", "test.scenario", string, test-case+, ")" ;
test-case  = "(", "test.case", string, expr+, { attribute }, ")" ;
private    = "(", "private",
             (definition | constant | function | macro), ")" ;
parameters = "(", { "(", symbol, type-expr, ")" }, ")" ;
body       = "(", "do", { expr }, ")" ;
```

Imports are public module-local aliases but are never re-exported. Definitions,
constants, functions, and macros are public unless wrapped by `private`. Tests
are runner-discovered declarations rather than exported symbols. `private`
accepts exactly one `def`, `const`, `defn`, or `macro`; imports and tests cannot
be wrapped. `main` remains the program entrypoint.

```vibra
(import io "../stdlib/src/io.vib")

(defn write-prefix (text str) void (do (io.stdout.print text)) visibility: @private)

(defn main () void (do (write-prefix "Hello") (io.stdout.println ", World!")))
```

Type definitions use `(def name type-expr ...)` and place a type constructor
directly after the name. Typed constants use `(const name type-expr expression
...)`; neither form is an untyped top-level expression.

## Type expressions

Primitive types are `int8`, `int16`, `int32`, `int64`, `uint8`, `uint16`,
`uint32`, `uint64`, `float32`, `float64`, `bool`, `void`, and `str`. `self` is
reserved for the existing interface and implementation contexts. A bare symbol
in type position is a type reference.

```ebnf
type-expr = symbol
          | "(", symbol, type-expr+, ")"
          | "(", "record", field-type*, ")"
          | "(", "tuple", type-expr*, ")"
          | "(", "array", type-expr, ")"
          | "(", "map", type-expr, type-expr, ")"
          | "(", "union", type-expr+, ")"
          | "(", "enum", enum-tag+, ")"
          | "(", "interface", interface-member*, ")"
          | "(", "fn-type", "(", type-expr*, ")", type-expr, ")"
          | "(", "newtype", type-expr, ")"
          | "(", "mut", type-expr, ")"
          | "(", "ref", type-expr, ")"
          | "(", "intersect", type-expr+, ")"
          | handle-type | effect-type ;

field-type       = "(", symbol, type-expr, ")" ;
enum-tag         = "(", symbol, type-expr, ")" ;
interface-member = "(", symbol, type-expr, ")" ;

handle-type      = "(", "handle", symbol, ")" ;
effect-type      = "(", "effect", atom, atom, ")" ;
```

`effect-type` was added by the effect system (see
`docs/decisions/effect-system.md`); its two operands are the domain and the
action, and further operands are reserved for handler definitions. The former
capability/policy grammar and its rationale, including `policy.narrow`, are
historical; see [`archive/capability-policy-grammar.md`](../archive/capability-policy-grammar.md).

Generic type application uses the same direct, call-like list shape as other
Vibra forms: `(constructor type...)`. There is no generic `inst` head. Type
position makes `(option int64)` an application of the `option` type
constructor, while expression and pattern positions continue to interpret list
heads as runtime calls or enum constructors. A generic type constructor may
not be referenced bare. Built-in type-form heads such as `record`, `tuple`,
`array`, `map`, and `union` are recognized before user-defined type
constructors. Record field order, tuple order, enum tag order, and interface
member order are source-significant exactly where the existing semantic model
requires order.

Examples:

```vibra
(def pair (tuple a b) where: (a any b any))
(def option
  (enum (some t) (none void))
  where: (t any)
  doc: "A value that may be absent.")

(defn
  unwrap-or
  (input (option t) fallback t)
  t
  (do
    (match
  input
  (option.some (bind value))
  (do (return value))
  (option.none)
  (do (return fallback))
)
  )
  where: (t any)
)
```

Handle, function-interface, and WebAssembly signature forms follow the same
rule: one head followed by positional children. Their semantic inventory does
not change, but no form may accept both a compact and expanded spelling.

## Expressions and statements

`do` is the only sequencing form. Its final expression is its value; an empty
`do` has value `unit`. Function, loop, task, test, match-arm, and conditional
bodies use explicit `do` lists. There is no statement-only sibling-key syntax.

```ebnf
expr = atom
     | call
     | "(", "do", expr*, ")"
     | "(", "let", symbol, expr, ")"
     | "(", "let-as", symbol, type-expr, expr, ")"
     | "(", "set", symbol, expr, ")"
     | "(", "return", [ expr ], ")"
     | "(", "if", expr, body, body, ")"
     | "(", "while", expr, body, ")"
     | "(", "for", symbol, expr, body, ")"
     | "(", "match", expr, match-arm+, ")"
     | "(", "break", ")"
     | "(", "continue", ")"
     | "(", "record", field-value*, ")"
     | "(", "tuple", expr*, ")"
     | "(", "array", expr*, ")"
     | "(", "map", map-entry*, ")"
     | "(", "mut", expr, ")"
     | "(", "ref", expr, ")"
     | "(", "range", expr, expr, expr, ")"
     | "(", "convert", expr, type-expr, literal, ")"
     | "(", "cast", expr, type-expr, ")"
     | "(", "try", expr, ")"
     | "(", "task", captures, body, ")"
     | "(", "spawn", symbol, captures, expr, ")"
     | "(", "join", symbol, symbol, ")" ;

call        = "(", symbol, expr*, ")" ;
field-value = "(", symbol, expr, ")" ;
map-entry   = "(", expr, expr, ")" ;
captures    = "(", "captures", symbol*, ")" ;
match-arm   = pattern, expr ;
```

Match arms are bare alternating pattern/body pairs, not `(case pattern body)`
lists. This is less regular because arm boundaries are positional rather than
delimited, but the corpus-compatible form is retained because it is the
implemented reader grammar. The irregularity is a known trade-off for the
otherwise canonical surface and should be revisited if a future control-flow
extension needs a more regular arm delimiter.

`return` without an expression returns `unit`. `break` and `continue` never
take operands. `join`'s final symbol is the new result binding. Primitive
operations (`add`, `equal`, `not`, and peers), enum constructors, imported
functions, inherent operations, and interface dispatch all use ordinary call
syntax.

### Result and option propagation

`(try expr)` is the one canonical propagation form. It has exactly one
operand; `?` suffixes, reader macros, and other operator spellings are not
part of the language. For a `(result u err)` operand, the enclosing function
must return `(result t err)` with the identical error type. Success continues
with the operand's `u` value; an error returns from the enclosing function
unchanged. An operand whose error type differs requires an explicit conversion
call before propagation; Vibra does not infer an `into`-style conversion at a
`try` site.

The same form applies to options: `(try expr)` accepts an `(option u)` operand
only inside a function returning `(option t)`. `some` continues with its
payload, while `none` propagates from the enclosing option-returning function.
The result and option kinds cannot be mixed.

Propagation is also result handling for the unhandled-result/option rule in
issue #247. A result or option consumed by `(try ...)` is therefore not an
unhandled statement value; no second `match` is required merely to satisfy
that diagnostic.

`try` does not cross a structured task or spawned-computation boundary. Those
sites are rejected instead of attempting to return through a computation that
owns a different control-flow scope. This is a boundary rule, not automatic
cleanup: host handles are still closed only by explicit operations such as
`stream.manage.close`. Early propagation can therefore expose the same
pre-existing handle leak as any other early return; handle lifecycle is owned
by issue #255. The async runtime has scope open/close machinery, but no
user-facing scope form currently connects it to this language contract, so
`try` promises no additional scope teardown.

#### Corpus measurement for issue #248

The `examples/fs-roundtrip.vib` migration is measured mechanically from the
source file: physical lines, `match` forms, maximum simultaneously open
`match` forms, and explicit error-conversion calls. An error-conversion call
means a call such as `fs.stream-error` that changes one error type into
another; result constructors do not count.

| Metric | Before | After |
| --- | ---: | ---: |
| Physical lines | 36 | 31 |
| `match` forms | 3 | 1 |
| Maximum nested `match` depth | 2 | 1 |
| Explicit error-conversion calls | 0 | 0 |

The migrated helper has two `try` sites, both using `fs-error` directly, so
the explicit conversion frequency is `0/2` among its propagation sites. This
single example does not establish a corpus-wide conversion rate; it records
only the observable cost of this migration.

### Unhandled result and option values

The stdlib `result` and `option` types are fallible values. A value of either
type in non-final statement position must be handled with `match` or retained
by `let`/`let-as`; otherwise the compiler emits `W-RESULT-001`. The final
expression of a `do` and a `return` operand are consumed as block/function
values and are exempt. Intentional disposal uses the existing wildcard binder,
for example `(let _ (stream.write.string out text))`. A named `let` or match
`(bind name)` that is never read emits the distinct `W-BIND-001`; a read on any
match arm counts as a read.
Dropping the `$` sigil makes a primitive name syntactically indistinguishable
from a user function of the same name, which the legacy `$`-prefixed table
never had to resolve. The adopted rule: an unqualified call head matching one
of the 22 primitive names resolves to the primitive, and a qualified head (e.g.
`mymod.add`) is never a primitive, regardless of its suffix.

Declaring a function whose bare name is a primitive name is **permitted**. Such
a function is reachable through its qualified name, which is how the standard
library's `option.and`, `option.or`, `result.and`, and `result.or` combinators
are named and called. Only the unqualified spelling is unavailable, and within
the declaring module an unqualified `and` therefore means the primitive rather
than the local combinator. Primitive availability is uniform across every
module and never depends on what a module happens to declare.

Enum constructors have the same ambiguity to resolve, since ordinary call
syntax makes `(mytype.tag ...)` indistinguishable from a qualified call to a
function literally named `tag`. Resolution commits to the enum-constructor
reading only on a full match: the qualified prefix must name a registered
enum type, *and* the suffix must be one of that enum's declared tags. A
prefix match alone (the type exists, but the suffix is not one of its tags)
falls through to ordinary call resolution rather than raising an enum-tag
error -- otherwise a function that merely shares a name with an enum type,
such as the standard library's `option.empty`, could never be called.

The former `policy.narrow` expression and its subsumption rationale are
archived with the decommissioned capability grammar at
[`archive/capability-policy-grammar.md`](../archive/capability-policy-grammar.md).

The current structured-concurrency, reference, mutation, range, conversion,
cast, collection, checked-arithmetic, and scope rules remain unchanged.

## Patterns

```ebnf
pattern = literal | "_"
        | "(", "bind", symbol, ")"
        | "(", qualified-symbol, pattern*, ")"
        | "(", "record", pattern-field*, ")"
        | "(", "tuple", pattern*, ")"
        | "(", "array", pattern*, ")"
        | "(", "map", pattern-entry*, ")"
        | "(", "newtype", type-expr, pattern, ")"
        | "(", "interface", type-expr, pattern, ")" ;
```

An enum constructor pattern uses the same qualified constructor head as an
expression. A payload-free tag is `(option.none)`. `_` is the sole wildcard
form. Bindings use `(bind name)`. Exhaustiveness and binding-scope rules do not
change.

## Definition attributes and implementations

Definition attributes trail all positional operands:

```ebnf
attribute  = label, required-trivia, form ;
type-param = "(", symbol, type-expr*, ")" ;
impl       = "(", "impl", type-expr,
             [ "types:", "(", type-expr*, ")" ],
             [ "methods:", "(", method*, ")" ], ")" ;
method     = "(", "method", symbol, (qualified-symbol | inline-function), ")" ;
```

The known definition/function/macro attributes are `doc: string`, `where:
(type-param*)`, `defs: (function*)`, and `impls: (impl*)`. `where:` lists type
parameters in declaration order inside its single list value. Each following
type is an interface bound; no following types means unbounded. `defs:`
contains named functions using ordinary `defn` syntax. An `impl` keeps its
required interface positional, then uses `types:` and `methods:` for optional
configuration. A method binding is either a qualified function symbol or an
inline anonymous `fn`. Existing nominal dispatch and orphan rules remain.

`comment:` and `lint:` attributes do not exist. Lint suppression moves to CLI or
project configuration so source semantics do not contain diagnostic policy.

## Tests and metadata

The canonical test form is:

```vibra
(test.scenario "arithmetic"
  (test.case "addition is checked"
    (test.assert-eq-int (add 1 1) 2)
    tags: (@language @arithmetic)
    expect-error: (@compile E-OP-002 "overflow")
    clock: (@fixed 0 0)))
```

Each metadata label consumes one value with its own positional schema. Known
labels are `tags:`, `expect-error:`, `clock:`, `benchmark:`, and `workspace:`.
Profiles and tags still
select tests only. Workspace access remains an explicit runner option.
Metadata is unordered and must follow the body.

## Source files, manifests, locks, and packages

`.vib` is the only source extension. `.vib.yaml` and conditional
`.vib.<flag>.yaml` files are not recognized.

`project.vib` is a Vibra source file with a required `(project ...)` root. It
uses the same lexer and scalar rules as every other `.vib` file; project
commands interpret that root as the project description. Repeated children
replace mappings and sequences. The filename has no separate manifest
extension.

```vibra-project
(project
  (package "example" "0.1.0")
  (target app kind: @bin root: "src" entry: "main.vib")
  (dependency std path: "../stdlib"))
```

Targets use a positional name followed by required `kind:`, `root:`, and
`entry:` labels. Optional package documentation uses trailing `doc:`.
Dependencies accept trailing `path:`, `git:`, `rev:`, and `wasm:` labels.
Plugin interfaces use repeated
`(function <name> params: (...) result: ...)` children.

The dependency lock becomes `vibra.lock.json`, canonical UTF-8 JSON with sorted
object keys and a trailing newline. Package and release metadata embedded in
`.vapp` artifacts become canonical JSON. JSON is chosen for these generated
machine contracts because it has mature deterministic serializers and does not
pretend generated metadata is Vibra code.

Legacy YAML manifests, locks, package metadata, and `.vapp` metadata fail with
a targeted migration diagnostic; they are never auto-rewritten during build,
publish, install, or sync.

## Embedded data

The `embed` expression remains, with one positional path and optional explicit
format:

```vibra-expr
(embed "assets/message.txt")
(embed "assets/config.json" format: @json)
```

Supported formats are `text`, `binary`, `yaml`, `json`, `toml`, and `xml`.
`yaml` is an external data-interoperability format only. It is accepted by
explicit `(embed "path" format: @yaml)` and may be inferred from `.yaml` or
`.yml`.
The decoder accepts the library's safe data model, rejects tags and non-string
record keys, and converts mappings, sequences, strings, booleans, numbers, and
null into the same record, array, and scalar values produced by the JSON/TOML/
XML decoders. No YAML node, anchor, alias, style, comment, ordering extension,
or source span crosses that boundary. Embedded YAML cannot contain Vibra forms,
manifests, macros, templates, or compiler directives.

Keeping this decoder does not make YAML a Vibra-owned syntax or output format.
Applications may likewise parse YAML through a third-party runtime library.

Templates use S-expression record and array values for `with` data. Inline
compiler expressions accepted by `vibra exec` and read-only MCP tools use the
same S-expression reader, never YAML.

## Compiler-owned output

JSON is the sole general machine-readable output. `--format json` is explicit,
and commands whose stdout is primarily machine data default to JSON. Commands
intended for people may default to `human`. `raw` remains where the command
returns uninterpreted bytes, and SARIF remains for lint integrations.
Program-owned stdout from `vibra run` is unchanged.

The `yaml` format enum member and `--format yaml` are removed from every command,
including `test`, `fmt`, `lint`, `docs`, `expand`, `effects`, package, plugin,
status, and MCP-related commands. Report files contain JSON and examples use
`.json`. A `.yaml` report path does not select YAML by extension.

JSON Schemas remain JSON Schema documents. Schema descriptions and examples
must describe S-expression source or JSON output; keeping JSON Schema is not
keeping YAML support.

## Diagnostics and spans

Reader errors use the `E-SYN-*` family. `E-YAML-*` codes are deleted, not
repurposed. At minimum the reader distinguishes unexpected character,
unexpected close parenthesis, unclosed list, invalid atom, invalid number,
invalid string escape, unclosed string, invalid UTF-8, and invalid top-level
form.

Every token and syntax node stores a half-open UTF-8 byte span `[start, end)`.
Diagnostics additionally derive one-based line and Unicode-scalar column for
display. The byte span is authoritative for editors and fixes. Related spans
and fixes continue using the diagnostic schema, but fixes identify a document
revision and byte range rather than JSON Patch over YAML mappings.

Semantic diagnostics retain their existing stable codes when the semantic rule
is unchanged. Messages and examples must use the new spelling.

## Canonical formatter

`vibra fmt` parses and prints one canonical representation:

- UTF-8, LF line endings, and exactly one trailing newline;
- two-space indentation;
- one space between atoms on the same line;
- no trailing whitespace;
- short leaf lists remain on one line when the result is at most 88 columns;
- a multiline list places its head after `(`, each remaining child on its own
  indented line, and `)` at the parent's indentation;
- top-level forms are separated by one blank line;
- strings use only the specified escapes and otherwise preserve Unicode;
- numeric literals are normalized without changing type or value;
- comments remain attached to the following form when possible and are never
  synthesized;
- declaration, field, arm, parameter, capture, and body order is preserved.

Formatting is idempotent. There is no alternative flow/block style.

## Macros and expansion origins

Macros are compiler forms over typed syntax categories. They are not runtime
source editors and have no filesystem, environment, process, network, clock, or
random authority.

```ebnf
macro          = "(", "macro", symbol, macro-parameters, syntax-category,
                 macro-body, { attribute }, ")" ;
macro-parameters = "(", { "(", symbol, syntax-category, ")" }, ")" ;
syntax-category = "@expr-syntax" | "@type-syntax" | "@pattern-syntax"
                | "@definition-syntax" | "@module-syntax" ;
macro-body     = "(", "do", macro-expr+, ")" ;
macro-expr     = atom
               | "(", "let", symbol, macro-expr, ")"
               | "(", "if", macro-expr, macro-body, macro-body, ")"
               | "(", "quote", syntax-category, form, ")"
               | "(", "unquote", symbol, ")"
               | "(", "splice", symbol, ")"
               | "(", "capture", symbol, ")" ;
```

There is no `statement-syntax`: statements are expressions and sequencing is
`do`. Macro arguments are positional. Invocation uses ordinary expression
syntax and macro resolution precedes value-call resolution only in expression
or definition positions:

```vibra
(macro
  unless
  (condition @expr-syntax body @expr-syntax)
  @expr-syntax
  (do
    (quote @expr-syntax (if (not (unquote condition)) (do (unquote body)) (do)))
  )
)

(defn main () void
  (do
    (unless ready (io.stdout.println "not ready"))))
```

`quote` requires an explicit result category. `unquote` inserts exactly one
node of the category required by its grammar position. `splice` is legal only
in a grammar-declared repeated-child position and requires a syntax list of
the matching category. `capture` is the sole explicit opt-out from hygiene and
resolves its symbol at the invocation site.

Quoted binders and references use compiler symbol identities and lexical
scopes, never textual suffixes. Free quoted names resolve in the macro
definition's module. Imported macros retain that definition context. A macro
and an ordinary value may not declare the same symbol in one module.

Direct generic type syntax remains contextual: `(option int64)` in a type slot
is a type application, while the same list shape in an expression slot is a
value call or macro invocation. Macro resolution never intercepts a type
application. Mandatory quote categories make quoted call-like lists
unambiguous.

Expansion remains deterministic and bounded by stable limits for recursion
depth, evaluation steps, and generated nodes. Limit and category diagnostics
name the macro invocation as the primary span and attach its definition and
quote-template spans as related information.

Every expanded AST node has an `OriginId` into an origin arena:

```text
Source {
  document,
  span
}

Expansion {
  macro-symbol,
  call-site: OriginId,
  definition-site: SourceSpan,
  template-site: SourceSpan,
  parent: OriginId
}
```

Quoted nodes originate at the quote template. Unquoted nodes retain their
source origin and add the expansion as their parent. Expansion fingerprints
include the macro definition, invocation syntax, resolved imports, and compiler
version.

`vibra expand <entry>` prints canonical expanded S-expression source.
`--format json --origins` additionally emits modules, expanded node kinds and
spans, and the origin arena. `--at <path>:<byte> --format json` returns the
smallest expanded node at that source position and its complete origin chain.
Source output may show origin comments only with explicit `--annotate`.

The LSP exposes expansion origins through related diagnostic information and a
read-only `vibra/expansionAt` request. MCP may wrap the same read-only compiler
service. Generated expansion output is never an editable document.

## Typed rewrites

The source model is a lossless S-expression CST plus a typed AST. No public API
exposes a generic syntax tree editor, structural query language, arbitrary tree
patterns, mapping/key paths, JSON Patch, or runtime source mutation.

Trusted compiler tooling may use an internal rewrite planner:

```text
WorkspaceSnapshot {
  documents: DocumentId -> { revision, CST }
}

RewritePlan {
  operation,
  documents: [{ id, expected-revision }],
  edits: [TypedEdit]
}

TypedEdit {
  document,
  target: AstId,
  expected-fingerprint,
  expected-kind,
  replacement: ParsedFragment
}
```

The initial operations are semantic symbol rename and compiler/linter-provided
fixes. A plan resolves typed symbol identities, rejects targets that exist only
in generated macro output, creates non-overlapping source byte edits, applies
them in memory, reparses and retypechecks every affected module, and returns a
canonical diff plus diagnostics. Writing rechecks all document revisions and
node fingerprints and applies the complete workspace transaction atomically.
There is no partial application.

The public CLI is `vibra rewrite rename <path>:<byte> <new-name> [--write]`;
preview is the default. A future `vibra fix` may apply diagnostics already
produced by the compiler, but it must use the same planner. LSP
`textDocument/rename` and code actions use the planner and return ordinary LSP
workspace edits. Type applications, value calls, and macros with the same
textual head are distinguished by their resolved `AstId` and kind.

The existing generic code framework is removed:

- delete the `vibra code` pipeline command and its preview/write pipeline;
- delete generic `Form`, `Pattern`, `Query`, key/index `Path`, `Edit`,
  `ChangeSet`, mapping insert/upsert/rename, sequence splice, copy, and move
  APIs from `src/code`;
- delete `stdlib/src/code.vib`, all `vibra_code` host imports, and runtime
  handles for documents, nodes, queries, edits, workspaces, and syntax wrappers;
- delete `code-form.schema.json`, `code-path.schema.json`,
  `code-query.schema.json`, and `code-change-set.schema.json`;
- replace the YAML macro source schema with a JSON
  `macro-expansion.schema.json` for expansion inspection output;
- remove structural editor fields such as mapping keys, generic valid-child
  keys, and YAML macro names from public query responses.

The semantic index needed by compilation and LSP remains an internal compiler
service over the shared CST/AST. MCP receives no arbitrary write tool. If a
remote client needs rename, it may request a read-only rename plan; applying
filesystem changes remains an explicit CLI or LSP workspace action.

## Implementation and PR plan

Work lands on a temporary issue integration branch. Each PR is independently
reviewable but the integration branch is not released until the final removal
PR, because there is intentionally no dual-syntax supported state.

1. **Reader, CST, spans, and formatter.** Add lexer/parser error recovery,
   golden parser tests, formatter idempotence tests, typed AST identities, and
   the internal semantic index.
2. **Lowering and compiler.** Lower the new AST directly, migrate attributes,
   and remove semantic dependence on YAML mappings.
3. **Macros and origins.** Reimplement macro collection, typed quote/unquote/
   splice/capture, hygiene, limits, and the origin arena over the AST. Add
   source and JSON expansion inspection.
4. **Language corpus.** Mechanically migrate stdlib, language tests, examples,
   fixtures, templates, macros, and generated source. Add semantic parity and
   macro-origin tests.
5. **Projects and packages.** Migrate `project.vib`, introduce canonical JSON
   locks and package metadata, and update dependency, publish, and sync flows.
6. **LSP and rewrites.** Move LSP reads to the compiler snapshot, expose origin
   diagnostics and `expansionAt`, then add the typed rename/fix planner, CLI
   rename preview/write, and LSP rename.
7. **Generic editor removal.** Delete the `vibra code` command, generic
   `src/code` query/edit machinery, `stdlib/src/code.vib`, `vibra_code` host
   ABI, editor schemas, YAML pipeline tests, and editor documentation. Add no
   arbitrary MCP write replacement.
8. **Output and embedding.** Make JSON/human/raw/SARIF exhaustive, remove YAML
   report paths, isolate YAML to the external `embed` data decoder, and update
   container/distribution scripts.
9. **Removal and documentation.** Delete the YAML source validator/parser/
   formatter, `.vib.yaml` discovery, YAML dependencies that are no longer
   used, legacy diagnostics, DRAFT YAML design, README examples, and
   compatibility tests.

PRs must not add a hidden legacy feature flag. A one-shot external migration
utility may be published separately, but it is not invoked by Vibra and is not
part of the supported compiler.

## Acceptance criteria

- Every `.vib` source, stdlib module, test, example, and template uses the
  grammar in this document.
- The parser rejects YAML source and `.vib.yaml` discovery is absent.
- Positional and labelled calls, `types:`, atoms, strings, escapes, comments,
  definitions, visibility, types, expressions, patterns, attributes, and tests
  have focused positive and negative parser tests.
- Parser and diagnostic tests verify exact half-open byte spans, including
  Unicode and recovery after malformed forms.
- `vibra fmt --write` is idempotent across the full source corpus.
- `project.vib` is S-expression based; locks and package/release metadata are
  canonical JSON; legacy YAML inputs produce targeted failures.
- No compiler command advertises or accepts YAML output. JSON, human, raw, and
  SARIF behavior is covered where applicable.
- Embedded YAML has focused safe-data conversion tests and cannot be consumed
  as source, a manifest, a compiler directive, or an output serialization;
  text, binary, JSON, TOML, and XML retain focused tests.
- Macro tests cover category/arity errors, imported definition-context
  resolution, explicit capture, binder hygiene, legal and illegal splicing,
  expansion limits, Unicode spans, origin chains, and related diagnostics.
- Expansion inspection reports contextual AST kinds, so a direct generic type
  application cannot be confused with a value call or macro invocation.
- Rewrite tests cover stale revisions and fingerprints, overlapping edits,
  atomic rollback, retypecheck failure, generated-only target rejection, and
  cross-module semantic rename.
- `vibra code`, generic structural patterns and edits, runtime code handles,
  `stdlib/src/code.vib`, its host ABI, and the four generic code schemas are
  absent. No MCP tool provides arbitrary source writes.
- LSP formatting, hover, definition, references, completion, rename, code
  actions, origin-aware diagnostics, and `vibra/expansionAt` use the shared
  compiler CST/AST service.
- `rg` finds no user-facing claim that Vibra source or CLI output is YAML-first,
  no `.vib.yaml` support, and no `E-YAML-*` diagnostic.
- `yaml-edit` is absent from production dependencies. A YAML data library may
  remain solely behind the external `embed` decoder; production uses elsewhere
  fail this criterion.
- README, DRAFT, schemas, test guidance, skills, container examples, and release
  documentation agree with this contract.
- Both required suites pass: `cargo test` and `cargo run -- test`.
- The release notes prominently identify the change as non-backward-compatible
  and provide the mechanical migration table, without promising automatic
  compatibility.

## Amendment (2026-07-27): internal typed-AST-to-`Value` migration bridge

This contract's single-direction rule was written against an *external*
migration utility (`tools/corpus-migrator`, text-to-text, never invoked by
Vibra). It did not anticipate an *internal* bridge inside the compiler crate
itself, so this amendment records that exception explicitly rather than
leaving it implied.

`src/surface_adapter.rs` (issue #150) converts the typed S-expression surface
AST (`crate::ast`) directly into the internal compatibility `Value` shape `src/load.rs`
and `src/lower.rs` already consume, so S-expression source can reach the
existing, proven lowering semantics without re-deriving them on the typed
path (`typed_lower`/`typed_body`/`typed_program`), whose measured readiness
(`materialized-valid`) trails `src/lower.rs`'s coverage by an order of
magnitude. This is permitted under the following constraints, all enforced
by the adapter's own structure:

- **Internal and private.** The module is `pub(crate)`, exported to no
  external crate or CLI surface, and documented in its own header as
  temporary.
- **Single direction only.** AST -> `Value`. There is no `Value` -> AST
  function anywhere in the adapter, not even for tests; the reverse
  direction remains exclusively `src/ast/surface.rs`'s parser.
- **Not a dual-syntax state.** The adapter does not parse YAML, does not
  select a parser from file contents, and does not change what `vibra`
  accepts as source. Until a follow-up PR repoints `src/load.rs`, the
  compiler's only source parser remains the typed S-expression reader; the
  adapter is an internal lowering seam.
- **Not a supported interface.** It carries no stability guarantee, is not
  documented as a public API, and is not the intended long-term shape of the
  compiler: it is a bridge to be deleted, not a permanent third pillar
  alongside the legacy and typed lowering paths.
- **Temporary.** It is slated for removal as the typed path's own coverage
  reaches parity with `src/lower.rs`; the adapter remains slated for deletion
  with that legacy map-based reader.
- **Fails closed.** Every construct the adapter cannot map to a precise
  legacy shape produces a specific, path-qualified `E-ADAPT-*` error rather
  than a best-effort guess, because a silently wrong `Value` would be a
  miscompilation, not merely an unsupported-feature diagnostic.

This does not relax the reader contract, the removal of the legacy YAML
*parser* at cutover, or the prohibition on a second source parser: the
adapter never parses anything. It only widens, for the migration window
between the typed frontend landing and its coverage reaching parity, what
"single-direction" is understood to permit inside the compiler.
