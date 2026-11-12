# Checked codebase encoding

This chapter defines the canonical records used for checked object and
revision addresses in M1. It is normative for `@checked-definition.v1`,
`@checked-component.v1`, `@codebase-revision.v1`, and `@source.v1` records.
The workspace encoder consumes immutable resolver and checker facts. It MUST
NOT hash source bytes as checked identity, use Rust debug output, or infer
entity targets from a written name. Source attachments retain the exact bytes
separately.

Import MUST request complete immutable declaration facts from the existing
checker, including unreferenced builtin members owned by an admitted type and
abstract interface signatures. An execution-oriented subset of checked
functions is insufficient for an object that owns the complete declaration.
This complete projection mode MUST NOT change the builtin inventory selected
by ordinary file-workspace execution.

## Canonical VIBON bytes

Every record uses the field order stated here and in the codebase-data chapter.
Canonical bytes are the shared VIBON formatter's output: LF line endings and
one trailing LF. Strings use canonical VIBON string spelling. Record field
order is never sorted. A semantically unordered array uses the ordering stated
for that array; all other arrays preserve language-defined or source order.
Address hashing consumes these complete canonical bytes, including the final
LF.

The final LF belongs to the complete document frame. Inline record values,
canonical entity-key selector strings, and normalized-type text embedded in
implementation keys use canonical VIBON value text with no trailing LF.
Embedding an inline value does not introduce an additional document delimiter.

## Definition tree

An object definition is the record `(record format: @checked-definition.v1
kind: @K form: NODE)`. `K` is the owning source form (`def`, `defn`,
`deftype`, `defint`, `deffect`, `implementation`, or `test`). `NODE` is one
closed recursive alternative:

| Node | Exact fields after `kind` | Value |
| --- | --- | --- |
| `@list` | `items: (array NODE...)` | A source form or ordered group. |
| `@atom` | `value: "..."` | Canonical syntax atom that is not a reference, literal, label, or binder. |
| `@atom-value` | `value: "..."` | Decoded atom literal value. |
| `@label` | `name: "..."` | Exact observable source label. |
| `@string` | `value: "..."` | Decoded Unicode string value. |
| `@character` | `value: "..."` | One decoded Unicode scalar value. |
| `@bytes` | `value: "..."` | Exact byte sequence as lowercase hexadecimal, two digits per byte. |
| `@integer` | `type: @T value: "..."` | Checked integer type and exact signed base-10 value without suffix. |
| `@float` | `type: @T value: "..."` | Checked float type and canonical round-tripping literal spelling. |
| `@void` | none | The void literal. |
| `@reference` | `identity: "..."` | Resolver-confirmed canonical declaration or trusted registry identity. |
| `@binder` | `binder-kind: @K index: N u64` | A normalized lexical or generic binder. |

The outer `@checked-definition.v1` fields are exactly `format`, `kind`,
`form`, in that order. A node's first field is always `kind`; no unlisted
fields or alternatives are valid. Literal values are taken from decoded syntax
literal values after checking. Integer and float type atoms are the language's
canonical checked type spellings.

`form` is a checked semantic tree built from the accepted source declaration,
its existing resolved references, and its existing checked IR. It is not a
formatted source string, Rust debug value, or serialized diagnostic snapshot.
Each tagged form is a `@list` whose first child is an `@atom` tag and whose
remaining children are its fields in the order below. A field is itself a
`@list` beginning with an `@atom` field name, followed by its value nodes.
Arrays are represented by `@list` nodes whose children are their values.
Every declaration list begins with its source form atom and retains public
names, exact labels, attributes that affect checking, defaults, bounds, types,
effect rows, and source ordered bodies/owned parts. Attributes use
`Attribute::canonical_order()`; effect rows use the checker-provided
`EffectRowOrder`; call operands use the matching `ApplicationBinding` when
source order is semantically unobservable. Imports are represented by
resolver-confirmed targets, never local aliases. Comments, whitespace, source
IDs, spans, checkout paths, raw local/generic names, and allocator/checker
indices are omitted.

### Checked IR expression forms

The body field is a recursively tagged checked IR tree. The first field is
always `@atom("expr")`; the second is the variant atom. Remaining fields
preserve this exact order:

