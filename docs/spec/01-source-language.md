# Vibra v1 source language

Status: normative target
Implementation status: not started

## Reader

A `.vib` file is UTF-8 and contains zero or more forms. Parentheses delimit
lists; whitespace separates tokens; `;` starts a line comment. There are no
reader macros, quote syntax, commas, brackets, braces, raw strings, heredocs,
or interpolation.

```ebnf
module         = trivia, { top-form, trivia }, EOF ;
form           = atom | discard | list ;
list           = "(", trivia, symbol, { required-trivia, form }, trivia, ")" ;
atom           = string | boolean | integer | float | unit | atom-name
               | label | symbol ;
literal        = string | boolean | integer | float | unit | atom-name ;
boolean        = "true" | "false" ;
unit           = "unit" ;
symbol         = kebab-name, { ".", kebab-name } ;
atom-name      = "@", symbol ;
label          = symbol, ":" ;
discard        = "-" | "@-" | "-:" ;
kebab-name     = lowercase-letter,
                 { lowercase-letter | digit | "-" } ;
lowercase-letter = "a" | "b" | "c" | "d" | "e" | "f" | "g"
                 | "h" | "i" | "j" | "k" | "l" | "m" | "n"
                 | "o" | "p" | "q" | "r" | "s" | "t" | "u"
                 | "v" | "w" | "x" | "y" | "z" ;
digit          = "0" | "1" | "2" | "3" | "4"
               | "5" | "6" | "7" | "8" | "9" ;
trivia         = { whitespace | line-comment } ;
required-trivia = ( whitespace | line-comment ), trivia ;
```

Strings are double quoted and support `\"`, `\\`, `\n`, `\r`, `\t`, and
`\u{HEX}`. Integers are signed decimal. A float contains a decimal point or an
exponent. Numeric range belongs to type checking, not lexing.

`unit` is the single value of `void`. It is the result of a function or form
that completes successfully but has no meaningful value to return. Empty `do`
and an empty function body evaluate to `unit`.

Every named symbol consists of one or more dot-separated segments, and every
segment independently satisfies `kebab-name`. Its first character MUST be a
lowercase ASCII letter; every later character is a lowercase ASCII letter,
digit, or hyphen. Consecutive and trailing hyphens are therefore valid, while a
leading hyphen is not. `?`, `!`, `_`, `/`, an empty segment, and a segment that
starts with a digit are invalid in a symbol. A dotted symbol is a qualified
name, never field-access syntax. For example, `a`, `a1`, `a-`, `a--b`, and
`a.b2-c` are symbols; `1a`, `-a`, `a..b`, and `a?` are not.

Labels and atom names derive mechanically from symbols: `some.name:` is a
label and `@some.name` is an atom name. `-`, `@-`, and `-:` are the three exact
discard spellings. They cannot be namespaced or extended: `-.name`, `@-.name`,
and `-:.name` are invalid. All three are semantically equivalent to Rust's `_`:
in a binder or wildcard position they create no identity, may repeat, and do
not participate in redeclaration or shadowing checks. A discard is not a value,
label, atom name, or name reference and is rejected in every non-discard
position.

Keywords, booleans, `unit`, and atom names cannot be rebound.
Boolean, `unit`, and reserved-form recognition takes precedence when their
spelling also satisfies the symbol production.

An atom is an ordinary value in expression position. Only a grammar or data
schema position that explicitly expects an entity reference resolves it as
one. Imports and effect rows are such positions; writing `@std.io` elsewhere
produces an atom value rather than loading or invoking code.

## Labels and calls

A label consumes one following form. Required ordered input is positional;
optional or named input is labelled. A call may contain fixed positional,
labelled, and variadic operands.

The recovery parser accepts labelled operands interleaved with a variadic tail
when every operand can be bound unambiguously to the resolved signature. The
canonical formatter emits fixed positional operands first, labelled operands in
declaration order, and the variadic tail last. Ambiguous or duplicate binding
is an error. Noncanonical but unambiguous order is a style diagnostic, not a
compile error.

