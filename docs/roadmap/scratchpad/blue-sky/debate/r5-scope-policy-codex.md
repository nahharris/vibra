# Round 5: resource-scope policy forms — Codex

Proposed default for Claude to attack, 2026-11-16. This completes the **design choice** behind Nexus O4/A-20; it does not claim the adapters or confinement tests exist. Changes remain inside debate/.

[argued] Amendment after Claude's review: **accept A and B**. HTTP targets gain component-prefix selection with exact-or-any query matching; the Windows filesystem adapter gets a v0.0.2 M0 feasibility prerequisite. Also make fresh per-invocation traversal and the hard-link consent warning mandatory below. These amend the initial exact-target-only proposal, not a measured result.

[cited] Basis: [Nexus O4](../../../nexus/decisions.md) requires selected endpoints/protocols/verbs and read-only `.txt` files beneath a run-time selected directory. [A-7/A-20](ledger.md) and [the accepted native review](r5-native-how-codex.md) require closed effect ceilings plus sealed grants, pure attenuation, shared accounting, actual dispatch checks and separate HostResource/ActorEndpoint meanings. I accept the proposed split: Claude prepares the effective design and X-K3c registration; I propose these policy forms.

## 1. Decision: ordinary typed policy data, sealed authority

[argued] Use **closed, versioned standard-library data types and ordinary constructors/functions**. No policy keyword, embedded DSL, regex engine, user-supplied predicate Fn, dependent path type or source-level grant minting at this gate. Arbitrary pure predicates would make inclusion/normalization opaque, introduce termination/fault behavior into the security boundary, and hide dependencies in consent. **Instead:** provide the small positive tests below, with later extensions requiring typed semantics, schema versions and enforcement cases.

| Entity | Meaning |
| --- | --- |
| Policy value | [argued] Describes allowed requests. Fully ordinary serializable data. Anyone may describe a broad policy; that grants nothing. |
| Binding recipe | [argued] Protected target data naming a reviewed issuer, held/approved root, requested policy and limits. Build validates the recipe; startup performs any required acquisition. |
| Grant | [argued] Sealed runtime token referencing a bound resource, effective policy, world/provider and accounting lineage. Not constructible by policy parsing, a wrapper constructor, casts or VIBON decoding. |
| Request | [argued] A typed operation plus its typed arguments. The adapter derives the concrete action/resource from those arguments; it does not trust an extra caller-written “this request is safe” field. |
| Restriction | [argued] `restrict(parent filter)` returns a grant with effective policy `parentPolicy AND filter`, never a replacement policy. It performs no DNS lookup, file opening or other host action. |

[argued] Use proposed public names `FsPolicy`, `NetPolicy`, `NameTest`, `RelativePath`, `Origin`, `IpPrefix`, `HttpTarget` and operation-specific restriction functions, for example `DirRead.restrict` and `NetSend.restrict`. The names and examples here are **unmeasured library API proposals**, not already accepted grammar or existing APIs. Smart data constructors return Result for invalid inputs; grant restriction returns Result for invalid policy/version/complexity, with exact types fixed in the library specification. Ordinary nominal variants/records can express all these data types.

[argued] A policy schema has a required kind/version and no permissive unknown fields or unknown variants. The compiler checks ordinary types and effect ceilings; the runtime checks resource membership. A parameter branded LogDir still certifies its issuer's policy, not a compile-time future directory path. No policy is silently inferred from the human-readable name of a type.

## 2. Policy algebra and normalization

[argued] Each resource family provides `none()`, `allOf(items)` and `anyOf(items)` over **policy data**, plus family-specific rule leaves. Empty allOf is the universal filter; empty anyOf is empty. In a root binding, any universal filter is explicit and subject to approval; it is never the default for a missing field. In restrict, even a universal filter leaves only the held parent's authority. A list of alternatives joins **whole rules**, preserving relationships among resource, operation and additional conditions.

