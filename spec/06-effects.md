# 6. Effects

Status: draft

Every Vibra function has an **effect row**: the set of host capabilities it may
exercise, transitively. Rows are computed statically and checked at declaration
boundaries.

## Read this first: v1 does not enforce effects at run time

In v1 the effect row is a **compile-time fact about the source**. After
checking, it is erased. `vibra run` grants the program whatever authority the
launching process has, and no host operation re-checks anything.

This is a scoping decision, not a design position. A language that describes
authority it never checks is describing decoration, and this specification says
so plainly rather than letting a reader infer a guarantee that does not exist.
Wave 1 of the post-v1 plan converts declared rows into runtime grants checked at
every host operation. Until then:

- No v1 document may describe Vibra as capability-safe or sandboxed.
- Do not execute untrusted Vibra source under v1.

What v1 *does* buy is real: an author, a reviewer, and a tool can all read a
function's signature and know the complete set of host capabilities it can
reach, and the compiler proves that reading correct.

## Roots and operations

An effect **root** is a nominal name declared with `deffect`. Its canonical
identity is `<module>.<root>`, resolved through the defining module, never
through an import alias.

```ebnf
deffect = "(", "deffect", symbol, { attribute }, { member }, ")" ;
```

Attributes: `visibility:`, `doc:`. Members are `defn` forms named
`<root>.<operation>`, following the same block-namespacing rule as `deftype`
and `defint`.

This is the `fs` module, so its own names are unqualified here:

```vibra
(deftype reader (newtype (handle @read)))

(deftype error
  (enum
    (not-found str)
    (permission-denied str)))

(deffect read
  doc: "Reading from the filesystem."

  (defn read.open ((path str)) (result reader error)
    (intrinsic @fs-open-read path)))
```

That operation's canonical name is `fs.read.open`, and callers in other modules
write `(fs.read.open path)`.

An operation **owns** its root implicitly. Its `effects:` label lists only the
*additional* roots it reaches, so the row of `fs.read.open` above is exactly
`(fs.read)`.

A root name colliding with another top-level name in its module is
`E-EFFECT-004`. A duplicate operation name within a root is `E-EFFECT-005`.

## Where declarations are required

This is the central ergonomic decision of the effect system.

| Position | Effects |
| --- | --- |
| Public `defn`, and any member of a public type | **Declared.** Absence declares the empty row. |
| Interface method in `defint` | **Declared.** Sets the ceiling for implementations. |
| `deffect` operation | **Declared.** The additive roots beyond its own. |
| Private `defn` (`visibility: @private`) | **Inferred** from the body. |
| Member of a private type | **Inferred.** |
| `fn` literal | **Inferred.** |
| `main` | **Inferred.** |

### Absence declares purity

At a position where a declaration is required, **an absent `effects:` label
declares the empty row.** A public function with no `effects:` is a claim that
it reaches no host operation at all, and the compiler checks that claim like any
other: a body that performs an effect is `E-EFFECT-001`.

This matters because most of a standard library is pure and public, and
requiring every one of those functions to carry `effects: ()` would be pure
noise on the majority of signatures.

Writing an explicit empty row is therefore `E-EFFECT-008` — absence already
means it, and two spellings of one meaning is what canonical form exists to
prevent.

An `effects:` label at an *inferred* position is `E-EFFECT-007`. There is one
correct place for the information, and restating it is not a second correct
place.

### Why boundaries and not everything

The pre-reboot language required every function to declare its complete
transitive ceiling by hand. In practice `main` ended up carrying rows like
`(fs.read fs.write io.stdout io.stderr stream.read stream.write stream.manage)`,
none of which is derivable from anything visible in `main` — it is the
transitive union of everything the program reaches. An author had to either
know the whole standard library's effect table or iterate against the compiler.

The justification for mandatory annotation is that annotations let a
type-directed decoder terminate its search **at signature positions**. A
transitive union restated at an internal call site is a derived fact, not a
signature, and buys nothing. So v1 keeps the declarations where they pay —
module boundaries, interface contracts, host operations — and infers the rest.

`main` infers because it is the worst offender and its boundary is the program,
not a signature any caller reads.

## Inference

For inferred positions, the compiler computes the least fixed point of the
union over the resolved call graph:

- A call contributes the callee's row.
- A `deffect` operation contributes its owned root plus its declared additives.
- An interface dispatch contributes the union over every implementation
  reachable at that call site's concrete types. Because generics are
  monomorphized and conformance is declared, that set is finite and known.
- Recursion is handled by the fixpoint; mutually recursive private functions
  converge to a common row.

The module graph is acyclic and every call graph is finite, so the fixpoint
terminates.