| IR variant | Fields after the tag |
| --- | --- |
| literal | `value` |
| external | `identity` (`@compiler:<symbol>` reference), `arguments`, `result` |
| host | `identity` (`@host:<symbol>` reference), `arguments`, `result` |
| default | `type` |
| sequence | `expressions` |
| variable | `binder`, `type` |
| global | `identity` reference, `type` |
| function | `identity` reference, `signature` |
| closure | `signature`, `parameters`, `captures`, `capture-types`, `body` |
| captured | `binder`, `type` |
| let | `binder` (or `@void` for discard), `value`, `body` |
| match | `scrutinee`, `arms` (pattern then body per arm), `type` |
| widen | `value`, `type`, `member` (or `@void`) |
| try | `value`, `type`, `exit-type` |
| return | `value` |
| if | `condition`, `then`, `else` |
| call | `target`, `arguments`, `result`, `tail` |
| record | `type`, ordered `(name, value)` fields |
| variant | `type`, `name`, `payload` (or `@void`) |
| wrap | `type`, `value` |
| project | `record`, `field`, `type` |
| tuple | `type`, ordered `components` |
| tuple-project | `tuple`, `index`, `type` |
| array | `type`, ordered `elements` |
| dict | `type`, ordered key/value `entries`, `key-order` (or `@void`) |
| lookup | `collection`, `key`, `key-order` (or `@void`), `type` |

Call targets are tagged `direct` with a resolver identity, `indirect` with the
checked callee expression, or `contract` with the resolved interface identity,
contract member identity, normalized type arguments, member arguments,
destination (or `@void`), and closed-contract atom (or `@void`). A call target
identity also emits the matching `@call` or `@contract` dependency edge.
Every direct global/function target is selected from the checked module set
and encoded as its canonical resolver identity. Compiler and host targets are
trusted references with their registry fingerprint. A missing or ambiguous
mapping rejects encoding.

### Owning declaration forms

The `form` for each owner kind has this exact tagged field order:

| Owner kind | Tagged form and fields, in order |
| --- | --- |
| `def` | `def`: `attributes`, checked `type`, `initializer` expression |
| `defn` | Source declaration: `function-part` (`name`, `attributes`, `body`). A manifest-owned assertion uses `trusted-assertion` (`member`, `type-parameters`, `signature`). |
| `deftype` | `deftype`: `attributes`, `body`, `members` |
| `defint` | `defint`: `attributes`, `members` |
| `deffect` | `deffect`: `attributes`, `members` |
| `implementation` | `implementation`: checked `receiver`, checked `interface`, `members` |
| `test` | `test`: decoded `name`, checked function `body` |

The `attributes` field is an array sorted by `Attribute::canonical_order()`.
Its entries are tagged `generic` (`binder`, checked `bound`), `visibility`
(`value`), `role` (`value`), `external` (`value`), `symbol` (`value`), or
`native` (`value`). Documentation comments and attributes whose semantics are
already represented by the checked function signature are omitted. Generic
binders use the complete owner's shared ordinal table. `deftype` bodies are
tagged `intrinsic-type` (`name`), `record` (`fields`), `enum` (`variants`),
`wrapper` (`type`), `tuple` (`items`), or `union` (`members`). A field or
variant is tagged with its exact public `name` and checked `type`. Nested
methods are `function-part` nodes in source order. `defint` and `deffect`
members are likewise retained in source order.

Checked type nodes have these tagged forms and exact fields:

| Type node | Fields, in order |
| --- | --- |
| `primitive` | `name` atom |
| `atom-singleton` | `name` string |
| `declared` | `identity` reference |
| `applied`, `interface` | `identity` reference, `arguments` array |
| `array` | `element` type |
| `dict` | `key`, `value` types |
| `tuple` | ordered `items` array |
| `record`, `enum` | `members` array of `member` nodes (`name`, `type`) |
| `union` | `members` array in the checked type's canonical order |
| `function` | `signature` |
| type binder | `@binder` with `binder-kind: @type` |

The checked function `signature` is tagged `signature` with `positional`,
`labelled`, `variadic`, `result`, and `effects` fields, in that order. Each
labelled parameter is `labelled` with exact `name` as an `@label` node, checked
`type`, and checked `default` (or `@void`). Variadic shape is tagged `absent` or
`present` (`type`). Effects are `effects` with sorted `roots` references and
`tail: @atom("none")`. A declared union body retains its source member order;
only an anonymous checked union uses the type system's canonical member order.

