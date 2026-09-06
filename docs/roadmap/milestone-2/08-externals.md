# Step 8 — closed compiler externals and pure stdlib bootstrap

Requires Step 7 and C7/C8. Read source **External definitions**, runtime
**External providers**, **Evaluation**, types **Control flow and failure**,
projects **Dependencies and lock**: [source](../../spec/01-source-language.md),
[runtime](../../spec/06-runtime.md), [types](../../spec/02-type-system.md),
[projects](../../spec/04-programs-and-packages.md).

## Implementation sequence

1. Implement only the reviewed C7 registry entries. Store exact signature,
   version and semantic identity in backend-neutral data; map admitted IDs to
   interpreter operations. No string-selected arbitrary host function.
2. Verify C8's toolchain provenance before accepting an external declaration.
   Check provider/symbol pairing, no body, exact types, and empty effects.
   Untrusted source cannot acquire authority by copying a trusted declaration.
3. Reject unknown symbols/providers and source Wasm/WASI declarations before
   execution. `@host` is a recognized provider with later availability, not
   executable M2 functionality; preserve the source placement rules.
4. Add the reviewed pure library sources and explicit-import bootstrap. Higher
   operations are ordinary Vibra functions over the small registry. No ambient
   prelude, dependency sync, hidden stdlib path search, or archive source copy.
5. Execute each operation through typed IR and the ordinary function-call path.
   Expose canonical values and empty audit traces through interpreter cases.

| Positive | Negative / boundary |
| --- | --- |
| Every trusted entry with exact signature | Unknown symbol, wrong types/arity/provider, body present, nonempty ceiling |
| Explicit import of bootstrap library | Missing import; spoofed module/path/package; modified trusted bytes |
| Every operation's specified bool/Unicode/numeric/text edge | Checked arithmetic must not wrap, trap, or invent a private result type |
| Repeated inputs yield identical values and empty events | Environment/clock/random/filesystem dependence and host callback access |
| Valid plain source function sharing a textual name | Ordinary source external declaration must still be rejected |

C7 must specify floating equality/NaN and signed-zero behavior for every
applicable operation. Do not inherit host-language defaults by accident. Defer
fallible arithmetic/conversions requiring M3 nominal results; use only the
explicit M2 inventory. Assertions' test outcome path is completed in Step 13.

Run [common validation](validation.md), focused registry/types/interpreter tests,
and independent `V1-RUNTIME-*` cases. Done includes a registry-to-case table,
provenance/tamper evidence, unknown-provider rejection, and proof that the
bootstrap works offline from its documented exact inputs.
