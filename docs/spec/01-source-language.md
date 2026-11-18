# Vibra source language

Status: normative target
Implementation status: none of this chapter is implemented. The shipped
v0.0.1 toolchain implements the previous S-expression surface, which this
chapter replaces. The [release roadmap](../roadmap/README.md) schedules the
surface defined here. Only a small subset of it (declaration keywords, typed
closers, application, `if`, `match`, `let`, `return`) has been exercised in a
model-writing pilot; nothing here is claimed to be measured beyond that.

## Design of the surface

Every construct opens with a keyword and, when it contains other constructs,
ends with a closer named after its opener: `fn … endfn`, `if … endif`. Inside
a declaration, a **section** is a keyword followed by its operand: `takes`,
`returns`, `do`. Four bracket pairs have one job each:

| Token | Only use |
| --- | --- |
| `( )` | Application: `f(a b)`, `Cents(5i64)`, `List(T)` |
| `{ }` | Product shapes: tuples, records, parameter vectors, effect rows |
| `[ ]` | Collections: lists and dicts |
| `:` | Joins the two halves of one pair: `name: Type`, `name: value` |

There are no commas, no semicolons, no infix or prefix operators, no `=`, and
no parentheses used for grouping. Whitespace separates tokens and is otherwise
insignificant: line breaks and indentation never change meaning.

## Lexical structure

A `.vib` file is UTF-8. A byte-order mark, an invalid UTF-8 sequence, or a NUL
is `@syntax.invalidEncoding`. Whitespace is space, tab, CR, and LF.
`//` starts a comment that runs to the end of the line. There is no block
comment.

```ebnf
identifier  = letter, { letter | digit } ;
letter      = "A"…"Z" | "a"…"z" ;
digit       = "0"…"9" ;
atom        = "@", atom-part, { ".", atom-part } ;
atom-part   = letter, { letter | digit } ;
discard     = "_" ;
void        = "void" ;
integer     = [ "-" ], digits, [ integer-suffix ] ;
float       = [ "-" ],
              ( digits, ".", digits, [ exponent ], [ float-suffix ]
              | digits, exponent, [ float-suffix ]
              | digits, float-suffix ) ;
integer-suffix = "i8" | "i16" | "i32" | "i64"
               | "u8" | "u16" | "u32" | "u64" ;
float-suffix   = "f32" | "f64" ;
exponent    = ( "e" | "E" ), [ "+" | "-" ], digits ;
digits      = digit, { digit } ;
string      = '"', { string-char | escape }, '"' ;
doc-string  = '"""', ? any text not containing '"""' ?, '"""' ;
character   = "'", ( char-scalar | escape | "\'" ), "'" ;
escape      = "\\" | '\"' | "\n" | "\r" | "\t"
            | "\u{", hex, { hex }, "}" ;
```

### Identifiers

An identifier starts with an ASCII letter and continues with ASCII letters and
digits. Hyphens, underscores, `?`, and `!` are not identifier characters. An
identifier that fails this production is `@syntax.invalidName` over the whole
token.

Two adjacent tokens of the classes identifier, reserved word, number, atom,
string, character, and doc string MUST be separated by whitespace, a bracket,
or `:`. Two such tokens with nothing between them, as in `"a"b` or
`1u8"x"`, are `@syntax.missingSeparator` at a zero-width span where the
second token starts; both tokens are kept. `-` directly before a digit is
part of the numeric token and never needs separation.

Casing is a convention, never a parse rule. Types, interfaces, and type
parameters are written in PascalCase; every other name, including a variant
case, is written in camelCase. `vibra check` reports a name that departs from
the convention as the style diagnostic `@style.casing`; the program's
meaning does not change. No rule in any chapter depends on the case of a name.

### Reserved words

The following words are reserved. None can be declared or bound, and each is
recognized before the identifier production.

```text
as await be case do doc else endfn enddo endif endimpl endinterface endlet
endmatch endproc endtest endtype enduse ensures external fn given if impl
interface labelled let match native of performs proc representation
requires return returns role symbol takes test then try type use visibility
void where
```