Call arguments are evaluated in resolved fixed-parameter order, then labelled
parameter declaration order, then variadic source order. Evaluation therefore
does not change when the formatter normalizes operand order.

```vibra
(log.write "build finished" level: @info "target" "app")
```

Every executable list not reserved by the grammar is a call. A function value
is `f`; a nullary call is `(f)`.

An array variadic parameter receives every remaining unlabelled form as one
array. A map variadic parameter receives alternating key and value forms. The
call MUST contain an even number of remaining forms, and construction follows
the ordinary map-literal rules.

```vibra
(collect-values v1 v2 v3)
(collect-fields k1 v1 k2 v2 k3 v3)
```

## Declarations

V1 has exactly these user top-level forms:

```ebnf
top-form = import | deftype | defint | deffect | def | defn | test ;
import   = "(", "import", symbol, atom-name, ")" ;
deftype  = "(", "deftype", symbol, type-expr,
           { type-attribute | nested-method }, ")" ;
defint   = "(", "defint", symbol,
           { declaration-attribute | interface-member
           | interface-implementation }, ")" ;
deffect  = "(", "deffect", symbol,
           { declaration-attribute | effect-member }, ")" ;
def      = "(", "def", symbol, type-expr, expr,
           { declaration-attribute }, ")" ;
defn     = "(", "defn", symbol, parameters, type-expr,
           { function-attribute }, { expr }, ")" ;
test     = "(", "test", string, [ "effects:", effect-row ], expr+, ")" ;

nested-method = "(", "defn", symbol, parameters, type-expr,
                { function-attribute }, { expr }, ")" ;
interface-member = "(", "defn", symbol, parameters, type-expr,
                   { function-attribute }, ")" ;
interface-implementation = "(", "impl", type-expr,
                           implementation-member+, ")" ;
implementation-member = "(", "defn", symbol, parameters, type-expr,
                        { function-attribute }, { expr }, ")" ;
effect-member = "(", "defn", symbol, parameters, type-expr,
                { function-attribute }, { expr }, ")" ;

binding-name         = symbol | discard ;
parameters           = "(", { binding-name, type-expr }, ")" ;
labelled-parameters  = "(", { symbol, type-expr, literal }, ")" ;
variadic-parameter   = "(", binding-name, variadic-type, ")" ;
variadic-type        = "(", "array", type-expr, ")"
                     | "(", "map", type-expr, type-expr, ")" ;
effect-row            = "(", { atom-name }, ")" ;
where-clause         = "(", { symbol, generic-bound }, ")" ;
generic-bound        = "any" | symbol ;

declaration-attribute = "visibility:", atom-name | "doc:", string ;
type-attribute = "where:", where-clause
               | "implements:", "(", { symbol }, ")"
               | declaration-attribute ;
function-attribute = "where:", where-clause
                   | "labelled:", labelled-parameters
                   | "variadic:", variadic-parameter
                   | "visibility:", atom-name
                   | "effects:", effect-row
                   | "external:", atom-name
                   | "symbol:", string
                   | "doc:", string ;
lambda-attribute = "labelled:", labelled-parameters
                 | "variadic:", variadic-parameter
                 | "effects:", effect-row ;
```

`def` introduces an immutable module value. There is no separate `const` form
in v1.

Every required positional parameter is written directly as a name/type pair;
there is no wrapper list around each parameter. `labelled:` is one flat list of
name/type/default triples. Every labelled parameter MUST have a literal default
value. `variadic:` contains exactly one name/type pair, and its type MUST be an
`array` or `map`. A function has at most one variadic parameter. A positional
or variadic parameter may use any discard spelling when its value is
intentionally unused; labelled parameters require a real symbol because their
name is part of the call contract.