[argued] Meaning is set membership: `allOf` is intersection; `anyOf` is union. `restrict` always retains its parent intersection. No public operation unions two sealed grants into a new stronger grant. A program can intentionally hold two grants in a record and select one; that visible bundle does not imply a synthesized combined root or a new least-authority proof.

[argued] Normalize data in one versioned order: validate all leaves first; flatten nested operators of the same kind; remove duplicate identical children; sort children by canonical typed VIBON bytes; remove a one-child wrapper; reduce empty allOf/anyOf and their identity/absorbing cases. Sort/deduplicate set fields within leaves. Do not reorder request operands, query pairs, acquisition steps or other sequences. Do **not** distribute allOf over anyOf: a conjunction of two alternatives stays a conjunction, avoiding an exponential expansion.

[argued] The normalized-policy hash identifies this canonical **syntax**, not every semantically equivalent policy. No general equivalence, emptiness or subset solver is needed to attenuate: retaining the parent conjunction is the inclusion proof. Two differently written but equivalent formulas may hash differently and trigger a conservative review. No “unknown subset” path approves replacement of a parent constraint. Valid policy expressions that happen to deny everything remain valid deny-all grants; the implementation need not solve emptiness first.

[argued] Choose an explicit first tool profile limit of 4,096 total policy nodes/entries, maximum structural depth 64, with the ordinary input-byte bound also applying. These are reversible engineering defaults, not measured capacity requirements. Count data within rules (including address/target sets), not just Boolean nodes, and check before allocating or recursing. Restriction chains exceeding the limit return policy-complexity failure; they do not omit ancestors, broaden to all, or recurse on the native stack. A future compact representation may change this limit under a new tool profile without changing membership semantics.

### Relation preservation is essential

[argued] Parent permits `(HTTPS, apiA, GET)` and `(HTTPS, apiB, POST)`. Extracting `{apiA, apiB}` and `{GET, POST}` into separate sets invents POST to apiA and GET to apiB. **Instead:** keep two correlated rules, and intersect whole membership tests. The same applies to `{rootA, read}` versus `{rootB, write}`, endpoint-specific IP sets and actor message fields.

[measured] [Finite model](experiments/scope-policy/model.py), [raw results](experiments/scope-policy/results.json): the projection produces exactly those two unintended requests. For 1,000 seeded policy pairs over eight abstract requests, all 8,000 membership comparisons match a separate finite-set oracle after intersection/normalization; checks also cover idempotence, commutation and duplicate removal. This is evidence for the finite algebra model only, not Vibra typing, OS confinement, HTTP parsing, quotas or any production adapter.

## 3. Filesystem policy

[argued] A filesystem rule consists of a **bound root slot**, a relative component prefix, a depth selector, an action set and a basename test. Root slots refer to broker-held directory objects acquired from held authority; policy strings do not open roots. Root A and Root B remain distinct even when their paths look similar. `restrict` only references slots in the parent and intersects them; an unknown/different-world root is rejected, not adopted.

| Field | First form / meaning |
| --- | --- |
| Root | [argued] Logical binding slot in policy/manifest data; sealed anchor identity in a live grant. Absolute spelling is acquisition input and display evidence, not authority by itself. |
| Prefix | [argued] `RelativePath` as a component vector, empty for the root. Membership is component-prefix comparison, never string starts-with. Pure sub grants add/retain this constraint on the same anchor. |
| Depth | [argued] `children` permits a file immediately inside the selected directory; `tree` permits any descendant file. Traversing intermediate directories is internal to a permitted file operation, not a listing/read grant for their contents. |
| Actions | [argued] Closed individual actions: read contents, append existing, replace contents, create new, list directory, remove entry and rename entry. Map each registry operation to its actual actions; a text-write API that creates or truncates must not be classified as append-only. Rename checks both source and destination. Adding an action requires a registry/policy contract, not an unrecognized atom. |
| Basename test | [argued] `anyName()`, `exact(names)` or `suffix(value)` and allOf/anyOf. Exact Unicode-scalar spelling, no case folding, locale folding or normalization. Suffix is literal suffix matching, not a glob or a claim about format/content. Empty suffix is rejected; use explicit anyName instead. |

