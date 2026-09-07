# Step 8 — closed compiler externals and pure stdlib bootstrap

Implementation status: executable closed registry and signed bootstrap
verification landed on `codex/m2-step-08-externals`.

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
| Every admitted text operation's Unicode edge | Integer arithmetic remains unavailable; do not wrap, trap, or invent a private result type |
| Repeated inputs yield identical values and empty events | Environment/clock/random/filesystem dependence and host callback access |
| Valid plain source function sharing a textual name | Ordinary source external declaration must still be rejected |

C7's M2 inventory contains no floating or integer compiler operation, so those
semantics remain deferred with the nominal result contract. Do not inherit
host-language defaults by accident. Implement only the exact text operations
listed in the Step 1 ledger. Assertions' test outcome path is completed in
Step 13.

Run [common validation](validation.md), focused registry/types/interpreter tests,
and independent `V1-RUNTIME-*` cases. Done includes a registry-to-case table,
provenance/tamper evidence, unknown-provider rejection, and proof that the
bootstrap works offline from its documented exact inputs.

## Implemented registry-to-case table

The backend-neutral registry carries the exact v1 identity and semantic
contract alongside each signature. Its version is `vibra_v1`; the two semantic
identities are Unicode scalar concatenation for `text.concat` and Unicode
scalar length for `text.length`.

| Registry entry | Checked signature | Execution evidence |
| --- | --- | --- |
| `text.concat` | `str str -> str` | `vibra-interp` intrinsic execution test, including Unicode scalars |
| `text.length` | `str -> u64` | `vibra-interp` intrinsic execution test counts scalars rather than UTF-8 bytes |

`vibra-types::verify_bootstrap` resolves only the fixed files under the supplied
repository root, checks the reviewed SHA-256 values, and verifies the detached
Ed25519 signature over the exact artifact bytes. `check_bootstrap_source` then
admits only the exact `stdlib/m2/src/std/text.vib` bytes and canonical source ID;
ordinary `check_source` reports `@tool.unavailable` for copied declarations.