`where:` is a flat list of generic-name/bound pairs. It is the only declaration
of generic names: every generic name used by a declaration MUST occur exactly
once in its `where:` clause. `any` states that no interface bound is required.

The canonical function-attribute order is `where:`, `labelled:`, `variadic:`,
`visibility:`, `effects:`, `external:`, `symbol:`, then `doc:`. The canonical
type-attribute order is `where:`, `implements:`, `visibility:`, then `doc:`.
Attributes may be parsed in any unambiguous order, occur at most once, and are
formatted canonically. Nested methods follow attributes.

Every labelled argument or attribute follows all fixed positional forms of its
enclosing form and precedes its variadic body or member forms. The parser MAY
recover another unambiguous order for ordinary calls as described above, but
the declaration grammar and formatter never use a label to interrupt a fixed
header.

All declarations are private unless they contain `visibility: @public`.
Visibility is part of the declaration, not a wrapper form.

```vibra
(import io @std.io)

(deftype user-id (newtype uint64)
  visibility: @public)

(def default-retries uint8 3)

(defn greet (name str) str
  visibility: @public
  (text.concat "hello, " name))

(deftype option
  (enum (some t) (none))
  where: (t any)
  visibility: @public)
```

The following declaration has labelled defaults and an array variadic tail:

```vibra
(defn write-log (message str) void
  labelled: (level atom @info)
  variadic: (fields (array (tuple str str)))
  visibility: @public
  effects: (@std.io.stdout)
  (log.write message level fields))
```

Omitted `effects:` always means `effects: ()`. This default is identical for
every `defn`, `lambda`, function type, and test; omission never requests effect
inference. The checker computes the effects performed by a body and requires
them to fit within its written or default-empty ceiling. An ordinary effectful
`defn`, `lambda`, or test MUST therefore write `effects:` explicitly. For a
`deffect` member, the owner root remains implicit and `effects: ()` means that
the operation performs no additional roots.

`deftype`, `defint`, and `deffect` are native AST forms. They MUST NOT be
parser desugarings into a generic definition node. A nested `impl` is likewise
a native child of its `defint`, not a generic call or a method-list desugaring.

## External definitions

Vibra code calls external behavior through ordinary typed functions. A
toolchain-owned declaration binds a signature to one of exactly two providers.
`external:` and `symbol:` MUST occur together, and such a declaration MUST omit
its body:

```vibra
(defn add-checked (left i32 right i32) (result i32 overflow)
  visibility: @public
  external: @compiler
  symbol: "integer.add-checked")
```

`@compiler` selects a pure compiler intrinsic. Such a declaration MUST have an
empty effect ceiling. `@host` selects a typed host operation and is valid only
on a member of the `deffect` that owns the operation's nominal effect root. A
`@host` external member MUST have no additive effects. An interface contract
cannot be external.

The provider and string symbol are checked against closed, versioned toolchain
registries. External declarations are accepted only in toolchain-signed
standard-library modules; ordinary packages cannot declare them. Every call
uses the declared Vibra name and ordinary call syntax—there is no `external`,
`primitive`, or `host-op` call expression. V1 has no WebAssembly external
provider and no source-level Wasm FFI.

## Types, interfaces, and methods

An interface contract method is qualified by its interface name. A `deftype`
lists every interface it implements in `implements:` and defines those methods
with the interface-qualified name. A regular method nested in a `deftype` uses
the type-qualified name.

```vibra
(defint printable
  visibility: @public
  (defn printable.render (value self) str))

(deftype user
  (record (name str) (id user-id))
  implements: (printable)
  visibility: @public
  (defn printable.render (value self) str
    (field value name))
  (defn user.name-length (value self) uint64
    (text.length (field value name))))
```

The package that owns an interface may implement it for a type declared in
another package by placing an `impl` block in the owning `defint`:

```vibra
(defint printable
  (defn printable.render (value self) str)
  (impl i32
    (defn printable.render (value self) str
      (integer.to-str value))))
```

