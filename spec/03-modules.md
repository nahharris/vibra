# 3. Modules and names

Status: draft

## A file is a module

Every `.vib` file is exactly one module. There is no module declaration inside
the file; the module's identity is its resolved path. There are no nested
modules, no re-exports, and no wildcard imports.

## Top-level forms

```ebnf
module     = { top-form } ;
top-form   = import | deftype | defint | deffect | defn | const | scenario ;

import     = "(", "import", symbol, string, ")" ;
const      = "(", "const", symbol, type-expr, expr, { attribute }, ")" ;
defn       = "(", "defn", symbol, params, type-expr, { attribute }, body, ")" ;
params     = "(", { param }, ")" ;
param      = "(", symbol, type-expr, ")" ;
body       = { expr } ;
attribute  = label, form ;
```

`deftype`, `defint` and `defn` members are specified in [Types](04-types.md);
`deffect` in [Effects](06-effects.md); `scenario` — the `(test.scenario ...)`
form — in [Toolchain](08-toolchain.md).

Every top-level form is a definition. There are no top-level expressions and no
initialization order to reason about: a `const` must be a compile-time constant
expression (`E-CONST-001` otherwise).

Tests are declarations the runner discovers, not exported symbols: a scenario
has no name in the module's namespace and cannot be referenced from code.

## Imports

```vibra
(import io "@std/io.vib")
(import parse "../parse/src/lexer.vib")
```

An import binds a **module alias** in the importing file. The alias is a single
undotted segment. Every name from that module is then reached through the
alias:

```vibra-expr
(io.stdout.println "hello")
```

There is no way to import a name unqualified. This is deliberate: a qualified
call site tells a reader which module a function came from without consulting
the import list, which is exactly the local-context property design rule 1 asks
for.

### Path resolution

The string operand is resolved in one of two ways, chosen by its first
character:

| Form | Meaning |
| --- | --- |
| `"@<dep>/<path>"` | A path inside the dependency named `<dep>` in `project.vib`. `@std` is the standard library. |
| `"./..."`, `"../..."` | A path relative to the importing file. |

An absolute filesystem path is `E-IMPORT-001`. A relative path that escapes the
target's `root` directory is `E-IMPORT-002`. An unresolvable path is
`E-IMPORT-003`. A dependency name not declared in `project.vib` is
`E-IMPORT-004`.

Import cycles are `E-IMPORT-005`. Modules form a directed acyclic graph, which
is what makes whole-program effect inference a terminating fixpoint over a
finite call graph.

### Aliases

An alias must not collide with another alias in the same file (`E-IMPORT-006`).
An alias may equal the last segment of the imported path, and idiomatically
does. Aliases are file-local and are never re-exported: importing a module does
not make its own imports visible.

**A module never names itself.** There is no self-alias, and importing a module
into itself is `E-IMPORT-005` like any other cycle. Inside `fs`, a type declared
as `error` is written `error`, and its constructors are `error.not-found` and
peers; `fs.error.not-found` is what other modules write. This keeps every
qualified name in a file mean exactly one thing: a reference through an import.

## Visibility

Every top-level definition is **public** unless it carries `visibility:
@private`:

```vibra
(defn write-prefix ((text str)) void
  visibility: @private
  (io.stdout.print text))
```

A private definition is visible only within its own file. Referencing one from
another module is `E-VIS-001`.

Visibility is a label, not a wrapper form. There is exactly one way to spell it,
and it composes uniformly with `deftype`, `defint`, `deffect`, `defn` and
`const`.

A public type's members are public. A private type may not appear in the
signature of a public definition (`E-VIS-002`), because that would export a
value whose type the caller cannot name.

## Name resolution

A symbol in **expression head** position resolves in this order, and the first
match wins:

1. **Primitive operations.** An *unqualified* symbol matching one of the
   primitive names in [Expressions](05-expressions.md) is that primitive,
   always, in every module. A qualified symbol is never a primitive.
2. **Special forms.** `do`, `let`, `if`, `match`, and the rest of the grammar.
   These are recognized before user names and may not be shadowed by a
   definition (`E-LEX-005`).
3. **Lexical bindings** in the enclosing scopes, innermost first.
4. **Module-local definitions** in this file.
5. **Qualified names** through an import alias, a type namespace, an interface
   namespace, or an effect root.