An inferred row always reports **leaf operations**, never a declaration root. A
row is a set: order is not significant, and duplicates are collapsed.

## Checking a declaration

At each declared boundary, the compiler compares the declared row against the
inferred row of the body.

| Relation | Result |
| --- | --- |
| declared ⊇ inferred | Accepted |
| declared ⊉ inferred | `E-EFFECT-001`, naming each uncovered operation |
| declared ⊋ inferred | `W-EFFECT-001`, naming each unused root |

Over-declaration warns rather than errors so that a boundary may be widened
deliberately ahead of an implementation change, but it is a warning, not
silence, because an inflated ceiling is a false statement about what a function
can reach.

### Root subsumption

A declared **root** covers every operation under it. Declaring `fs.read` covers
`fs.read.open`, `fs.read.to-str` and the rest.

Subsumption applies only to this comparison. It does not rewrite the stored
declaration and it does not change what the `effects` report prints for the
inferred row. Most long effect rows are root-family enumeration, and this
collapses them without losing precision anywhere it matters.

```vibra
(import fs "@std/fs.vib")
(import io "@std/io.vib")

(defn copy-report ((source str) (destination str)) (result void fs.error)
  effects: (fs.read fs.write io.stdout)
  (let contents (try (fs.read.to-str source)))
  (try (fs.write.from-str destination contents))
  (io.stdout.println "copied")
  (result.ok unit))
```

### Interface ceilings

An interface method's declared row is a **ceiling** on every implementation. An
implementation whose inferred row exceeds it is `E-EFFECT-003`.

Implementations may differ from each other: one may perform nothing, another
may read the filesystem, as long as both stay under the ceiling the interface
published. A caller reasons about the ceiling, never about which implementation
it got.

A generic function's declared row is fixed. Effect polymorphism — a combinator
whose row depends on the row of its function argument — is deferred; see
[Deferred and rejected](10-deferred.md). Until then, a higher-order function
that takes an effectful callback must declare a ceiling covering it.

## The host boundary

Two forms cross from Vibra into the host. Both are restricted.

### `intrinsic`

```ebnf
intrinsic = "(", "intrinsic", atom-literal, { expr }, ")" ;
```

An `intrinsic` names a closed, compiler-known operation from the versioned
`vibra_v1` registry. Its arity and its argument and result types are checked
against the registry entry during lowering (`E-ABI-001` on mismatch, `E-ABI-002`
for an unknown name).

An intrinsic that crosses the authority boundary — anything touching the
filesystem, the environment, processes, the clock, or randomness — **must** be
the body of a `deffect` operation. Using one anywhere else is `E-ABI-003`. Pure
value intrinsics may be called from ordinary functions.

This is what makes the effect row complete: there is no other way to reach the
host, so a function's row is exactly the set of operations it can transitively
reach, with nothing hidden underneath.

### The ABI carries scalars only

Every value crossing the `vibra_v1` boundary is numeric. Dynamic values — a
`str`, an `array`, a `record`, a handle — cross as **nonzero indices into a
host-owned arena**, checked and resolved on the owning side. An index is not a
guest pointer and confers no access to host memory.

Mutable cells, references, function values and pointers never cross
(`E-ABI-004`), including nested inside an aggregate.

This constraint is adopted now because it is nearly free while the ABI is small
and very expensive to retrofit. Isolation arguments for compartmentalized
compilation depend on the cross-compartment interface carrying scalars only;
extending them to shared pointers is an open research problem. Wasm already
supplies well-bracketed call and return and numeric-only parameters, so keeping
the rule costs nothing today.

The host arena is reclaimed per scope. An implementation whose arena grows
monotonically for the life of a program does not conform, because it makes the
memory ceiling scheduled in wave 1 unenforceable.

### Foreign WebAssembly is not in v1

Linking dependency-provided `.wasm` modules is a weaker, pointer-carrying
boundary and is explicitly outside the `vibra_v1` claim. It is deferred to
wave 4.

## Hardening runs last

Compiler stages that establish reachability and emit Wasm run before any
hardening stage. Hardening passes form the final contiguous suffix of the
pipeline. A later layout-changing or semantics-changing pass is a contract
violation, and the ordering is asserted by a test rather than left to
convention.

Composing passes preserves only the intersection of their property classes, so
pass ordering is a correctness constraint, not a performance preference. v1 has
no concrete hardening transform; the invariant is recorded now so that the
first one cannot be added in the wrong place.

## Reporting

`vibra effects` prints, per function: the declared row where one exists, the
inferred row, the owning root and additive roots for each operation, and the
outgoing call edges used to compute the fixpoint. The inferred rows are the same
fixpoint the checker uses — there is no second call-graph implementation.

Output is deterministic and schema-described; see [Toolchain](08-toolchain.md).