[argued] A portable RelativePath accepts a vector of nonempty names. Reject `.`, `..`, slash, backslash, NUL/control characters, colon, trailing dot/space and Windows reserved device-name spellings (including their extension aliases). Absolute, drive-relative, UNC, device-namespace and alternate-stream spellings cannot be represented by this type. Do not strip, percent-decode, expand environment variables, normalize away `..`, or reinterpret a name as a URI. This intentionally restricts filenames that the host OS might otherwise permit. The explicit native-path import function returns a typed error for excluded names; bytes that cannot be represented as Unicode scalar names are outside this first portable profile.

[argued] `NameTest.suffix(value: ".txt")` accepts `notes.txt` and `.txt`; rejects `notes.TXT`, `notes.txt.exe` and a `notes.txt:secret` path (the last is invalid before matching). The adapter must check the actual directory-entry spelling used for the opened object, and reject case-insensitive/short-name aliases that would bypass the declared test. It must not check only the user-supplied alias. This makes behavior more restrictive than ordinary Windows lookup, but consistent with the exact-case policy the user reviews. Directory traversal checks root/prefix/depth; a permitted filename alone never authorizes a different directory.

[argued] A read grant has only read actions. It does not grant listing, write, delete, create, rename or ACL modification. A read+write bundle narrows by deriving a **sealed** read-only child from its held read authority, not merely erasing the concrete type to a read interface. If a requested operation needs an action absent from the parent, dispatch returns typed denial. Internal metadata needed to resolve/check a permitted action is allowed to the trusted adapter; it is not an API for returning directory listings or metadata the source was not granted.

[argued] File rules above govern file-content operations. Directory listing/removal/rename need a separate typed directory selector, not a fictitious file basename on the directory request. Defer those additional selectors unless an admitted operation requires them; a recognized but unimplemented action remains unsupported. In particular, the read-only `.txt` grant is not an enumeration grant. A future filtered listing must check each exposed entry and specify directory traversal and metadata separately; it cannot return all names just because the parent directory was permitted.

### Concrete owner example

```text
Protected startup recipe:
  selectedFolder := configuration input under the approved root/broker
  root := acquire directory object(selectedFolder)
  grant := read-only(root,
                    prefix = [], depth = tree,
                    basename = suffix(".txt"),
                    links = forbidden, mount crossing = forbidden)
  bind grant to main's reader parameter
```

[argued] The text block describes recipe data, not new source syntax. Equivalent K3 policy construction uses ordinary calls, for example `NameTest.suffix(value: ".txt")` and `FsPolicy.allOf([...])`. Config parsing alone cannot grant the root. Missing approved issuer/recipe fails build; denied acquisition of a required root fails startup; optional specified refusal yields None. Pure `sub` extends a relative-prefix restriction without opening it or asserting that it exists.

### OS enforcement and the boundary of the claim

[argued] Anchor resolution to a held directory object. Resolve from held handles using an adapter that refuses symlinks/reparse traversal and mount crossing, verifies the final regular-file kind and actual entry name, and performs data access using that same opened object. No `canonicalize/check-prefix/open-absolute-path` sequence. Required internal handle inspection must not read file contents or truncate/create a target before authorization. Unsupported filesystem/adapter semantics return unsupported/denied before the action; do not silently fall back to unchecked path APIs.

[argued] Separate nondestructive resolution from destructive work: do not truncate on the initial open of an existing file, before kind/name/rights checks complete. A create-new action first authorizes the parent/name and uses exclusive creation; an existing case/alias collision must not turn it into replace-existing. If a platform cannot preserve those semantics under races, that adapter/action is unsupported until fixed. Partial/ambiguous external outcomes keep the operation's typed delivery status, not a fabricated “nothing happened.”