`impl` is valid only as a direct child of the `defint` that owns the interface;
there is no top-level `impl` and no `for:` implementation attribute. A contract
member has no body. A method implementing an interface has a body and MUST
match its contract after substituting the enclosing `deftype` or `impl` target
for `self`. The type chapter defines completeness, conflict, and ownership
rules.

Qualified method definitions occur only inside their owning `deftype`,
`defint`, nested `impl`, or `deffect`. A module-level `defn` name MUST be
unqualified.

Records are constructed by calling their nominal type with labelled fields.
Enum tags are constructors qualified by the type name. `field` reads a
statically known record field.

```vibra
(user name: "Ada" id: (user-id 1))
(option.some "value")
(field person name)
```

## Functions and expressions

A named function declares a name, a flat parameter list, and a result type.
Function bodies are direct expression sequences; their final expression is the
result. `lambda` declares an anonymous function with the same parameter,
result, labelled, variadic, and effect syntax, but no name or visibility.
`fn` is reserved for function types and is never an anonymous declaration.

```vibra
(lambda (value i32) i32
  (integer.increment value))
```

```ebnf
expr = atom | call | lambda
     | "(", "do", { expr }, ")"
     | "(", "let", binding-name, expr, { expr }, ")"
     | "(", "if", expr, expr, expr, ")"
     | "(", "match", expr, pattern, expr, { pattern, expr }, ")"
     | "(", "while", expr, expr, ")"
     | "(", "for", binding-name, expr, expr, ")"
     | "(", "break", ")" | "(", "continue", ")"
     | "(", "return", [ expr ], ")"
     | "(", "try", expr, ")"
     | "(", "field", expr, symbol, ")"
     | collection ;
lambda = "(", "lambda", parameters, type-expr,
         { lambda-attribute }, { expr }, ")" ;
collection = "(", "tuple", { expr }, ")"
           | "(", "array", { expr }, ")"
           | "(", "map", { expr, expr }, ")" ;
pattern = literal | discard | "(", "bind", binding-name, ")"
        | "(", symbol, { pattern | label, pattern }, ")"
        | "(", "tuple", { pattern }, ")"
        | "(", "array", { pattern }, ")" ;
```

`let` introduces an immutable name for the remainder of its form. There is no
assignment or shared mutable state in v1. A binder MUST NOT shadow any visible
name. A discard never binds and may be used as `(let - expression)`,
`(let @- expression)`, or `(let -: expression)` to state that a result is
intentionally ignored. Repeating a discard is always valid because it creates
no declaration to redeclare or shadow.

`while` and `for` exist for local iteration; mutation needed by their library
implementation is compiler/runtime internal. Collections have immutable value
semantics. A `map` contains alternating key and value expressions directly;
there is no `entry` wrapper.

`match` contains alternating pattern/result forms directly; there is no `case`
wrapper. Every arm has exactly one result expression, so `do` groups multiple
expressions. Arms are checked for reachability and exhaustiveness.

```vibra
(match value
  (option.some (bind text)) (text.length text)
  (option.none) 0)
```

`try` propagates the error of `result` or the absence of `option` through a
function returning the same container and error type. There are no exceptions
or implicit error conversions. A fallible value MUST be matched, propagated,
returned, bound for later use, or explicitly ignored with a discard binding.

## Canonical format

`vibra fmt` defines the only canonical representation:

- UTF-8, LF endings, and one trailing newline;
- two-space indentation and no trailing whitespace;
- one blank line between top-level forms;
- leaf lists on one line when they fit within 88 columns;
- declaration headers before labelled attributes and bodies;
- fixed, labelled, then variadic call operands;
- one pattern/result arm per line in a multiline `match`; and
- preserved comments attached to the following form when possible.

Formatting MUST be idempotent and semantics-preserving. The formatter MAY
normalize recoverable presentation but MUST NOT guess through a syntax,
binding, or type ambiguity.
