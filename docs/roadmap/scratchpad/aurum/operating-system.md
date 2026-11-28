# An operating system for the fabric

The prospectus sketches its first operating environment as a boot monitor,
an object store, an effect broker, and a shell, with scheduling, isolation,
drivers, networking, and graphics to follow. This note works out what those
parts are on a machine with no program counter and no addressable memory,
running a language with no ambient authority.

Its argument is that most of a conventional operating system exists to
supply properties that the machine and the language underneath it lack. On
a fabric that runs checked Vibra, isolation, authority, and determinism are
properties of the substrate. The operating system that remains has four
jobs: allot the fabric, guard the boundary, keep the store, and admit
programs. Ingalls's remark that an operating system is the collection of
things that do not fit into a language applies here with unusual force,
because so much fits [Ingalls81].

## Theory

- **Authority is reachability.** In the object-capability model a
  computation can affect only what it holds a reference to, and it obtains
  references only by creating an object or by being handed one
  [DennisVanHorn66] [Miller06]. A net has this property physically: a node
  acts only through its ports, and no rewrite rule produces a wire to a node
  the net was not given [Lafont90].
- **Protection and management are separable.** The lowest layer only has to
  multiplex the hardware safely; every policy can live above it
  [Exokernel95].
- **A deterministic machine is a function of its input log.** Recovery and
  replication then reduce to keeping the ordered inputs [Schneider90].
  Vibra's runtime is "deterministic for fixed source, inputs, and ordered
  host responses", so each program is such a machine, and the operating
  system has to keep that true when many of them run at once.
- **Persistence is orthogonal to type.** Any value may outlive the
  computation that made it, with no conversion code written for the purpose
  [AtkinsonMorrison95].
- **Sharing a surface is packing.** Allotting regions of a fabric to tasks
  that arrive and leave is online two-dimensional packing, and
  fragmentation is how it fails [Bazargan00].

## What an operating system is for

Each row takes one job of a conventional operating system, the property of
the machine that makes the job necessary, and what becomes of it here.