[cited] Microsoft's [OBJECT_ATTRIBUTES](https://learn.microsoft.com/en-us/windows/win32/api/ntdef/ns-ntdef-_object_attributes) documents relative RootDirectory handles and a no-reparse flag. The [user-mode NtCreateFile reference](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile) separately documents relative names and file access/open controls. Those pieces are implementation leads, **not evidence that our proposed complete Windows race/confinement adapter is already qualified**. On Linux, [openat2](https://man7.org/linux/man-pages/man2/openat2.2.html) documents beneath/no-symlink/no-cross-mount resolution and race-related failure. Do not replace platform-specific validation with a shared string sanitizer.

[argued] Make the temporal promise precise: authority is bound to directory/file **objects** reached through authorized handle-relative traversal; it is not a promise about the spelling or location of a global pathname forever. Root rename does not transfer authority to a replacement object now occupying the old path. Already acquired directory/file handles may remain authorized if another process moves those objects; use the acquired object, never reopen by the old absolute name. New walks must establish their chain again. This is an explicit object-capability rule, not a continuously reevaluated global-ancestry sandbox.

[argued] Retain only the grant's own acquired anchor across invocations. **Do not cache intermediate directory handles across invocations.** Pure sub retains a prefix restriction on that same anchor; it does not silently acquire a descendant anchor. Each operation walks again from the grant anchor and releases its intermediate handles when the invocation ends. A subtree moved out is not reachable on the next walk merely because an old walk once reached it. Handles already resolved within one in-flight invocation follow the object-lifetime rule above; this does not promise rollback of an operation racing an external rename. A separately acquired/delegated anchor is a distinct reviewed grant, not this optimization.

[argued] Hard links are an important limit. A genuine `ok.txt` entry can refer to the same file as an entry elsewhere; a name policy does not establish unique ownership, secrecy, file format, or that writing through the permitted entry cannot be observed through another link. Read-only still performs no writes, but it can read content placed there by someone else. **Instead of claiming data-origin or exclusive-object confinement**, state this aliasing limit in the contract and consent summary; applications needing isolated writable storage require a separately reviewed storage provider. A hostile process with the same OS privileges remains outside the sandbox claim. Reopen the provider design if the product requires continuously rooted namespace isolation or exclusive file identity, rather than hiding the stronger guarantee in “under a folder.”

[argued] Required plain-language consent text includes the substance: “Access follows the opened directory object. File-name restrictions do not prove file contents or unique ownership; a permitted file can share its contents with names elsewhere through hard links.” A private technical note is not enough. Adapt this text to read versus write rights while preserving the aliasing limitation.

### Windows feasibility prerequisite — accepted amendment B

[argued] Add a **v0.0.2 M0 feasibility record before M2 grant implementation is planned around this adapter**, analogous to Nexus X3-S2. Record the actual Windows/filesystem versions, APIs, access/share/create flags, driver boundary, threat model, observations, not-run cases and reproducible commands. Include successful allowed reads/writes and no-write checks for read-only grants, as well as intermediate/final reparse points, mount/junction traversal, actual-entry case and short-name aliases, non-truncating first open, exclusive creation, root replacement, fresh traversal after descendant removal and concurrent rename/reparse attacks. Verify the real opened object and output effects, not merely which API names appeared.

[argued] If the required contract cannot be supported, return unsupported-action and revise the adapter/milestone contract before implementation; never fall back to unchecked paths. An always-refusing adapter is safe but **does not pass** the useful file-program gate. This prerequisite records feasibility risk, not proof of all future filesystem behavior. The canonical record location belongs to the later authorized roadmap rewrite; no new file outside debate/ is created now.

## 4. Network policy

[argued] The first network policy describes structured HTTP requests, not arbitrary TCP/UDP packets. A complete rule correlates **origin, transport, allowed peers, methods and target selector**. Multiple rules are alternatives. A scheme/host/port tuple is never projected apart from its verbs/peers. The driver identity pins supported HTTP versions and resolver/TLS behavior; adding raw sockets, WebSocket/CONNECT tunneling, HTTP/3 or a proxy is a different typed operation/profile.