A `function-part` has fields `name`, `attributes`, and `body`. A concrete body
is a `checked-function` with fields `signature`, source-semantic `body`,
`lowered-body`, `native-body`, `implements`, and `effect-owner`, in that order.
The lowered and native bodies are `@void` when absent. `implements` is
`@void` or an `implements` record containing `contract` reference, checked
`receiver` type, and normalized type `arguments`. `effect-owner` is `@void` or a
resolver-confirmed effect reference. An abstract `defint` member with no body
uses `abstract-function` with exactly one field, its checked `signature`; this
explicitly preserves a checked contract without inventing an executable body.
The abstract form is valid only for a bodyless interface member. Any other
missing checked function is an encoding error. An implementation member is
tagged `implementation-member` with `contract`, `attributes`, and concrete
checked `body` fields. Its contract and implementation dependencies are
recorded as reference edges at that owned part.

The `signature` is the checked function signature record described above,
including checked parameter/result types, labelled defaults, variadic shape,
and effect row. Primitive-backed `bool`, `str`, and `bytes` types retain the
nominal role identity selected by the checker as a `type-reference` edge;
numeric intrinsic primitives remain primitive types. The names `true` and `false` written
in source remain references to their prelude values; boolean constructor
constants in checked IR refer to the resolved `bool.true` or `bool.false`
variant identities.

The verified `@std.assert` test-registry module contains catalogue-owned
assertion declarations even though its embedded source is only a marker and
has no declaration AST. When the non-running test checker admits an assertion,
the encoder emits one `defn` object keyed by its full canonical standard
library function identity and `entity-kind: @function`. Its form is
`trusted-assertion` with exactly `member` (the assertion member as a decoded
string), `type-parameters` (an array of `@binder` nodes in the closed
`TestAssertion` catalogue order), and `signature` (the checked signature), in
that order. These signatures and generic parameter lists MUST agree with the
verified manifest assertion catalogue and the immutable
`CheckedFunction::test_assertion()` fact. No executable body or placeholder
body is encoded. The declaration's source origin names the admitted
`stdlib/src/std/assert.vib` marker; the exact module bytes are pinned by the
revision's standard-library module hash. An assertion absent from either the
manifest catalogue or checked facts is not projected.

Checked patterns use tagged `pattern` lists with `wildcard`, `bind`, `literal`,
`variant`, `record`, `tuple`, `wrap`, `array`, or `member`. `bind` contains a
normalized binder and checked type; public variant/field names and union member
indices remain. Record fields preserve their canonical checked order.

Function signatures use normalized types, ordered positional slots, labelled
slots in declaration order with exact labels and checked defaults, an explicit
absent/present variadic marker, result type, and an effect-row record. Every
type reference in a signature or body is a `@reference` to a full canonical
resolver identity, and every generic parameter is a `@binder`.

The binder counter starts at zero for each complete object and is shared by
all its nested parts. A first pass over source syntax finds every binder
introduction and assigns indices in depth-first syntax preorder. The emission
pass uses that table for both introductions and uses, so a generic parameter
whose `where:` clause follows the parameter and result types already has the
same index at its earlier type uses. The counter order is:

1. For a `defint` owner, the implicit type binder `self` receives index zero.
   It is not a written generic and has no separate generic attribute node.
   Explicit generic introductions are then preassigned by canonical header
   order, with `where:` bindings in their written order, so a use before the
   clause shares its later introduction index.
2. Function and lambda parameter slots are introduced left to right. Label
   names remain exact labels and corresponding value slots receive binders.
3. Expression children follow the checked IR field order above. A `let`
   initializer precedes its binder and body; a match scrutinee precedes arms,
   and each arm introduces pattern binders before its body. Captured values
   map to the outer binder index they capture.
4. Remaining binding patterns preserve their checked binding order.

Each binder introduction emits `@binder` with its kind (`value` or `type`) and
global zero-based index. Every use of that local name emits the same binder
node. Lexical lookup respects nested scopes; distinct bindings in disjoint
scopes receive distinct indices. Shadowing remains an error under the type
system chapter and MUST be rejected before graph encoding. Written local and
generic binder names are omitted. Labels, public
declaration/member names, field and variant names, nominal identities, and
literal values remain observable and are retained.