Anything unresolved is `E-NAME-001`.

Declaring a function whose bare name equals a primitive name is permitted; it
is simply unreachable unqualified from inside its own module, and reachable as
`alias.name` from outside. This is how a module may define `option.and` while
`(and a b)` still means the primitive everywhere.

### Qualified-name resolution

A dotted symbol `a.b.c` is resolved as a whole, by trying the longest namespace
prefix that names something and then requiring the remainder to be a member of
it. A prefix that names a namespace but whose remainder is not a member falls
through to the next interpretation rather than erroring immediately, so a
function may share a name with a type without becoming unreachable.

If no interpretation matches, the diagnostic reports the longest prefix that
did resolve, so the author sees which segment was wrong.

### Shadowing is an error

A lexical binder whose name is already visible in an enclosing lexical scope is
`E-SCOPE-001` — a hard compiler error, not a lint, and not suppressible.

This applies to `let`, function parameters, `for` binders, and `(bind name)`
inside a pattern. It does not apply to module aliases or to top-level
definitions, and `_` is exempt, so repeated `_` binders stay legal.

```vibra
(defn total ((values (array int64))) int64
  visibility: @private
  (let sum (mut 0))
  (for value values
    (set sum (add sum value)))
  sum)
```

Rejected — `E-SCOPE-001`, the inner `value` shadows the parameter:

```vibra-bad
(defn total ((value int64)) int64
  visibility: @private
  (let value (add value 1))
  value)
```

Names in sibling scopes may be reused; only *enclosing* scopes conflict. The
diagnostic's primary span is the shadowing binder, with the original binding
attached as related information.

The rationale is empirical: variable shadowing is a leading cause of failure in
model-generated code, and the fix costs no type machinery. It also removes an
entire class of question from the author — a name means one thing for the whole
function.

## Entry point

A target's entry file must define:

```vibra
(import io "@std/io.vib")

(defn main () void
  (io.stdout.println "hello"))
```

`main` takes no parameters and returns `void` or `(result void e)` for any error
type `e`. Arguments come from `env.read.args`; the exit status comes from returning
an error, or from `sys.exit.with-status`. Any other signature is `E-MAIN-001`. A target
whose entry file has no `main` is `E-MAIN-002`.

`main` is the one public function whose effects are **inferred** rather than
declared; see [Effects](06-effects.md).

## Projects

`project.vib` is an ordinary Vibra source file whose single root form is
`(project ...)`. It uses the same reader, the same atoms and the same labels as
every other file. There is no separate manifest format, no YAML, and no TOML.

```vibra-project
(project
  (package "line-filter" "0.1.0"
    doc: "A small line-processing filter.")
  (target line-filter kind: @bin root: "src" entry: "main.vib")
  (dependency std path: "../stdlib")
  (dependency parse path: "../parse"))
```

| Child | Operands | Labels |
| --- | --- | --- |
| `package` | name, version | `doc:` |
| `target` | name | `kind:` `@bin`, `root:`, `entry:` — all required |
| `dependency` | name | `path:` — required in v1 |

Rules:

- Exactly one `package` (`E-PROJ-001`).
- At least one `target` (`E-PROJ-002`). Duplicate target names are
  `E-PROJ-003`.
- `kind:` is `@bin` in v1. `@lib` and the packaging that needs it are deferred.
- Version strings are `major.minor.patch`; anything else is `E-PROJ-004`.
- A dependency named `std` is implied if not declared, and resolves to the
  toolchain's bundled standard library. Declaring it explicitly overrides that.
- `path:` is relative to the directory containing `project.vib`. Git,
  registry and version-solved dependencies are deferred to wave 3, and the
  lockfile they need does not exist in v1.

A dependency's own `project.vib` is read for its targets; its dependencies are
**not** transitively visible to the importing package. Reaching a transitive
dependency requires declaring it.

## The `core` module

One module is in scope in every file without an import: `core`. It provides the
primitive types, `result`, `option`, `ordering`, and the primitive operations.
Its names are still qualified (`result.ok`, `option.some`), so it adds no
unqualified names and cannot collide with a user definition.

`core` is the only implicit module. Every other standard-library module is
imported explicitly like any dependency.