| Field | First form / meaning |
| --- | --- |
| Origin / protocol | [argued] Exact `http` or `https`, DNS name or IP literal, and numeric U16 port (1–65535). HTTPS authenticates the named origin; HTTP cleartext is an explicit distinct rule. HTTP protocol-version negotiation stays within the pinned adapter profile. No host glob or implicit suffix match. |
| Host normalization | [argued] ASCII DNS labels lowercased, validated lengths/characters, no trailing dot, userinfo, wildcard, percent encoding or empty labels. International names require a separately explicit, pinned IDNA conversion before policy construction; there is no implicit Unicode-host equivalence. Numeric hosts must be unambiguous IP literals, not legacy integer/octal address spellings. |
| Port / IP normalization | [argued] Materialize the default port (80/443); explicit default equals omitted default. IPs compare typed address bytes, not spelling. Prefixes have family plus masked network bytes and prefix length; reject a noncanonical prefix instead of silently broadening it. Normalize IPv4-mapped socket peers to IPv4 before checking; the policy builder rejects a second mapped-IPv6 spelling. Zone-scoped IPv6 is outside this first profile. |
| Peers | [argued] A required explicit set of exact IPs or CIDR prefixes per origin rule, possibly the issuer's expressly reviewed all-addresses choice. Empty denies all. No implicit “DNS answer means approved” or underspecified `publicInternet` classification. Parent policy/issuer approval may be broader than one name, but a child never exceeds it. |
| Methods | [argued] Closed method values for GET, HEAD, POST, PUT, PATCH, DELETE and OPTIONS. No implicit GET→HEAD permission and no assumption “read-like means harmless.” Wire method spelling is fixed and case-sensitive. TRACE, CONNECT and extension methods are unsupported at this gate. |
| Target selector | [argued] Explicit anyTarget, finite exact typed HttpTargets, or a **path-component prefix with an explicit exact-or-any query selector** (accepted amendment A). A target is path-component vector, trailing-slash bit, and **ordered** query pairs. Render once using a pinned UTF-8 percent encoder; reject path components `.`, `..`, slash, backslash, percent and controls. Query pairs may contain arbitrary scalar text, escaped by that encoder; duplicate/order semantics are preserved. No raw request-target override, URI fragment or hidden alternate parser. General URL globs/regex selectors remain deferred. |

[argued] HttpTarget components are nonempty; the root is the empty vector, rendered `/`, with its trailing-slash bit fixed false. Nonempty vectors render one slash between components and the explicit optional final slash. Encode only unreserved ASCII directly; percent-encode other UTF-8 bytes with uppercase hex. A nonempty query list renders `?` plus encoded key/value pairs separated by `&`, with `=` between each key and value; the empty list means no query delimiter. This deliberately cannot express empty path segments, a bare trailing `?`, arbitrary raw query syntax or alternate encodings of the same component. A URL import function either produces this typed form under the same rules or returns an error; it never changes parsers between authorization and sending.

[argued] Prefix uses the **same component-vector comparison helper** as filesystem policy. Prefix `["users"]` includes `/users`, `/users/` and `/users/123`, but excludes `/usersEvil`; trailing-slash distinction is enforced only by an exact-target rule. The empty prefix includes all supported paths and is shown as such in consent. QueryTest.exact compares the complete ordered query-pair vector, including duplicates; QueryTest.any deliberately allows all supported query values. Prefix and query selector stay correlated in one rule, never projected into independent global sets. This extends useful endpoint scoping without adding another matching language; it still does not prove the remote server uses identical path semantics.

