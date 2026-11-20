# Step 14: policy-and-recipe-data

Kind: infrastructure. Slices 3 and 4, as data: `@scopePolicy.v1`, its
normal form, hash, and evaluation; path names; the recipe types, the
provider table, and the recipe identity. No grant and no driver.
Depends on: 02.
Crates: `vibra-host`.

## Prerequisites

- Read: [effects chapter](../../../../spec/03-effects.md#scope-policies)
  "Scope policies" whole, except "HTTP policies"; [programs
  chapter](../../../../spec/04-programs-and-packages.md#targets) "Targets"
  from "`grants` is a dict of binding recipes", "Startup order" first
  paragraph; [tooling
  chapter](../../../../spec/05-tooling.md#command-payloads) the row
  `StartupRecord.bindings[]`; [questions](../spec-questions.md) Q7, Q8,
  Q9.

## Entry points

| Existing | Symbol | Use |
| --- | --- | --- |
| `crates/vibra-ir/src/external/host.rs:161` | `pub struct HostRow` | `HostInterface`, the type of a leaf |
| `crates/vibra-syntax/src/schema.rs:55` | `pub enum Slot` | the schema language the policy schema is written in |
| `crates/vibra-workspace/src/project.rs:356` | `fn grant_binding` | the recipe shape the project decoder already accepts |

Proposed: `vibra-host::policy::{Policy, Expr, NameTest, decode,
normalize, hash, count, permits}`; `vibra-host::path::{Name, check_name,
RelativeNames}`; `vibra-host::recipe` as the architecture gives it, with
`Provider`, `Resource`, `Limits`, `PROVIDERS`.

## Files owned

- `crates/vibra-host/src/policy.rs`
- `crates/vibra-host/src/path.rs`
- `crates/vibra-host/src/recipe.rs`
- `crates/vibra-host/tests/it/policy.rs`
- `crates/vibra-host/tests/it/recipe.rs`

## Ordered tasks

1. `path::check_name`: the name profile and the closed reserved set,
   written from the chapter and tested against the same vectors as
   `vibra-fs-scope::name` (step 13). Invariant: the two functions agree
   on every vector; a shared vector file is in this crate's tests.
2. `policy::decode`: the closed schema of `@scopePolicy.v1` of family
   `@filesystem`. An unknown field, kind, version, or family is an error;
   so is a missing field and an empty suffix. Family `@http` decodes to
   "reserved" and is an error for every caller of this release.
3. `policy::normalize`: flatten nested `allOf` or `anyOf` of one kind,
   remove duplicates, sort items by canonical bytes, remove single-item
   wrappers, sort sets; path components keep their order. Explicit stack.
4. `policy::hash`: `sha256:` over the canonical bytes of the normal form.
5. `policy::count`: nodes and entries, and depth, as Q9 recommends; the
   limits 4,096 and 64 are constants of this module.
6. `policy::permits(expr, request)`: one rule at a time; a file rule
   matches prefix, depth, action, and name together; a directory rule
   prefix, depth, action. `request` holds the names, the action, and
   whether it selects a file or a directory. Invariant: no field of one
   rule is combined with a field of another.
7. `recipe`: the types; `PROVIDERS`, the nine rows with their grant types
   and resource kinds; `BindingPlan::validate` (leaf type against the
   provider, resource kind, `source` leaf kind and, for a variable, its
   `names`; policy present exactly for `@host.fs.directory` and rooted at
   its own leaf; `after` contains `source`, names leaves, is acyclic);
   `BindingPlan::order`; `recipe_identity`.

## Test matrix

Host tests of `vibra-host`. `F(prefix depth actions names)` abbreviates a
`@fileRule` rooted at `@notes`.

| Row | Kind | Input | Expected | Specification |
| --- | --- | --- | --- | --- |
| decode a file rule | positive | `{format: @scopePolicy.v1 family: @filesystem expression: {kind: @fileRule root: @notes prefix: [] depth: @tree actions: [@read] names: {kind: @anyName}}}` | decodes | 03 The wire form |
| unknown field | negative | the same with `extra: 1u8` | error | 03: "Unknown fields, kinds, and versions are rejected" |
| unknown kind | negative | `expression: {kind: @glob}` | error | the same |
| unknown version | negative | `format: @scopePolicy.v2` | error | the same |
| missing field | negative | a file rule without `names` | error | 03 Meaning: "A missing field is an error, never a wildcard" |
| empty suffix | negative | `names: {kind: @suffix value: ""}` | error | 03 Names: "An empty suffix is invalid" |
| family `@http` | negative | `family: @http` | error | 03 HTTP policies: "Reserved" |
| normalize: flatten | positive | `allOf[allOf[A B] C]` | `allOf[A B C]`, sorted | 03: "nested `allOf` or `anyOf` of the same kind are flattened" |
| normalize: duplicates | positive | `anyOf[A A]` | `A` | 03: "duplicate items removed … single-item wrappers removed" |
| normalize: sets | positive | `actions: [@stat @read]` | `[@read @stat]` | 03: "and sets sorted" |
| normalize: prefix order kept | positive | `prefix: ["b" "a"]` | unchanged | 03: "Sequences such as path components keep their order" |
| hash of equal normal forms | positive | `anyOf[A B]` and `anyOf[B A]` | one hash | 03: "The hash identifies this normal form" |
| hash of equal meanings | boundary | `A` and `allOf[A anyOf[A B]]` | two hashes | 03: "two policies with the same meaning may hash differently" |
| `none` | negative | `none`, any request | denied | 03 Meaning: "an empty `anyOf`, like `none`, permits nothing" |
| empty `allOf` | positive | `allOf[]`, a read of `a.txt` | permitted | 03: "An empty `allOf` permits everything the parent permits" |
| empty `anyOf` | negative | `anyOf[]`, the same request | denied | the same |
| file rule, `children` | positive | `F([] @children [@read] anyName)`, read `["a.txt"]` | permitted | 03: "`children` means exactly one further name" |
| file rule, `children`, two names | negative | the same, read `["d" "a.txt"]` | denied | the same |
| file rule, `tree` | positive | `F([] @tree …)`, read `["d" "e" "a.txt"]` | permitted | 03: "`tree` any positive number of further names" |
| file rule, the prefix itself | negative | `F(["d"] @tree …)`, read `["d"]` | denied | the same: "positive" |
| prefix by names | negative | `F(["reports"] @tree …)`, read `["reportsOld" "a.txt"]` | denied | 03 Paths: "so `reports` does not cover `reportsOld`" |
| action | negative | `F([] @tree [@read] anyName)`, replace `["a.txt"]` | denied | 03 Actions |
| `ATK-policy-rule-fields-crossed` | negative | `anyOf[F(["a"] @tree [@read] anyName) F(["b"] @tree [@replace] anyName)]`, replace `["a" "x"]` | denied | 03 Meaning: "does not permit replacing under `a`" |
| suffix | positive | `suffix(".txt")`: `notes.txt`, `.txt` | permitted | 03 Names: "accepts `notes.txt` and `.txt`" |
| suffix, case and tail | negative | `suffix(".txt")`: `notes.TXT`, `notes.txt.exe` | denied | 03: "rejects `notes.TXT` and `notes.txt.exe`" |
| exact | positive and negative | `exact(["a" "b"])`: `a`; `c` | permitted; denied | 03 Names |
| name `allOf` and `anyOf` | positive and negative | `allOf[suffix(".txt") exact(["a.txt"])]`: `a.txt`; `b.txt` | permitted; denied | 03 The wire form: `names` |
| a file rule grants no listing | negative | `F([] @tree [@read] anyName)`, list `[]` | denied | 03: "A file rule grants no listing" |
| directory rule, `self` | positive and negative | `@directoryRule` `prefix: ["d"] depth: @self actions: [@list]`: list `["d"]`; list `["d" "e"]` | permitted; denied | 03: "`self` is `prefix` itself" |
| directory rule, `children` | positive and negative | depth `@children`: list `["d" "e"]`; list `["d"]` | permitted; denied | 03: "`children` one name further" |
| directory rule, `tree` | positive | depth `@tree`: list `["d"]` and `["d" "e" "f"]` | permitted | 03: "`tree` the prefix and everything below" |
| `stat` by rule kind | negative | a file rule with `@stat`, stat of a directory | denied | 03 Actions: "`stat` covers `DirStat` for the kind of rule it appears in" |
| size 4,096 | boundary | a policy counted at exactly 4,096 | accepted | 03: "at most 4,096 nodes and entries in total" |
| size 4,097 | negative | one more entry | too complex | the same |
| depth 64 and 65 | boundary, negative | nested `allOf` wrappers of distinct kinds to depth 64; to 65 | accepted; too complex | 03: "a depth of at most 64" |
| deep input | boundary | 100,000 nested `anyOf` given to `decode` on a 256 KiB thread | too complex, no overflow | architecture, risk 5 |
| names agree with the adapter | positive | the shared vector file | `path::check_name` equals `vibra_fs_scope::name::check` on each | 03 Reserved device names: "the same closed set is rejected on every platform" |
| provider table | positive | each of the nine providers with each of its grant types and resource kinds | `validate` accepts | 04 Targets, the provider table |
| provider cannot issue the type | negative | `@host.io.stdout` for a `DirReadText` leaf | `BindingFault` at `provider` | 04: "The leaf's type … MUST be one of the issuer's grant types" |
| resource kind | negative | `@host.random` with `@fixedPath` | `BindingFault` at `resource` | 04: "of a kind the issuer accepts" |
| `source` of the wrong kind | negative | an `@argumentPath` whose `source` is an `EnvReadVar` leaf | `BindingFault` at `source` | 04: "a leaf of type `EnvReadArgs` for an argument" |
| variable outside `names` | negative | `@variablePath name: "HOME"` whose source's `names` are `["TMP"]` | `BindingFault` | 04: "whose `names` include the variable" |
| `after` lacks `source` | negative | `source: @args`, `after: []` | `BindingFault` at `after` | 04: "It MUST contain the recipe's `source` leaf" |
| `after` cycle | negative | `@a` after `@b`, `@b` after `@a` | a cycle fault at the later edge | 04: "The `after` relation MUST be acyclic" |
| policy on another provider | negative | `policy` on `@host.io.stdout` | `BindingFault` at `policy` | 04: "for `@host.fs.directory` only" |
| policy missing | negative | `@host.fs.directory` with no `policy` | `BindingFault` | 04: "and required there" |
| policy with another root | negative | leaf `@notes`, a rule with `root: @logs` | an invalid-policy fault | 04: "Its rules name this leaf as their root" |
| order by spelling | positive | leaves `@b`, `@a`, `@logs.reader`, no `after` | `@a`, `@b`, `@logs.reader` | 04 Startup order: "smallest in UTF-8 byte order" |
| order with `after` | positive | `@a` after `@z` | `@z`, `@a` | 04: "A leaf is ready when every leaf in its `after` list is processed" |
| recipe identity | positive | one recipe written with its policy items in two orders | one identity | 05: "the canonical bytes of the recipe with its policy normalized" |

## Diagnostics and schema changes

No diagnostic is reported here: `BindingFault` and the policy errors are
data that step 26 maps to `@grant.invalidBinding`, `@grant.bindingCycle`,
`@grant.invalidPolicy`, and `@grant.policyTooComplex`. The
`@scopePolicy.v1` schema is new in `vibra-host`.

## Validation

```powershell
$env:RUSTFLAGS='-D warnings'
cargo fmt --all --check
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline -p vibra-host -p vibra-workspace --all-features
$env:RUSTDOCFLAGS='-D warnings'
cargo doc --locked --offline --workspace --no-deps --all-features
```

Read: all exit 0.

## Stays unavailable

Everything observable: no command reaches this code yet.

## Excluded

Grants and counters (15); decoding a project's recipes (26); HTTP rules.

## Completion evidence

- [ ] Each row is a test, named in the report.
- [ ] `policy.rs` has no function that calls itself on a sub-expression.
- [ ] The identity of the chapter's example recipe is computed by hand in
      the report and equals the test's constant.
- [ ] The validation commands exit 0.