Every nonlocal entity name that affects the checked definition MUST be
confirmed by immutable resolver/checker facts and encoded as `@reference`.
This includes type names, interface contract names, bounds, effect roots,
constructors, value/function uses, initializers, implementation targets, and
trusted externals. A required name without a resolution fact rejects graph
encoding. Trusted compiler and host names use their exact `@compiler:<symbol>`
or `@host:<symbol>` identity and the closed registry fingerprint. The
spellings `true` and `false` resolve to the trusted prelude declarations and
are references, not boolean literal nodes.

## Normalized type text

The `receiver` and `interface` fields of an implementation key are canonical
VIBON text for the complete applied type, not `vibra-ir::canonical_type`
output and not a printed source type. The text is the canonical encoding of a
`@normalized-type.v1` record:

~~~vibon
(record format: @normalized-type.v1 form: (record kind: @declared identity: "@demo@0.1.0/app.main.item" arguments: (array ...)))
~~~

The `form` node uses these closed records:

| Kind | Remaining fields, in order |
| --- | --- |
| `@primitive` | `name: @T` |
| `@atom-singleton` | `name: "..."` (exact atom name without its leading `@`) |
| `@declared` | `identity: "..." arguments: (array NODE...)` |
| `@parameter` | `binder-kind: @type index: N u64` |
| `@array` | `element: NODE` |
| `@dict` | `key: NODE value: NODE` |
| `@tuple` | `items: (array NODE...)` |
| `@record` | `fields: (array (record name: "..." type: NODE)...)` |
| `@enum` | `variants: (array (record name: "..." type: NODE)...)` |
| `@union` | `members: (array NODE...)` |
| `@function` | `positional: (array NODE...) result: NODE labelled: (array (record name: "..." type: NODE)...) variadic: VARIADIC effects: (record roots: (array (record kind: @reference identity: "...")...) tail: @none)` |
| `@interface` | `identity: "..." arguments: (array NODE...)` |

`VARIADIC` is exactly `(record kind: @absent)` when the function has no
variadic tail, or `(record kind: @present type: NODE)` when it has one. Each
labelled record has exactly `name` and `type` in that order. Function effect
roots are sorted by canonical resolved identity and represented as closed
`@reference` nodes; the v1 row tail is exactly `@none`. Function labels and
their types remain in declaration order.

Named types and interfaces use their full canonical resolver identity.
Arguments and tuple/function components preserve their declared order.
Record fields and enum variants are sorted by UTF-8 bytes of the exact member
name because their structural type meaning is order-insensitive. Union members
use the type system's canonical union order. Function labels remain exact
strings and normalized function types sort them by UTF-8 label bytes; checked
declaration signatures retain their declared order. Effect roots are sorted by canonical
resolved identity. Each nested type parameter is encoded with the same global
type-binder index used by the owning declaration tree; spelling is not kept.
The complete `@normalized-type.v1` frame is formatted canonically and its
UTF-8 text is stored in the implementation key. Its type-binder indices use
the same preassigned binder table as the complete owning implementation
definition.

## Object keys and owned parts

Object keys have one of these exact records:

~~~vibon
(record kind: @declaration identity: "@demo@0.1.0/app.main.run" entity-kind: @function)
(record kind: @test module: "@demo@0.1.0/tests.math" name: "edge case")
(record kind: @implementation receiver: "<normalized type VIBON>" interface: "<normalized applied interface VIBON>")
~~~

For a declaration key, `entity-kind` is exactly one of `@type`, `@interface`,
`@effect`, `@value`, `@function`, `@field`, `@variant`, or `@operation`.
Tests use the separate test key above; modules are not addressed entities in
M1. Neither `@declaration` with `@test` nor a declaration-module selector is an
alternative spelling.

The object array and SCC member order compare the complete canonical key bytes
lexicographically as UTF-8. Nominal declarations with equal definitions but
different resolver identities therefore remain distinct objects. Every
top-level `def`, `defn`, `deftype`, `defint`, and `deffect`, every test, and
every explicit `impl` block is one owner object. Nested type fields, variants,
methods, interface contract members, effect operations, and implementation
members remain parts owned by their enclosing declaration or implementation
block. The revision entity key for an implementation member contains its
canonical contract-member identity and the normalized receiver and interface;
its owner-relative path starts with that contract-member identity.