A reserved word where the grammar requires an identifier is recovered: the
parser takes the word as the identifier and reports it. In a declaration or
binder position (a declaration name, a type parameter, a pattern binder, an
import name) the report is `@name.reservedDeclaration` at the word. In a
label position (a labelled parameter, a record or case field, a labelled
argument, a record literal or pattern field, a type-application label) it is
`@name.reservedLabel` at the word. The recovered name still binds or labels
for the rest of its scope so that the error does not cascade.

The names of the prelude (the [type chapter](02-type-system.md#the-prelude)
lists them, for example `Bool`, `Option`, `true`, `false`) are ordinary
identifiers to the lexer and are reserved by resolution. A type, interface,
type parameter, or pattern binder spelled as one is
`@name.reservedDeclaration` at the name; a module value, `fn`, `proc`, or
import spelled as one is `@name.reservedValueSpelling` at the name. Either
still binds for the rest of its scope so that the error does not cascade.

### Atoms

An atom is `@` followed by one or more dot-separated parts. A part starts with
an ASCII letter and continues with ASCII letters and digits. Atoms are
written in camelCase like every other value name, the registered tags of the
toolchain included: `@runtime.invalidHostValue`, `@auditTrace.v1`.

An atom is an ordinary value. What an atom means depends on the grammar
position it is written in:

- The locator of an `use` is an entity reference. Its path is matched
  against actual entity names with exact case.
- The operand of a `visibility`, `representation`, `role`, or `external`
  section is a tag. It MUST be a member of that section's closed tag set,
  and it names no declaration.
- Anywhere else an atom is a value. `@std.io` in an expression loads and
  calls nothing.

Effect rows do not use atoms; they contain names resolved through the
module's imports.

### Literals

Integers are signed decimal. A float contains a decimal point, an exponent, or
a float suffix. An adjacent suffix is part of the token and fixes the
literal's exact type: `1u8`, `25i32`, `2.5f64`, `2f64`. An unsuffixed literal
takes its type from its expression context. An unknown suffix, an integer
suffix on a float body, or trailing token text is
`@syntax.invalidNumericLiteral`. Range is checked by the type checker, not
the lexer: `-1u8` is one `U8` literal and is `@type.numericOutOfRange` at
the literal, as is an unsuffixed literal that does not fit its expected
type. There are no numeric separators and no non-decimal bases. `-`
immediately followed by a digit starts a numeric token; `-` has no other
use.

A string is double quoted and supports `\"`, `\\`, `\n`, `\r`, `\t`, and
`\u{HEX}` with one to six hexadecimal digits naming one Unicode scalar; a
surrogate or a value above U+10FFFF is rejected. A terminated string with an
unsupported or malformed escape is `@syntax.invalidStringLiteral` over the
whole token. A string that reaches the end of the file unterminated is
`@syntax.unmatchedDelimiter`, is recovered, and keeps its bytes; it does not
also report the first diagnostic. A string may span lines; its interior bytes,
including CR and CRLF, are kept as written. There is no interpolation and no
raw string.

A doc string is delimited by `"""`. It processes no escapes. If the text after
the opening delimiter starts with a line break, that break is removed; the
line break and indentation before the closing delimiter are removed; and the
longest whitespace prefix common to all non-blank lines is removed from each
line. A doc string is valid only as the operand of a `doc` section; elsewhere
it is `@syntax.invalidForm`.

A character literal is single quoted and denotes exactly one Unicode scalar:
one scalar written directly, or one string escape, or `\'`. An empty literal,
one holding more than one scalar, or a surrogate is
`@syntax.invalidCharacterLiteral`.

`void` is the single value of the type `Void`. `_` is the discard: in a binder
or pattern position it matches anything and binds nothing, it may repeat, and
it is rejected in every other position.

There is no boolean literal. `true` and `false` are the two public values the
standard `Bool` type's module declares, and they belong to the prelude.

## Grammar

```ebnf
module      = { top }, EOF ;
top         = use | type-decl | interface-decl | fn-decl | proc-decl
            | module-let | test ;

use         = "use", atom, [ "as", identifier ], "enduse" ;

type-decl   = "type", identifier, [ type-params ],
              ( "of", type-body | case, { case } ),
              { type-item }, "endtype" ;
type-body   = "Intrinsic", "(", atom, ")" | type-expr ;
case        = "case", identifier, [ record-type ] ;
type-item   = given | where | role | visibility | representation | doc
            | fn-decl | proc-decl | impl ;

interface-decl = "interface", identifier, [ type-params ],
                 { given | role | visibility | doc | assoc-type | fn-decl
                 | proc-decl | impl },
                 "endinterface" ;
assoc-type  = "type", identifier, "endtype" ;
impl        = "impl", type-expr, { assoc-choice | fn-decl | proc-decl },
              "endimpl" ;
assoc-choice = "type", identifier, "of", type-expr, "endtype" ;

fn-decl     = "fn", identifier, [ type-params ], signature,
              { given | requires | ensures | visibility | external
              | symbol | native | doc },
              [ "do", body ], "endfn" ;
proc-decl   = "proc", identifier, [ type-params ], signature,
              { given | requires | ensures | performs | visibility
              | external | symbol | doc },
              [ "do", body ], "endproc" ;
signature   = [ "takes", params ], [ "labelled", labelled ],
              "returns", identifier, ":", type-expr ;
params      = "{", { pattern, ":", type-expr }, "}" ;
labelled    = "{", { identifier, ":", type-expr, "be", expr }, "}" ;
type-params = "(", identifier, { identifier }, ")" ;

module-let  = "let", identifier, ":", type-expr, { visibility | doc },
              "be", expr, "endlet" ;
test        = "test", string, [ "takes", params ], [ performs ],
              "do", body, "endtest" ;

given       = "given", "{", { identifier, ":", type-expr }, "}" ;
requires    = "requires", expr, [ "else", expr ] ;
ensures     = "ensures", expr, [ "else", expr ] ;
where       = "where", expr, [ "else", expr ] ;
performs    = "performs", row ;
row         = "{", { path }, "}" ;
visibility  = "visibility", atom ;
representation = "representation", atom ;
role        = "role", atom ;
external    = "external", atom ;
symbol      = "symbol", string ;
native      = "native", string ;
doc         = "doc", doc-string ;

body        = { statement } ;
statement   = let | expr ;
let         = "let", pattern, [ ":", type-expr ], "be", expr,
              [ "else", body ], "endlet" ;

expr        = "return", expr | unary ;
unary       = "try", unary | "await", unary | postfix ;
postfix     = primary, { "(", { argument }, ")" | ".", identifier } ;
argument    = expr | identifier, ":", expr ;
primary     = literal | identifier | atom
            | "{", { expr }, "}"
            | "{", identifier, ":", expr, { identifier, ":", expr }, "}"
            | "[", { expr }, "]"
            | "[", expr, ":", expr, { expr, ":", expr }, "]"
            | "[", ":", "]"
            | if | match | do-block | anon-fn | anon-proc ;
literal     = string | character | integer | float | void ;
if          = "if", expr, "then", body, [ "else", body ], "endif" ;
match       = "match", expr, arm, { arm }, "endmatch" ;
arm         = "case", pattern, body ;
do-block    = "do", body, "enddo" ;
anon-fn     = "fn", [ type-params ], signature,
              { given | requires | ensures }, "do", body, "endfn" ;
anon-proc   = "proc", [ type-params ], signature,
              { given | requires | ensures | performs },
              "do", body, "endproc" ;

pattern     = discard | pattern-literal | path
            | path, "(", { pattern }, ")"
            | path, "(", identifier, ":", pattern,
                        { identifier, ":", pattern }, ")"
            | "{", { pattern }, "}"
            | "{", identifier, ":", pattern,
                   { identifier, ":", pattern }, "}"
            | "[", { pattern }, "]" ;
pattern-literal = string | character | integer | atom ;

type-expr   = path, [ "(", { type-expr }, { identifier, ":", type-expr }, ")" ]
            | atom
            | "{", { type-expr }, "}"
            | record-type
            | "Fn", "(", inputs, type-expr, ")"
            | "Proc", "(", inputs, type-expr, [ row ], ")"
            | "Intent", "(", type-expr, [ row ], ")" ;
record-type = "{", identifier, ":", type-expr,
                   { identifier, ":", type-expr }, "}" ;
inputs      = "{", { type-expr }, { identifier, ":", type-expr }, "}" ;
path        = identifier, { ".", identifier } ;
```

Notes on the grammar:

- `(` never starts an expression, a pattern, or a type. It always applies
  the form before it, with or without whitespace between them. Any
  expression may be a callee: `choose(c)(x)` calls the result of
  `choose(c)`. `f()` is a nullary call; `f` is the function value.
- `Fn`, `Proc`, `Intent`, and `Intrinsic` are **reserved type heads**. A
  `type-expr` or `type-body` that begins with one is parsed by that head's
  own production and never as an applied path, so `Fn({I32} Bool)` is a
  callable type even though its operands would also parse as type
  arguments. The heads name no declaration; declaring one is
  `@name.reservedDeclaration`.
- A constructor pattern is positional or labelled, never both. A pattern
  whose shape does not fit the constructor it names (labels for a wrapper or
  declared tuple, positional components for a declared record or a case with
  fields, a wrong arity, or an unknown or repeated field) is
  `@type.argumentMismatch` at the pattern.
- A statement never needs a terminator, because no statement can begin with
  `(`, `.`, or `:` and no expression can end with an open form.
- The `do` that follows a signature opens the declaration's body and is
  closed by the declaration's own closer. A `do` in expression position
  opens a block closed by `enddo`.
- In `Proc({Str} Void {DirWrite})` the row is the last operand and is
  optional. `Proc({Str} {DirWrite})` therefore declares a `proc` that
  returns the one-component tuple type `{DirWrite}` with an empty row; the
  checker reports `@type.suspiciousRow` when a result type is a tuple
  whose every component resolves to an operation interface.
- The grammar admits an empty brace or bracket form wherever a repetition
  can be empty: `{}` is the empty tuple (type, value, and pattern) and `[]`
  the empty list. An empty `takes {}`, `labelled {}`, `given {}`, or
  `performs {}` means the same as omitting the section; the formatter
  removes it.
- A form the grammar does not admit is `@syntax.invalidForm` over the
  smallest enclosing construct. An opener without its closer, or a closer
  that does not match the innermost open construct, is
  `@syntax.unmatchedDelimiter`.

The productions above write the sections of a declaration in their canonical
order, to be readable. The parser accepts them in any order, under three
constraints: the keyword, the name, and the type parameters come first; `do`
and its body come last; and the `case` alternatives of a variant type keep
their written order among themselves, which is the order of the variants.
So `fn f returns r: I32 takes {x: I32} do return x endfn` is accepted, and
the formatter moves `takes` before `returns`. Nested members and `impl`
blocks may be written between sections.

Every section occurs at most once, `requires` and `ensures` included; a
repeat is `@syntax.duplicateSection`. A required section that is missing,
such as `returns`, is `@syntax.invalidForm`. Several conditions are joined
with `Bool.and`. A section a declaration kind does not admit, such as
`performs` on a `fn`, is `@syntax.invalidSection`. The formatter writes
sections in the canonical order given under
[Canonical format](#canonical-format).

## Declarations

Vibra has exactly seven top-level forms: `use`, `type`, `interface`, `fn`,
`proc`, `let`, and `test`. There is no module declaration, no top-level
`impl`, and no local named declaration: a `type`, `interface`, named `fn`,
named `proc`, or `test` inside a body is `@syntax.invalidForm`.

All declarations of a module share one flat scope. A declaration may be
referenced before it is written. No declaration or binder may shadow a visible
name.

Every declaration is private to its module unless it contains
`visibility @public`. `@public` and `@private` are the only visibility values.

```vibra
use @std.io enduse

type UserId of U64
  visibility @public
endtype

let defaultRetries: U8 be 3u8 endlet

fn greet
  takes {name: Str}
  returns greeting: Str
  visibility @public
  do
    return Str.concat("hello, " name)
endfn
```

### `use`

`use @path enduse` imports one module or one public declaration of a module
under a local name. The name is the path's last part, or the identifier
after `as`: `use @billing.cents as money enduse`. The closer is always
written, as for every other declaration. The
[programs chapter](04-programs-and-packages.md#modules-and-imports) defines
resolution; because no module path part and no declaration name is a
reserved word, the default name is always a valid identifier, and `as` only
ever renames. Two imports that produce the same name, or an import whose
name is a declaration of the module, are `@name.redeclaration`. An import
whose name is a prelude name is `@name.reservedValueSpelling`. The
formatter and the import organizer keep a written `as`.

### `type`

`type Name of TypeExpr … endtype` declares a nominal type over one
representation: a wrapper over any other type, a declared tuple over a tuple
type, or a declared record over a record type. `type Name case … endtype`
declares a variant type; each `case` names one alternative and optionally
gives it named fields.

```vibra
type Celsius of F64 endtype

type User of {name: Str id: UserId}
  visibility @public
endtype

type Lookup(T)
  case found {value: T}
  case missing
  visibility @public
endtype
```

Type parameters are declared in the header. `given` gives a parameter its
bound, an interface written as in any type position; a parameter without an
entry has the bound `Any`. `where` states a type invariant and
`representation` controls who may construct and take apart the type; the
[type chapter](02-type-system.md#declared-types) defines all three. A `type`
may nest `fn` and `proc` declarations, which are its static members, and
`impl` blocks.

`of Intrinsic(@name)` and the `role` section are admissible only in the
embedded standard library.

### `interface`

An interface declares member signatures. A member without `do` is abstract; a
member with `do` is a default, and its body is checked like any other
callable body, so an empty one is `@type.missingReturn`. A nested
`type Name endtype`, with no body, declares an associated type, which each
implementation chooses; it is a member of the interface, and the
[type chapter](02-type-system.md#associated-types) defines it. An interface
whose members include a `proc` is an operation interface, which the
[effects chapter](03-effects.md) defines.

```vibra
interface Printable
  visibility @public
  fn render
    takes {value: Self}
    returns text: Str
  endfn
endinterface
```

### `impl`

`impl` is valid only as a direct child of the `type` that owns the type, where
its operand names the interface, or of the `interface` that owns the
interface, where its operand names the type. The parent supplies the other
half of the pair.

```vibra
type User of {name: Str id: UserId}
  fn nameLength
    takes {user: Self}
    returns length: U64
    do
      return Str.length(user.name)
  endfn
  impl Printable
    fn render
      takes {value: Self}
      returns text: Str
      do
        return value.name
    endfn
  endimpl
endtype
```

A callable member of an `impl` MUST have a body, or be an external
declaration, and MUST match its contract under the
[substitution](02-type-system.md#substitution) the type chapter defines. A
`type Name of T endtype` inside an `impl` chooses the associated type `Name`
and has no body. The operand of `impl` is positional: a label in it, as in
`impl Iter(Item: U8)`, is `@type.typeArgumentMismatch` at the label. The
type chapter defines completeness, overlap, and ownership.

### `fn` and `proc`

A `fn` is a pure function. A `proc` is a procedure that may perform
operations; its `performs` section is its closed effect ceiling, and an
omitted `performs` is the empty row. The ceiling is never inferred. The
effects chapter defines rows, plans, and `await`.

`takes` lists the required positional parameters as `Pattern: Type` entries;
a bare name is the simplest pattern. Every parameter pattern MUST be
irrefutable for its written type. The entries are positional: the names a
pattern binds are local to the callable and are not argument labels.

`labelled` lists optional parameters as `name: Type be default`. Each has a
real local name, which is part of the call contract, and a default that MUST
be a constant expression; a default that is not is `@type.notConstant`. The
type chapter defines constant expressions.

`returns name: Type` declares the result type and a result name. The result
name is visible only in the `ensures` section of the same declaration, in both
the predicate and its `else` consequence. It is not visible in `requires` or
in the body.

`requires`, `ensures`, and `where` are **guards**. The
[type chapter](02-type-system.md#guards) defines them: a guard without `else`
is an obligation the checker MUST discharge statically, and a guard with
`else` states the value produced when the guard is false. No guard traps.

```vibra
fn splitCents
  takes {total: Cents parts: I32}
  labelled {roundUp: Bool be false}
  returns shares: Result(List(Cents) SplitError)
  requires Ordered.greaterOrEqual(parts 1i32)
    else Result.err(error: SplitError.noParts())
  do
    return Result.ok(value: divide(total parts roundUp: roundUp))
endfn
```

An anonymous `fn` or `proc` is an expression with the same signature and
guard sections and no name, visibility, or doc. It cannot refer to itself.

```vibra
fn takes {value: I32} returns next: I32 do
  return I32.add(value 1i32)
endfn
```

There are no variadic parameters. A collection literal or a collection
parameter takes their place.

### Module values

A top-level `let name: Type be expr endlet` declares an immutable module
value. Its name is one identifier, not a pattern; its type is mandatory; it
admits no `else`. Its initializer is pure. `visibility` and `doc` sit between
the type and `be`. The [runtime chapter](06-runtime.md) defines when the
initializer runs. There is no separate constant form.

### `test`

`test "name" takes {world: TestWorld} performs {…} do … endtest` declares a
test. `takes` is optional and admits exactly one parameter, of the standard
type `TestWorld`; any other parameter list is `@syntax.invalidForm`. Its
body has type `Void`, MUST NOT contain a `return` or `try` outside an
anonymous callable, and may `await`. The
[programs chapter](04-programs-and-packages.md#tests) defines discovery and
the test world.

### External definitions

A toolchain-owned declaration binds a signature to a provider. `external` and
`symbol` MUST occur together, and such a declaration MUST omit its body. The
excerpt below is the `addChecked` member of the `I32` declaration in the
embedded `@std.builtin` module, whose
[intrinsic registry](06-runtime.md#compiler-intrinsic-registry) row fixes
the symbol, the owner, and the signature:

```vibra
type I32 of Intrinsic(@i32)
  visibility @public
  fn addChecked
    takes {left: Self right: Self}
    returns sum: Result(Self ArithmeticError)
    visibility @public
    external @compiler
    symbol "i32.add-checked"
  endfn
endtype
```

`@compiler` selects a pure compiler intrinsic and is valid only on a `fn`.
`@host` selects a typed host operation and is valid only on a `proc` member
of the operation interface that owns that operation; such a member MUST NOT
write `performs`. `native` names a toolchain native implementation of a `fn`
that keeps its Vibra body. Symbol strings are registry keys and keep their
registered spelling.

External declarations, `native`, `role`, and `Intrinsic` are accepted only in
the embedded standard-library modules; elsewhere each is
`@syntax.invalidSection`. The operand of `visibility`, `representation`,
`role`, and `external` is one atom of that section's closed tag set; any
other atom is `@syntax.invalidForm` over the section. A `@host` symbol whose
registry owner is not the enclosing interface is `@external.ownerMismatch`.
An external function is applied through its Vibra name with ordinary
application; there is no foreign-call expression and no source-level foreign
function interface.

## Names and paths

A declaration name is always one identifier in its owner's scope. A reference
is a path through owners: `User.nameLength`, `Printable.render`,
`Option.some`, `io.Stdout`. A call names its owner; there is no method-call
sugar, so `user.nameLength()` is not a call of `User.nameLength`.

A dotted name is resolved from the left. If its first identifier is a local
binding or a module value, each later `.name` reads a field of a record or
declared record. Otherwise the longest prefix that names an entity is a path,
and any remaining `.name` parts read fields of its value. Because nothing
shadows, a name is never both. A field read of a value without that field is
`@type.unknownRecordField`.

A module-level `fn` name or a nested member path in expression position
denotes a function value. Constructors, field reads, and lookups are not
function values; an anonymous `fn` wraps one where a function value is
needed.

## Application

`callee(arguments)` applies a callee to its arguments. The checker classifies
the application from the statically resolved callee, and the
[type chapter](02-type-system.md#application) defines the closed set of
applicable kinds: a function or procedure, a constructor, and a collection
lookup. There is no user-defined call operator.

Positional arguments bind to required parameters in order. A labelled argument
`name: expr` binds to the labelled parameter, record field, or case field of
that name. A missing, duplicate, or unknown binding is an error. Labelled
arguments may be written in any position among the positional ones.

The callee is evaluated once, first. Then every written argument is evaluated
in written order, labelled arguments included. Binding an argument to its slot
never changes that order, and the formatter never reorders arguments.

Construction uses labels exactly when the shape has field names:

```vibra
User(name: "Ada" id: UserId(1u64))   // declared record: labels
Option.some(value: "x")              // case with fields: labels
Option.none()                        // case without fields
Celsius(21.5f64)                     // wrapper: positional
Pair(1i32 "x")                       // declared tuple: positional
```

Lookup is application of a collection to a key and returns an `Option`:
`items(0u64)`, `table(key)`, `text(0u64)`.

## Anonymous products and collections

```vibra
{"Ada" 42u64}                 // tuple value
{name: "Ada" id: 42u64}       // record value
[1i32 2i32 3i32]              // list
["name": "Ada" "role": "dev"] // dict
[:]                           // empty dict
```

Tuple and record values may hold values of different types. Every element of
a list has one exact type; the keys of a dict share one exact type and so do
its values. `[]` and `[:]` need an expected type. Entries are evaluated in
written order, and a later duplicate dict key replaces the earlier value.
There is no tuple index operation; a tuple is taken apart by a pattern.

## Bodies and statements

A **body** is a sequence of statements: the body of a `fn`, `proc`, or
`test`, each branch of an `if`, each arm of a `match`, the `else` of a `let`,
and the inside of a `do` block. Statements are evaluated in order. A statement
is a `let` or an expression.

Each of an `if` branch, a `match` arm, a `do` block, and a `let` fallback is
its own scope: a binding made in it ends with it. A callable body is one
scope together with its parameters.

An expression statement whose value is fallible, as the type chapter defines
(a `Result` that can hold an error), is `@type.unhandledFallible`.
`let _ be risky() endlet` discards one explicitly.

### `let`

```vibra
let total be I32.add(a b) endlet
let count: U64 be 0u64 endlet
let {name: n id: i} be user endlet
let Option.some(value: head) be items(0u64) else
  return 0i32
endlet
```

`let pattern be expr endlet` evaluates `expr` once and binds the pattern's
names. The names become visible after `endlet` and stay visible to the end of
the directly enclosing body. They are not visible in `expr` or in the
fallback. `endlet` ends the declaration statement; it is not a scope around
the uses of the names.

A written `: Type` is the expected type of the initializer: it fixes
inference and is one of the places where a widening may occur. Without it, the
type is inferred from the initializer alone, never from later uses.

One `let` holds one pattern. Without `else` the pattern MUST be irrefutable.
With `else` the pattern MUST be refutable, and an irrefutable pattern with
`else` is `@pattern.irrefutableLetElse`. When the value does not match, the
fallback body runs; every path through it MUST leave, so it has type `Never`.

A `let` is a statement and has no value. It cannot appear where an expression
is required; `do … enddo` supplies a place for one.

### `return`, `try`, `await`

`return expr` leaves the innermost enclosing `fn` or `proc`, named or
anonymous, with the value of `expr`, checked at that callable's result type.
It has type `Never`. A callable whose result type is `Void` writes
`return void`.

Every path through a callable body that completes normally MUST end in
`return`; a body that can run off its end is `@type.missingReturn`. A path
of type `Never` needs no `return`. There is no implicit result expression at
the end of a callable body. A `return` in a module-value initializer, or in a
`test` body outside an anonymous callable, is `@type.invalidReturn`.

`try expr` propagates the absence of an `Option` or the error of a `Result`
out of the enclosing callable, and otherwise yields the payload. The type
chapter defines it.

`await expr` runs a plan. It is valid only inside a `proc`, named or
anonymous, or a `test`; elsewhere it is `@effect.invalidAwait`. The
effects chapter defines it.

### `if`, `match`, `do`

```vibra
if Ordered.less(a b) then
  return a
else
  return b
endif

let label be if ready then "go" else "wait" endif endlet

match value
  case Option.some(value: text)
    return Str.length(text)
  case Option.none()
    return 0u64
endmatch
```

`if condition then body else body endif` evaluates the `Bool` condition and
one branch. `match subject case pattern body … endmatch` evaluates the
subject once and runs the first arm whose pattern matches; arms are checked
for reachability and exhaustiveness. `do body enddo` runs its statements in a
scope of its own.

Each of the three is an expression. Its value is the value of the final
expression statement of the body that ran. A body that is empty or ends in a
`let` yields `void`. When the value is used, an `if` MUST have an `else`, and
every branch that completes normally MUST yield the one expected type; a
branch of type `Never` yields nothing and is skipped when branches are
compared. No common supertype, numeric promotion, or union is computed. When
the value is not used, the form is a statement, an `if` may omit `else`, and
each branch MUST have type `Void` or `Never`.

The final expression of a `do`, `if`, or `match` is the value of that form,
never the result of the enclosing callable.

Vibra has no loop, jump, or assignment form. Repetition is recursion or the
standard `Iter` interface. An effectful walk is a recursive `proc` over
`Iter.next`.

## Patterns

Patterns appear in `let`, in `takes`, and in `match` arms, with one grammar
and one set of rules.

| Pattern | Matches |
| --- | --- |
| `_` | Anything; binds nothing |
| `name` | See below |
| literal, atom | That value. Float, `void`, and `Never` literals are not patterns |
| `Owner.case(field: p)`, `User(name: p)` | A case or declared record, by field name; omitted fields are ignored |
| `Celsius(p)`, `Pair(p q)` | A wrapper or declared tuple, positionally, with exact arity |
| `{p q}` | A tuple, with exact arity |
| `{name: p}` | A record, by field name; omitted fields are ignored |
| `[p q]` | A list of exactly that length |

A bare name that resolves to a visible module value whose initializer is a
constant expression is a **constant pattern**: it matches that value and binds
nothing. `true` and `false` are such names. Any other bare name introduces a
binding. A dotted name names a constructor or a constant and never binds. A
name that resolves to a function or to a non-constant module value is neither
and is `@name.redeclaration`. Casing never decides.

A pattern MUST NOT bind one name twice, and a binder MUST NOT shadow a visible
name. Sibling scopes may reuse a name.

There are no rest, alternative, or guarded patterns, and no pattern tests a
run-time type. The type chapter defines irrefutability and coverage.

## Canonical format

`vibra fmt` defines the only canonical representation:

- UTF-8, LF line endings, one trailing newline, no trailing whitespace;
- two-space indentation and a limit of 88 columns;
- one blank line between top-level forms; imports first, in written order;
- one space between tokens on a line; none inside `(`, `)`, `{`, `}`, `[`,
  `]`, before `(`, or before `:`; one after `:`;
- numeric suffixes, string contents, and name spellings exactly as written;
- a construct on one line when it holds no comment, no nested `let`, and
  fits within the limit; otherwise its opener line, its contents indented one
  step, and its closer on its own line at the opener's indentation;
- in a multi-line declaration, each section on its own line, one step in, in
  this order: `of` or the `case` list, `takes`, `labelled`, `returns`,
  `given`, `where`, `requires`, `ensures`, `performs`, `role`,
  `visibility`, `representation`, `external`, `symbol`, `native`, `doc`,
  then the associated type declarations of an interface or the choices of
  an `impl`, then nested `fn` and `proc` members, then `impl` blocks, then
  `do`; within each of these three groups the written order is kept;
- the statements of a callable body two steps in from the declaration, one
  step in from `do`; a guard's `else` on the next line one step in from its
  guard when both do not fit on one line;
- in a multi-line `if`, `then` on the condition's line and `else` alone on
  its line at the `if`'s indentation; in a multi-line `match`, each `case`
  one step in and its body one further step in;
- a multi-line `let` with `be` on the pattern's line, the initializer on that
  line when it fits and otherwise on the next line one step in, `else` at
  the end of the initializer's last line, the fallback one step in, and
  `endlet` on its own line at the `let`'s indentation; a single-line `let`
  ends with `endlet` on the same line;
- a brace or bracket form on one line when it fits, otherwise one entry per
  line, one step in, with the closing token on its own line;
- arguments in written order, always; and
- comments preserved: a comment on a construct's line stays at the end of
  that line, and any other comment stays on its own line before the form
  that follows it.

Formatting MUST be idempotent and MUST preserve meaning. It never reorders
anything that is evaluated. If the parser recovered from a syntax error
anywhere in a file, the formatter MUST return the file's bytes unchanged.

## Exclusions

The following are not part of the source language. Each is a deliberate
exclusion; the reason and the condition for revisiting it are recorded in the
roadmap.

- commas, semicolons, infix and prefix operators, `=`, grouping parentheses;
- loops, jumps other than `return`, and assignment;
- variadic parameters and variadic calls;
- call-site type arguments: a call whose type arguments its context cannot
  determine is rejected;
- an `as` ascription expression and an `as` narrowing pattern;
- untagged union types and anonymous variant types;
- tuple index projection;
- selective, relative, string, and wildcard imports, and re-exports;
- local named declarations and method-call sugar;
- string interpolation, raw strings, numeric separators, and non-decimal
  bases;
- rest, alternative, and guarded patterns;
- macros, reader extensions, and any user-defined syntax.