[cited] [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html#section-9.1) specifies case-sensitive method tokens, and its [redirect section](https://www.rfc-editor.org/rfc/rfc9110.html#section-15.4) discusses changed targets/methods and sensitive fields. [RFC 3986](https://www.rfc-editor.org/rfc/rfc3986.html#section-6.2.2) distinguishes case, percent-encoding and path normalization. **Our narrower typed constructors are a proposed security profile**, not a claim that the standards require rejecting every broader URI form.

[argued] Default to **no automatic redirect**. A 3xx response is returned to the caller; following it is another explicit request, checked against a held grant. No ambient proxy, automatic cookie jar, inherited credentials, Host/authority override or cross-origin connection reuse bypasses the rule. Caller-supplied ordinary headers are permitted only through a validated representation; the adapter owns routing/Host, transfer framing, TLS server name and forbidden hop/tunnel controls. Explicit credentials are data in a separately reviewed request/credential binding; they are not a source of authority.

[argued] At each connection, resolve the permitted name through the pinned resolver as part of the declared network operation; filter the returned addresses against the rule's peer set and connect **the selected address**, not the hostname via a second resolver. Check the actual connected peer, origin port and TLS identity. All retry candidates must satisfy the same rule. Reused connections must still satisfy that grant/world/rule; sharing a pool is not permission sharing. Reject a DNS rebind to an unapproved address before contact. A policy that deliberately approves all peer addresses does not also promise “no local/private network contact.”

[argued] Exact HttpTarget is a guarantee about the emitted request target, not the server's routing, authorization or side effects. A server may decode/rewrite differently; GET may mutate state; query/body data can itself request server-side actions. Do not claim these policies prove remote data confinement. A trusted typed service interface may add application semantics and payload validation, with that implementation/assurance boundary visible. The scope mechanism supplies no information-flow guarantee.

### Concrete owner example

```text
anyOf([
  rule(origin = https api.example:443,
       peers = approvedPeerSetA,
       methods = [GET], targets = [exact healthTarget]),
  rule(origin = https metrics.example:443,
       peers = approvedPeerSetB,
       methods = [POST], targets = [exact metricsTarget])
])
```

[argued] Again this is schematic data, not added syntax. Omitted peers/methods/target selector is invalid, not wildcard. Metrics may be an optional bound `Option(NetSend)`: refusal to acquire it produces the reviewed None path, while the rest of the application proceeds. No optional status converts an invalid policy, driver mismatch or malformed credential into a successful acquisition.

## 5. Limits, delegation, actor policies and audit

[argued] Grant limits use closed counters whose units are part of the operation schema, initially invocation count and cumulative/per-invocation input/output **payload byte** caps; UTF-8 content is counted by encoded bytes. This is not a bound on OS metadata traffic, resolver traffic, CPU cost or wall time. Child limits can only add tighter bounds. Copies and sibling children share every inherited parent counter; their own tighter counters are additional. Reserve capacity before an action/transfer, account actual progress and do not refund ambiguous external consumption. Overflow denies/resolves as typed exhaustion, never wraps. Large/streaming reads must be bounded before allocation and transmission, not checked only after the full payload is received.

[argued] An ActorEndpoint may reuse the authority envelope and Boolean-policy machinery, but not these filesystem/HTTP leaves. Its rule names actor identity, typed protocol operation and optional closed payload predicates supplied by a reviewed schema adapter. It cannot inspect an arbitrary payload by running an author's Fn or reinterpret a path string as a host grant. Host-resource grant transfer across contexts needs broker-issued delegation, world/provider checks and shared counters; serializing a token or context offset is not transfer. Future actor operation types are specified with their kernel contract, not invented by O4's shared envelope.

[argued] Artifacts hash normalized **policy descriptions**, issuer/binding recipes, driver/registry versions and protected configuration dependencies. Runtime traces link these descriptions to run-local root/peer/grant projections and invocation/parent correlation. Do not hash physical handles or environment-specific live IDs into a portable semantic identity. Canonical VIBON projections contain no bearer token or authority-restoring decoder. Redact credentials/secret path details under a stated profile while retaining enough stable references for authorized inspection. No completed event is fabricated for a pre-dispatch refusal; separate dispatch-refusal events need their own versioned schema if exposed.

## 6. Acceptance and attack matrix

| Attack / case | Required outcome |
| --- | --- |
| Empty policy, malformed/unknown field or unsupported version | [argued] Empty alternatives deny all; malformed/unknown versions reject. Missing fields never become universal permission. |
| Restrict a narrow parent with a universal/broader request | [argued] Effective authority remains parent intersection; no arbitrary new root, peer, action or fresh quota. |
| Cross-product of A/GET and B/POST | [measured] Countermodel exposes the two extra requests; correlated rule evaluator denies both. Production test repeats it through driver dispatch. |
| Root `reports` versus sibling `reportsEvil` | [argued] Component comparison denies sibling. `..`, mixed separators, alternate streams, drive-relative paths and device aliases reject during typed path construction. |
| Exact-case suffix and aliases | [argued] `notes.txt` allowed; actual `notes.TXT` and `notes.txt.exe` denied. Case/8.3 aliases cannot make a disallowed actual entry pass. Add actual Windows cases, not only model strings. |
| Junction/symlink/mount inserted before or during traversal | [argued] No follow/escape; either authorized object traversal under the specified semantics or typed refusal. Demonstrate Windows and each supported adapter independently; include ancestor replacement and final-target races. |
| Root renamed and replaced at old absolute name | [argued] Existing grant stays attached to the acquired object; it never acquires the replacement. Already held descendant objects follow the explicit lifetime rule. |
| Descendant moved out between invocations | [argued] The next walk starts from the grant anchor; a cached intermediate directory must not preserve access. Pure sub does not create a new anchor. |
| Hard-linked permitted basename | [argued] Exposes the aliasing limit, not a false proof of unique data origin. Read grant still cannot write. Stronger isolated-provider requirements get separate tests and policy. |
| Network origin alias, alternate port, method, peer or target | [argued] Every field must satisfy the same whole rule. Mixed-origin/verb grants do not share uncorrelated sets. |
| HTTP `/users/123` versus `/usersEvil`; changed query | [argued] Component prefix allows the former and rejects the latter. Exact query rejects changed order, duplicates or values; any-query permission must be explicit. |
| DNS rebinding, redirect, proxy or Host override | [argued] Unapproved peer denied before connection; redirect returned; other routing bypasses rejected. Test actual connected addresses, not only parsed URLs. |
| Two children race a shared byte/count limit | [argued] Sum cannot exceed the parent reservation contract. Unknown delivery is not refunded as though nothing happened. |
| Serialized/counterfeit/stale/foreign-world grant | [argued] Does not restore authority. Invalid internal token is defect failure; legitimate out-of-scope request is typed denial. |
| Policy normalizer complexity limit | [argued] Structured incomplete/error result, no broadened result, no native stack overflow. Equivalent reordered allOf inputs normalize identically; semantic-equivalence solving is not required. |

[argued] These tests must run through interpreter and native adapters with equivalent fake worlds, followed by actual-platform boundary cases. The finite model is useful for formula semantics but cannot close the host boundary. Every unsupported platform/filesystem behavior is a visible gate gap; do not replace a required confinement case with a skipped success.

## 7. Proposed ledger replacement and residual scope

[argued] Replace O4/A-20's policy-form opening with: **closed typed policy data; correlation-preserving alternatives and conjunction; restriction by retained parent intersection; handle-bound rooted file scopes with exact basename tests; structured HTTP origin/peer/method/exact-target rules; shared broker limits; versioned normalization and projections; no authority from data decoding.** Keep the concrete aliasing/temporal/remote-service limits above visible. Membership is runtime checked; exact static resource-instance identity is still not provided.

[argued] Under protocol rule 9 these are my defaults, not new owner questions. Reopen on a needed policy outside the finite selectors, a platform adapter unable to meet the specified handle/name contract, or a product need for a stronger isolation guarantee than object-capability traversal. Before calling O4 **implemented**, require the attack matrix. Before calling it **design-closed**, Claude should explicitly accept/amend these forms and their limits; the canonical rewrite then carries the precise contracts and assigned implementation gates. No canonical spec, roadmap, frozen study or production code was changed for this proposal.