Revision entity keys and CLI entity selectors use the same declaration, test,
and implementation record shapes as the object keys above. An implementation
member additionally uses exactly
`(record kind: @implementation-member contract: "..." receiver: "..." interface: "...")`,
with fields `kind`, `contract`, `receiver`, and `interface` in that order and no
extra field. `contract` is the full canonical resolver identity of the interface
contract member; `receiver` and `interface` are the canonical normalized-type
texts defined above. The triple retains the applied interface arguments and
receiver independently of the member name. It maps to its implementation owner
object plus the owned-part path, rather than creating another object.

An owned part path is `(array "name"...)` from outermost to innermost, with
the empty array denoting the owner. The declaration-to-object map includes
every declaration and nested member identity, including fields, variants,
contract methods, effect operations, tests, implementation blocks, and
implementation-member triples. A nested part change changes its owner
definition and therefore its owner address.

## Reference edges and components

Reference edge kinds are the exact atoms `@value-reference`,
`@type-reference`, `@contract`, `@implementation`, `@effect-owner`,
`@initializer`, and `@call`. A call is also a semantic reference; the converse
does not hold. Each edge records its owner part, kind, and resolved target.
Identical `(part, kind, target-identity)` edges are deduplicated. Edges sort
lexicographically by UTF-8 bytes of that tuple.

For a global `def`, every checked `@value-reference`, `@call`, and `@contract`
occurrence in its initializer also emits an `@initializer` edge to the same
resolved target at the same owner-relative part. The original edge remains as
well. This makes initializer dependencies explicit without inspecting source
spellings. Initializer edges are not dispatch facts; only the corresponding
checked `@contract` occurrence carries contract-dispatch metadata used to pin
eligible implementation objects.

The projection MUST retain the exact checked `CallTarget::Contract` and the
call's checked operand types in evaluation order for each such `@contract`
occurrence. It MUST use those immutable facts with the checker's
implementation-eligibility predicate for each indexed implementation in the
admitted checker scopes. The predicate selects by resolved interface and
member, receiver coverage, and the applied interface arguments after
instantiating the implementation's receiver against the checked receiver
type. A concrete receiver pins only the explicit implementation objects the
predicate admits. A generic-parameter or interface-erased receiver pins its
complete potential set. A destination-dispatched call uses its checked
destination as the receiver. Closed builtin conformance does not synthesize an
explicit implementation object. The graph encoder MUST NOT infer receiver or
interface arguments from source names, rerun resolution, or perform checking.
Each eligible target is the exact implementation object keyed by the
corresponding indexed implementation fact. Distinct checked calls at one owner
part may contribute different eligible sets; the canonical edge rules above
deduplicate their final `(part, kind, target-identity)` tuples.

The graph is the directed graph of all semantic references between owned
objects. Compute its strongly connected components (SCCs). Sort members in
each SCC by complete canonical object-key bytes. Sort SCC processing in
dependency order: a component is encoded only after every external target
component has an address. When several components are ready, process them by
the UTF-8 byte sequence of their sorted complete member keys. This choice does
not depend on acquisition, checker, hash-map, or DFS order.

The canonical component record has exactly these fields:

~~~vibon
(record format: @checked-component.v1 members: (array (record key: KEY kind: @K definition: DEFINITION references: (array (record part: (array ...) kind: @call target: TARGET)...))...))
~~~

Each member has fields `key`, `kind`, `definition`, `references`, in that
order; `members` is nonempty and sorted by key. Each reference has fields
`part`, `kind`, and `target`, in that order. Internal references target
`(record kind: @component-member index: N u64)` using the target's ordinal in
the sorted SCC. External object references target
`(record kind: @object address: "sha256:...")`. Trusted external references
target `(record kind: @trusted-external identity: "@host:..." registry:
"sha256:...")`. Reference arrays sort by the tuple specified above.