| Job on a conventional machine | Why it exists there | On the fabric |
| --- | --- | --- |
| Share a processor in time: scheduling, context switches, priorities | One program counter per core | There is no program counter to share. Every net reduces wherever it has an active pair. Tasks compete for area, links, and energy, so sharing is [allotment in space](#allotting-the-fabric) |
| Address spaces and virtual memory | A program can compute any address | There are no addresses. A net reaches only what it is wired to, so two nets with no wire between them cannot affect each other's values |
| System calls and a privileged mode | Ambient authority has to be gated somewhere | The fabric cannot perform a host operation at all. An effect is a request node that carries a grant to the [broker](#the-boundary) |
| Files: names and persistence over bytes | Memory is volatile and untyped | A [store](#the-store) of typed canonical values. A name is an entry in a dictionary that someone holds |
| Processes | One unit is needed for isolation, accounting, and failure | A [task](#the-unit-of-work): a region, a world, and a budget |
| Pipes and other interprocess communication | Address spaces are separate | A wire, inside one failure domain; a host operation across domains |
| Signals and kill | A process has to be stopped from outside | Erase the task's root wire and reclaim its region |
| Loading and dynamic linking | Code arrives as untyped bytes | Admission of a checked program under a profile. Nothing is linked at run time |
| Device drivers | Hardware differs | Unchanged, and off the fabric: drivers stay behind the broker |

Three jobs have no conventional counterpart, and all three belong to
allotment: deciding where a net sits, because wire length is latency;
keeping erasure from being starved, because reclamation competes with
computation; and throttling a net that would otherwise outgrow its region.

The isolation in the second row is a property of valid nets. The runtime
chapter says the same of source: "Capability confinement in source is a
contract of trusted compilation together with reviewed issuers, runtime,
and adapters". A fabric therefore needs the net-IR verifier at admission,
as Singularity relied on verified code where other systems rely on hardware
address spaces [Singularity07], and it needs a cheap hardware check at
region boundaries so that a faulty element cannot write outside its own
region. Neither is information-flow security. Tasks share links and power,
so timing and occupancy channels remain, and the effects chapter already
says that Vibra "does not track information flow".

## Two halves

The [target note](vibra-target.md#effects-and-authority-on-the-fabric)
keeps the broker as conventional software on a conventional core, and the
prospectus allows a host controller for debugging and physical input and
output. An Aurum computer is then two unlike machines, and its operating
system is closer to a small distributed system than to one kernel, which
is the multikernel's view of any heterogeneous machine [Barrelfish09].

| Part | Runs on | Job | In the specification today |
| --- | --- | --- | --- |
| Fabric monitor | The fabric and its controller | Allots regions, keeps the free-node pools, guarantees erasure, counts work, and halts a faulty region. Mechanism only | No |
| Broker | The host | Performs host operations for grants, enforces policy and limits, orders completed events, and writes the audit trace | Yes: the broker of the [runtime chapter](../../../spec/06-runtime.md#reclamation), which with the kernel owns "the live resource and accounting objects that sealed tokens refer to" |
| Store | The host and its storage | Keeps canonical values by content, the roots that name them, and the memo table | The value encoding and the identities exist. A store does not |
| Admission | The host | Checks a program against a profile, installs its function templates, and refuses what the profile cannot run | Yes: [admission](../../../spec/06-runtime.md#native-execution) and `@tool.unavailable` |
| Shell | The host, with its entries evaluated on the fabric | The user's root context | No; see [the shell note](shell.md) |

The lowest layer is called a monitor here because the runtime chapter
already uses "kernel" for the native runtime's own.

The charter excludes dynamic loading, runtime plugins, user-defined host
providers, permission prompts, and "a language-level budget for time,
memory, or host operations". None of the parts above needs any of them. The
operating system is a set of tools around the toolchain, in the way
`vibra run` is, and nothing here becomes something a Vibra program can
name.

## The unit of work

A **task** is what a process becomes. It has four parts, and each one
already has a definition.

- **A net in a region.** The region is the set of processing elements the
  monitor allotted. The net cannot grow outside it. When the region's node
  pools are empty the task ends with memory exhaustion, which the language
  already treats as a host event, and no other task is disturbed.
- **At most one sequencing wire.** A pure task has none. It is a
  [kernel-mode](vibra-target.md#three-ways-to-use-the-fabric) evaluation,
  and "pure evaluation has no provider at all".
- **A world.** The world is the set of grants issued for the task. "A grant
  from another world, or one released at shutdown, is unusable", so the
  world is also the unit of revocation: ending a task withdraws everything
  it was given. The effects chapter excludes revoking a single grant, and
  this design does not need it.
- **A budget.** "Every executing command runs under published, versioned
  limits on work, memory, and output", and reaching one is an unknown
  outcome that is never a program result. On a fabric the counters are
  rewrites, nodes, area, and energy. Grant limits already nest, since
  "every copy and descendant of a grant draws on the same ancestor
  counters, which the runtime's broker owns", and task budgets can nest the
  same way. KeyKOS did this for processor time and storage with meters and
  space banks [KeyKOS92], and ECOSystem did it for energy [ECOSystem02].

Whether a task has a sequencing wire is the distinction the operating
system uses most, because it decides what may be done to the task without
asking anyone.

| The operating system may | A pure task | A task with effects |
| --- | --- | --- |
| Stop it at any moment | Yes; nothing outside it changed | Yes, but completed host operations stay done, and one may be `outcomeUnknown` |
| Run it again | Yes, with the same result | Only as a new run: each `await` is a fresh invocation |
| Run it twice and compare, or run it early in case it is wanted | Yes | No |
| Answer from a stored result | Yes | No: a plan "is never a consumed ticket and never a memoized result" |
| Move it to another region | Yes | Yes, inside one failure domain; the broker keeps its world |

## Allotting the fabric

**What is allotted.** Processing elements, the node pools inside them, link
capacity, and energy. Time is not allotted, because nothing waits for a
turn.

**Two numbers describe a task.** Its **floor** is the number of live nodes:
the net does not fit in less. Its **width** is the number of active pairs:
more elements than that cannot be used. A conventional scheduler has to
guess how much parallelism a process could exploit. Here it is a counter
the monitor can read, and both numbers are among the metrics the prospectus
already asks the emulator to record. A policy follows directly: every task
gets its floor, and what remains is divided by width and by share. Whether
that policy is good is unmeasured.

**Space trades for time, and nothing else changes.** A pure net given fewer
elements than its width holds its active pairs in queues and finishes
later with the same result, because the calculus is confluent. Squeezing a
task is therefore always safe, which is not true of memory on a
conventional machine.

**Static-fragment tasks are the predictable class.** A task in the
[static fragment](vibra-target.md#two-fragments) has storage fixed before
it runs. Its region is reserved exactly, it never allocates, it needs no
erasure, and its latency can be bounded from its circuit. Net-fragment
tasks are elastic and best-effort. The two profiles of the target note are
also two service classes.

**Throttling.** A net reduced everywhere at once can expand breadth-first
and exhaust a region that a depth-first order would have fitted in. The
dataflow machines met this and added explicit control of parallelism
[RuggieroSargeant87] [CullerArvind88]. The monitor needs the same control:
as a region fills, it prefers interactions that shrink the net. This is
the [erasure guarantee](vibra-target.md#where-the-runtime-chapter-assumes-one-worker)
of the target note seen from the allocator's side.

**Fragmentation and migration.** Tasks finish out of order and leave holes.
Three results from reconfigurable hardware bound what to expect.

- Sharing pays. AmorphOS reports up to 4 and 23 times the aggregate
  throughput on two FPGA platforms from protected sharing with tasks that
  change their deployed size [AmorphOS18]. BORPH and Coyote are the cases
  for giving hardware tasks ordinary operating-system abstractions
  [BORPH08] [Coyote20].
- Migration pays when state moves with the task. Mestra shares a
  coarse-grained array among kernels. In FPGA emulation, sharing cut
  makespan by up to 70.48% against running one kernel at a time; mean wait
  fell by 91.39% while mean execution time rose 3.42 times from contention
  for memory bandwidth. On fragmented layouts, in simulation, moving
  running kernels with their state cut tail latency by up to 29.60%, and
  moving them without state gained under 3% or lost. The controller and
  read-back paths cost 0.13% of the device's lookup tables per region
  [Mestra26].
- Reading state back is the hard part on hardware not built for it. EPOCH
  saves and restores one FPGA configuration frame in 62.2 and 67.4
  microseconds, and had to reverse-engineer the bitstream format to do so
  [EPOCH25].

A net fabric is better placed than either. Its state is the net, which is
self-describing and which the net IR must be able to serialize anyway. So
checkpoint, migration, and suspension to the store are one mechanism: stop
firing in the region, let messages in flight arrive, read the net out. The
cost of reaching that quiescent point across many elements is unmeasured.

**Locality.** The [execution-model note](execution-models.md#spatial-scale)
records that communication distance and load imbalance limit spatial
scaling [Tascade23]. Two rules follow as conjectures: keep a task's net
compact, and place tasks with heavy host traffic near the edge where the
broker's ports are.

## The boundary

The broker already owns grants, policies, limits, and the audit trace. As
an operating-system service it gains one duty: it serves many worlds at
once.

**The only arbitration in the machine is here.** Each task with effects
has its own sequencing wire, and the broker merges their requests. Inside
a task, order is causality in the net. Between tasks, the merge order is a
choice, and the broker records it. Determinator showed that an operating
system can enforce determinism on parallel programs by giving them
shared-nothing spaces and deterministic communication [Determinator10].
Here the pure part is deterministic by confluence, so the record holds host
events only, and its size is proportional to input and output and not to
computation.

**The audit trace is nearly that record.** Each `@host` event of the
[trace](../../../spec/06-runtime.md#audit-trace) holds the operation, its
arguments, and "the canonical data of the operation's complete `Result`
value". Replaying a task means feeding those results back in order. It
never dispatches an operation again: all seventeen standard operations are
`@once`, and "`@once` is not exactly-once delivery: there is no
deduplication across a crash and no rollback". A trace with a redaction
profile applied cannot be replayed.

The runtime chapter excludes "universal journaling and recovery", and "the
shipped launcher has no user-selectable world or replay switch". Replay is
therefore a profile of its own, as the conformance harness is, and never a
promise of the language.

**Exhaustion is recorded as an ending.** Memory exhaustion and budget
limits depend on timing on a fabric. They are host events and unknown
outcomes already, so a replay reproduces the completed events and then the
ending, without pretending the ending had a position in the program.

**What crosses.** Values cross in the
[canonical encoding](../../../spec/06-runtime.md#canonical-value-encoding).
"A function, a procedure, a plan, and a sealed authority have no value
encoding", so none of them crosses as a value: a grant crosses as an opaque
index into the broker's table, and a function stays on the fabric as a
template named by its identity. Every crossing costs a conversion, and a
task that talks to the host constantly belongs on the host.

**Drivers, display, and input stay on the host**, as the prospectus
proposes for the first environment.

## The store

**Values by content.** The store keeps canonical values under the hash of
their canonical bytes, as Venti kept blocks [Venti02]. The specification
already identifies source by content: the workspace revision and the claim
and witness identities are SHA-256 hashes. Nix did the same for software
deployment and Unison for definitions [Nix06] [Unison].

**Names are values too.** A directory is a `Dict`, and what a user owns is
a root value. An update makes a new root that shares everything unchanged,
so a snapshot is a root and history is a list of roots. No namespace is
global: a task sees the dictionary it was handed, as a Plan 9 process sees
the namespace it was built [Plan9Names92]. Publishing a root is the one
mutable cell in the design, and it has to be atomic and durable. The
prospectus is right that immutable state does not supply durable
transactions.

**Values are the truth; images are a cache.** A suspended task is a net,
and it may hold functions and plans, which have no encoding. It is saved
as a **net image**, tied to the witness identities of its program and to
the net-IR version. An image saves work and is never the only copy of
anything. This keeps the tooling chapter's rule that "there is no staged
codebase, no live redefinition, no publication step, and no second
representation of source". The orthogonally persistent systems made the
other choice and persisted memory itself: KeyKOS in production, with a
single-level store and system-wide checkpoints [KeyKOS92]; EROS [EROS99];
and Twizzler for non-volatile memory, whose benchmark runs were 1.1 to 2.9
times faster than on native and NVM-optimized SQLite backends
[Twizzler20]. The store here persists values so that nothing depends on
yesterday's layout.

**The memo table.** A pure call is identified by the witness identity of
the function and the hash of its canonical arguments. The witness is the
right key: "a change to the body of anything a declaration resolves to,
however far, changes the declaration's witness". The toolchain identity is
part of the key, as the runtime chapter requires of compile-time
evaluation: "cache keys include the code, type, registry, and profile
identities, and never a process-specific authority token". This is the
complete input identity the prospectus asks of memoization. With it, the
store and the memo table form a build system in the exact sense of the
build-systems literature [BuildSystems18], and the tooling chapter already
allows it: "A content-addressed cache MAY hold derived results. Deleting it
MUST NOT change source, approvals, identity, or execution".

**Where hashing happens.** Interning every node as it is built needs one
global table, which is the non-local structure the fabric exists to avoid.
The recommendation is to hash at the boundary, when a value leaves for the
store. Hashing is linear in the value, so memoization is for coarse calls,
and its hit rate has to be measured, as the prospectus insists.

**When a type changes.** Typed stores usually die of this. A canonical
value carries its type description, and a declared type's identity is
nominal, so an old value stays decodable as the old type. Moving it to the
new type is an ordinary Vibra function. The store never reinterprets bytes.

**Bytes at the border.** Files and network data from other systems enter
as `Bytes` or `Str` through a grant and are decoded explicitly. Living
beside the byte world is permanent, not a transition.

## Faults and recovery

| Fault | What the task sees | Recovery |
| --- | --- | --- |
| A region fills | Memory exhaustion, a host event | Rerun with a larger region; a pure task loses nothing |
| A budget is reached | An unknown outcome | "A scheduler quota only suspends and resumes", so the task waits for more budget |
| A host operation fails | A typed `Result` | The program's own handling |
| An element computes wrongly | Nothing, if it is caught: a net has local invariants, such as conservation of ports and well-typed active pairs, that an element can check as it rewrites | Halt the region; replay from the last root and the record |
| The toolchain is wrong | One of the two defect traps | None; it is a defect |
| Power is lost | The task is gone | The last published root, the record, and the memo table |

Determinism is what makes the fourth row cheap. A pure task can be run
twice on different elements and its canonical results compared, which
detects a faulty element without any model of how it fails.

## More than one machine

A confluent net is indifferent to delay, which is the latency-insensitive
property [Carloni01], so a wire could cross a network. It is not
indifferent to loss. Partial failure is what location transparency has
always hidden badly [Waldo94]. The rule proposed here is that a wire never
crosses a failure domain. Across domains, work moves as a host operation
that returns a `Result`, and `outcomeUnknown` is one of its cases. A pure
call, named by its memo key, is the natural unit to send: the receiver
needs nothing but the key, and the sender can check the answer by running
it again elsewhere.

## What is missing

- Several worlds under one broker, with the merge order recorded. Today a
  run has one world.
- A replay profile over the audit trace.
- A limit profile for the fabric: area, rewrites, nodes, and energy.
- A store format, a root-publication protocol, and a memo table.
- A net-image format.

The first four need no fabric. They can be built and measured on the
interpreter, hosted on an ordinary operating system, and the
[interaction note](interaction.md#what-needs-the-fabric) separates what the
fabric adds.

## What to measure

| Claim | Measurement | Fails if |
| --- | --- | --- |
| The record of host events is enough to replay a session | Replay recorded sessions on the interpreter and on the emulator under varied schedules | Any outcome depends on something that is not a host event |
| Width is a usable allotment signal | Emulated mixes of tasks, allotted by width against equal shares | Throughput and tail latency are no better than equal shares |
| Throttling keeps peak nodes near the interpreter's peak | Peak live nodes per program, throttled and unthrottled | Programs that fit on the interpreter exhaust a region of comparable size |
| Reading a net out is cheaper than rerunning it | Time to quiesce and serialize against time to recompute, by task size | Stateless restart wins for typical tasks, the opposite of [Mestra26] |
| Boundary hashing pays | Memo hit rate and hashing time on the tooling workloads the prospectus lists | Time spent hashing exceeds recomputation saved |

[Ingalls81]: bibliography.md#operating-systems-capabilities-and-persistence
[DennisVanHorn66]: bibliography.md#operating-systems-capabilities-and-persistence
[Miller06]: bibliography.md#operating-systems-capabilities-and-persistence
[Exokernel95]: bibliography.md#operating-systems-capabilities-and-persistence
[Schneider90]: bibliography.md#operating-systems-capabilities-and-persistence
[AtkinsonMorrison95]: bibliography.md#operating-systems-capabilities-and-persistence
[Singularity07]: bibliography.md#operating-systems-capabilities-and-persistence
[Barrelfish09]: bibliography.md#operating-systems-capabilities-and-persistence
[KeyKOS92]: bibliography.md#operating-systems-capabilities-and-persistence
[EROS99]: bibliography.md#operating-systems-capabilities-and-persistence
[ECOSystem02]: bibliography.md#operating-systems-capabilities-and-persistence
[Determinator10]: bibliography.md#operating-systems-capabilities-and-persistence
[Twizzler20]: bibliography.md#operating-systems-capabilities-and-persistence
[Venti02]: bibliography.md#operating-systems-capabilities-and-persistence
[Nix06]: bibliography.md#operating-systems-capabilities-and-persistence
[Unison]: bibliography.md#operating-systems-capabilities-and-persistence
[BuildSystems18]: bibliography.md#operating-systems-capabilities-and-persistence
[Waldo94]: bibliography.md#operating-systems-capabilities-and-persistence
[Plan9Names92]: bibliography.md#shells-and-composition
[Bazargan00]: bibliography.md#sharing-reconfigurable-and-dataflow-hardware
[RuggieroSargeant87]: bibliography.md#sharing-reconfigurable-and-dataflow-hardware
[CullerArvind88]: bibliography.md#sharing-reconfigurable-and-dataflow-hardware
[AmorphOS18]: bibliography.md#sharing-reconfigurable-and-dataflow-hardware
[BORPH08]: bibliography.md#sharing-reconfigurable-and-dataflow-hardware
[Coyote20]: bibliography.md#sharing-reconfigurable-and-dataflow-hardware
[Mestra26]: bibliography.md#sharing-reconfigurable-and-dataflow-hardware
[EPOCH25]: bibliography.md#sharing-reconfigurable-and-dataflow-hardware
[Lafont90]: bibliography.md#interaction-nets-optimal-reduction-and-their-implementations
[Carloni01]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[Tascade23]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