If `C` is the canonical component frame, its raw digest is SHA-256 of the
ASCII bytes `vibra-codebase-component-v1`, one `0x00` byte, then `C`. Its
address is `sha256:` plus lowercase hex of that digest. If `D` is the 32-byte
digest and `i` is the zero-based member ordinal, the object address is SHA-256
of the ASCII bytes `vibra-codebase-object-v1`, one `0x00` byte, `D`, and
big-endian `u64(i)`, rendered as `sha256:` plus lowercase hex. A body, resolved
target, or owned-part change changes its component and object addresses. An
unrelated component keeps its address. Pinning external object addresses
propagates changed dependency content to all affected dependents.

## Trusted registry records

The revision's trusted array contains one `@stdlib` record and two `@registry`
records. The standard-library record carries the verified package name and
version, the raw SHA-256 of the exact embedded manifest bytes, and every
admitted module's canonical declaration identity and raw SHA-256 of its exact
embedded bytes. Module records sort by UTF-8 canonical module identity. Raw
hashes use lowercase `sha256:<64 hex>` and have no domain prefix.

The host registry record is exactly:

~~~vibon
(record format: @codebase-registry.v1 provider: @host version: "vibra_v1" minor: N u32 snapshot: "...")
~~~

`minor` is the current `HOST_REGISTRY_MINOR` as `u32`. `snapshot` is the exact
text returned by `vibra_ir::external::host::registry_snapshot()`; it includes
the registry version, closed error types, metadata types, and every row.

The compiler registry record is exactly:

~~~vibon
(record format: @codebase-registry.v1 provider: @compiler version: "compiler-v1" rows: (array ROW...))
~~~

Rows sort by UTF-8 `symbol`. Each row is exactly
`(record symbol: STRING type-parameters: (array STRING...) signature: STRING native: BOOL)`.
The rows come from every `CompilerIntrinsic::all()` entry. `symbol` is
`CompilerIntrinsic::symbol()`, `type-parameters` is
`CompilerIntrinsic::type_parameters()` in returned order, `signature` is
`vibra_ir::canonical_type(Type::Function(intrinsic.signature(&RoleTypes::new(None))))`,
and `native` is `CompilerIntrinsic::is_native()`.

Each registry fingerprint is lowercase `sha256:<64 hex>` of SHA-256 over the
ASCII bytes `vibra-codebase-registry-v1`, one `0x00` byte, then the complete
canonical registry record bytes including exactly one final LF. A revision
stores `(record kind: @registry fingerprint: STRING registry: RECORD)` with
the exact nested record above. The standard-library manifest and module hashes
are raw hashes and do not use this registry domain.

## Sources and revisions

Source identity is exact UTF-8 bytes and remains separate from checked
definition identity. The wrapper `(record format: @source.v1 text: "...")`
is formatted canonically; its domain-separated SHA-256 address uses the ASCII
bytes `vibra-codebase-source-v1`, one `0x00` byte, then those wrapper bytes.
The source hash is raw SHA-256 of the exact source bytes.

The revision manifest is the `@codebase-revision.v1` record specified by the
codebase-data chapter. Its `objects` array contains every local or admitted
trusted declaration, test, implementation block, and implementation member
mapped to its owner object address and part path. It sorts by complete
canonical entity-key bytes. Sources sort by UTF-8 source ID. Trusted stdlib
and closed registry records use the exact schema and ordering in the
codebase-data chapter. The revision address uses the ASCII bytes
`vibra-codebase-revision-v1`, one `0x00` byte, and the complete canonical
revision frame. A revision has no parent or publication timestamp.

## Required vectors

The host suite MUST include fixed independently reviewed address vectors for
one simple declaration, one self-recursive object, one mutually recursive
component, an external dependency, and a trusted registry reference. It MUST
also establish these metamorphic properties:

- changing trivia or alpha-renaming lexical and generic binders preserves
  checked object and component identities, while changing source attachment
  hashes and the revision that binds those exact attachments;
- shadowing is rejected before encoding, distinct bindings in disjoint scopes
  retain distinct ordinals, and changing a label, field,
  variant, public member, or typed literal changes the owner definition;
- changing one dependency body changes all dependent component addresses but
  leaves an unrelated component address unchanged;
- reversing acquisition, declaration visitation, and unordered source input
  does not change canonical records;
- different nominal declaration identities never collapse, even when their
  bodies match;
- incomplete resolution, rejected checks, malformed frames, missing graph
  nodes, duplicate keys, or missing trusted fingerprints are rejected before
  an encoded revision is returned.
