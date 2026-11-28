# Interaction

The conventions of using a computer are older than they look, and most of
them are consequences of the machine underneath. Applications exist because
address spaces and byte files make silos. "Save" exists because there are
two memories. The progress bar exists because one processor is shared in
time and its work is opaque. The permission dialog exists because authority
is ambient and has to be questioned after the fact.

An Aurum machine running Vibra removes those causes. This note asks which
conventions survive, what replaces the others, and what a person can
perceive on such a machine that they cannot perceive on this one. It also
takes the charter at its word about who is typing: source "primarily
written and changed by coding agents". The environment has two users, a
person who decides and an agent who writes.

The [operating-system](operating-system.md) and [shell](shell.md) notes
supply the mechanisms. Everything here is a proposal, and the last sections
say where it is hard and how little of it needs the fabric.

## Theory

- **Direct manipulation.** The objects of interest are visible, and actions
  on them are rapid, incremental, and reversible [Shneiderman83].
- **Liveness.** Environments differ in how soon the system responds to a
  change in the program, up to responding continuously [Tanimoto90].
- **An interface is a pure function of state.** A display is a function
  from a value that varies with time to a picture, and an interactive
  program is a function from a state and an event to a new state
  [ElliottHudak97] [Elm13]. The prospectus adopts this shape for
  applications and for games.
- **The cost of an update should follow the size of the change.** That is
  the aim of incremental computation, in programs and in queries
  [SelfAdjusting02] [Adapton14] [DBSP23].
- **A view that can be edited is a lens.** A pair of functions, one that
  shows a value and one that puts an edited view back, with laws that make
  the pair behave [Lenses07].
- **Designating is authorizing.** The act by which a user indicates what to
  work on can be the act that grants access to it, so that security adds no
  step [Yee02].

## Conventions that lose their cause

| Convention | Its cause on a conventional machine | On Aurum |
| --- | --- | --- |
| Launching and quitting an application | A program is a process with its own address space, started from a file | A tool is a function. Applying it is the whole act, and nothing is left running that has to be quit |
| Saving | Working memory is volatile and separate from storage | Every state is a root in the [store](operating-system.md#the-store). The remaining act is naming a state worth keeping |
| File formats and "open with" | A file is bytes that one program understands | A document is a typed value. A format matters only at the border with other systems |
| The progress bar and the spinner | One processor, shared in time, doing work nobody can see into | The answer itself, filling in |
| The permission dialog | Ambient authority, questioned at the moment of use | Authority handed over as an argument, before anything runs |
| Undo, separately in each application | Mutable state that each program tracks its own way | One history: the revisions of the session and the roots of the store |
| "Not responding" | One thread, one heap, a trap anywhere | Failure is confined to a region, and a pure entry can be run again |
| Foreground, background, and priority | Turns on a processor | Room on the fabric |
| Installing | Global state and code loaded at run time | Admission of a checked package with written ceilings. Nothing global changes |

## What a person can perceive

Each property below follows from the machine or the language, and each
names what a person would see because of it.

### Shape instead of sequence

Inside a pure computation the fabric has no order of events, only
causality. A scrolling log of what happened is the wrong picture, because
nothing happened in an order. What a computation has is a shape: how much
work it is and how deep its longest chain of dependencies runs. A reduction
over a balanced tree is wide and shallow. A fold over a list is as deep as
it is long.

The environment can show that shape, and it is the most useful thing a
person can learn about a program on this machine, because depth is the
part that more room cannot shorten. Order exists only where the language
puts it, in the completed host events of the audit trace. So there are two
views, a shape for pure work and a timeline for effects, and they should
not be merged into one log.

### Answers fill in

A net's result can become available from the outside in: the constructor
at the root is in place before the parts beneath it have finished. A display
can therefore show a result as it forms, a record with some fields still
pending or a tree whose branches are still arriving. Progress is then
structural: a person sees which parts are done, where a percentage would
only estimate.

This is an observation by the environment and never by the program. The
program must still see strict evaluation, which the target note states as
the [completion rule](vibra-target.md#what-an-aurum-run-may-change):
nothing observable leaves a net before everything the interpreter would
have finished first has finished. Reading a region also costs something,
so the environment samples what is on screen and leaves the rest alone.

The spreadsheet is the precedent, and it is the one mass-market environment
in which people already program a dataflow machine without a program
counter [Nardi93].

### Room instead of priority

A person does not make something run sooner. They give it more of the
fabric. The honest indicator for a running entry has three numbers: how
much room it holds, how much it could use, and whether it is waiting for
room, for budget, or for the host. A physical map of the fabric is a
debugging view; for use, a share of the whole says enough.

The risk is that nobody wants to manage room, as nobody wants to manage
memory. The defaults have to be good enough that most people never touch
them, and whether they can be is unmeasured.

### Describing is not doing

A [plan](shell.md#three-kinds-of-entry) is a thing on the screen. It shows
what it would do, to which resources, within which limits, and it can be
[rehearsed](shell.md#history-rehearsal-and-undo) against a fork of the
user's data so that its consequence is visible as a difference. Then it is
run or it is not.

Preview is the default and commitment is the explicit act. Vibra's own
tools already work this way: "a mutating command previews unless `--write`
is given".

### Giving is the only way to permit

Dropping a folder onto a tool passes a grant, narrowed to that folder, as
an argument. No dialog appears, because the tool never had a way to reach
anything it was not handed. Each tool can be shown with two facts: its
ceiling, which says what kinds of operation it could ever perform, and
what it has been given.

Grants do not behave like data. A grant "cannot be written to a file,
compared, or used as a dict key". A document can be copied and sent; the
right to change something cannot be put in an envelope. An interface
should make the two look different.

### Nothing is lost, and anything can be asked why

Every value on screen has an origin the system can reconstruct: the entry
it came from, the witness identity of the function, the arguments, and for
anything that touched the world, the host events in the trace with their
source origins. "Why is this value what it is?" has a mechanical answer,
and so does "what would it be if this input were different?": fork, change
the input, compare. The Whyline showed how much people gain from asking
such questions of a running program [Whyline04]. Here the information is
kept by construction.

The cost is that everything is kept. The record of a session is a record
of what a person did. Retention, redaction, and secrets need a policy of
their own, and the prospectus says the same of caches.

### Numbers mean what they say

The [numeric domains](number-systems.md) are exact, so a number on screen
is the number. A decimal expansion is a view of it, computed to the
precision asked for, and equality is real equality in the domains where it
is decidable. This is not exotic: Android's calculator computes with exact
real arithmetic, decides equality in the common cases, and has more than a
hundred million users [Boehm20].

## Tools and views in place of applications

An application bundles data, functions over the data, a way to see it, and
the authority to touch it, behind one wall. On this machine the four come
apart.

- **A document is a value** in the store, with a type.
- **A tool is a function.** A pure one transforms values. A `proc` states
  its ceiling and takes its grants as arguments.
- **A view is a pure function from a value to a scene**, and an editable
  view is a lens [Lenses07]. Anything shown remembers the value and type it
  presents, so anything on screen can be picked up as the argument of
  another tool. The Lisp machines called these presentations [CLIM91]
  [Genera87].
- **What used to be an application is a package**: some types, some tools,
  and some views.

Updating a view after a small change should cost in proportion to the
change, and the prospectus plans an incremental dependency engine for it.
The fabric does not supply one by itself. A net consumes its input as it
reduces, so there is nothing left to
update unless a trace is kept, and keeping traces is the memory cost that
self-adjusting computation pays [SelfAdjusting02]. What the design does
give cheaply is incrementality at the grain of a function call, through
the memo table: if state is a tree and a view is computed per subtree, a
change recomputes one path. Finer grain is research.

The idea is old. Oberon, the Lisp machines, Smalltalk, the Canon Cat, and
HyperCard all dissolved the application to some degree, and none of them
displaced it [Oberon89] [Raskin00]. A recent survey of that history argues
that tools should be reshapable by their users and that machine-written
code changes the cost of doing so [Malleable25]. Two of the old obstacles
were technical: components in one address space could not be trusted with
each other, and there was no shared typed data underneath. Grants and the
store answer those two. The third obstacle was commercial, and nothing here
answers it; the prospectus lists commercial launch among its non-goals.

## A person and an agent

The loop for a task that no existing tool covers runs as follows.

1. The person says what they want.
2. The agent writes entries in the session: a tool, and a plan that applies
   it.
3. The checker gives its verdict. The environment shows the facts the
   [shell knows before anything runs](shell.md#what-the-shell-knows-before-anything-runs):
   the type, the ceiling, the grants the plan asks for, and where it will
   run.
4. The plan is rehearsed, and the person sees its consequence as a
   difference.
5. The person hands over the grants and commits, or does not.
6. The run is in the log, with its trace.

The person reads consequences and authority, and need not read code. The
language does not become natural language: the charter says Vibra "is not
a natural-language programming system", and what the agent writes is
ordinary checked source. The agent's interface is the toolchain's machine
contracts, and the person's is a rendering of the same facts, as the
tooling chapter requires: "An adapter MAY present the engine's facts
differently. It MUST NOT contain a second checker".

What makes a tool written a minute ago safe to run is not trust in its
author. It is that the tool cannot exceed its ceiling, cannot reach what it
was not given, can be rehearsed, behaves the same on every run, and stops
at its budget.

Current work on agents rebuilds these properties on top of conventional
systems. AgenticOS proposes replacing resource primitives with declared
intents and per-task manifests, and its authors present it as a conceptual
architecture [AgenticOS26]. Planarian adds snapshot, rollback,
and fork to an agent's environment with process checkpoints and a
copy-on-write file system [Planarian26]. In Vibra the manifest is the
`performs` row and the grant parameters, checked statically, and the
snapshot is a root and a position in the log.

Three risks are specific to this loop. A person who approves many plans
stops reading them. Rehearsal is only as faithful as the broker's model of
a resource. And a pure function can be wrong without being dangerous,
which contracts and tests address and authority does not.

## The interactive loop stays on the host

Keystrokes, pointer events, and pixels belong to the host and its graphics
hardware. The fabric computes the state and the scene. Each interaction
crosses the boundary twice, so the scene has to cross as a small
difference, and the measure that matters is the one the prospectus names:
the time from input to display.

## Other senses

A view is a function of a typed value, so a spoken view or a tactile one
is another view of the same value, with the same structure to navigate. On
a conventional system a screen reader has to reconstruct that structure
from what was drawn.

## Where it is hard

- **Room and budgets as something people manage.** Covered above.
- **Types change.** The store keeps old values decodable, and someone still
  has to write the function to the new type.
- **Rehearsal has edges.** Anything other people hold can only be held back
  or stood in for.
- **Vibra is verbose at a prompt.** The answer offered is an agent or an
  editor, and it may not be enough.
- **Everything is kept.** That is a privacy and storage problem before it
  is a feature.
- **Copying has a cost that used to be invisible.** Values are safe to
  share, but duplicating a large net is work, and people will find the cost
  surprising.
- **The byte world is not going away.** Every exchange with another system
  passes through explicit decoding.
- **History is against it.** Every environment without applications stayed
  small.

## What needs the fabric

Most of the above follows from the language and would work on today's
executors, hosted on an ordinary operating system.

| Idea | Needs |
| --- | --- |
| A session as a document and a log | The checker and the interpreter |
| Plans shown before they run; authority by argument | The effects chapter as written |
| The store, the memo table, and one history | Canonical values and the identities the tooling chapter defines |
| Replay and "why" | The audit trace, and a replay profile |
| Rehearsal | A world the user can select, which is a profile the shipped tools lack |
| Exact numbers | [Library types](number-systems.md#numeric-domains-in-vibra) |
| Answers that fill in | Any evaluator that can be observed part-way; the fabric makes it the normal case |
| Shape, and room in place of priority | A fabric, or its emulator |
| Pausing, moving, and putting away a running computation | A fabric, or its emulator |
| Pipelines with no processes; parallelism without asking | A fabric |

The first six rows are a project that needs no hardware and reports on
Vibra whatever happens to Aurum, as stages 1 and 2 of the
[path](README.md#a-path-that-leaves-the-roadmap-alone) do. The fabric adds
the last four, and those are the ones that are new to interaction design.

## What to measure

The prospectus proposes a text editor, a spreadsheet, and a compositor as
test applications, measured by input-to-display latency, update work,
history size, and crash recovery. Those carry over. The proposals in this
note add the following.

| Claim | Measurement | Fails if |
| --- | --- | --- |
| Shape helps people write parallel code | Speedup achieved on set tasks with and without the shape display | No difference |
| Structural progress beats a percentage | Decisions to wait or cancel, and their correctness | People do no better than with a progress bar |
| Authority by argument needs no prompts | Tasks completed with correct least authority, against a prompting baseline | People or agents hand over the widest grant by habit |
| People read rehearsals | Unwanted effects committed, as the number of plans per session grows | The rate climbs with volume |
| Function-grain incrementality is enough for a UI | Update work per edit in the editor and the spreadsheet | Latency grows with document size |

[Shneiderman83]: bibliography.md#interaction-live-environments-and-agents
[Tanimoto90]: bibliography.md#interaction-live-environments-and-agents
[ElliottHudak97]: bibliography.md#interaction-live-environments-and-agents
[Elm13]: bibliography.md#interaction-live-environments-and-agents
[SelfAdjusting02]: bibliography.md#interaction-live-environments-and-agents
[Adapton14]: bibliography.md#interaction-live-environments-and-agents
[DBSP23]: bibliography.md#interaction-live-environments-and-agents
[Lenses07]: bibliography.md#interaction-live-environments-and-agents
[Yee02]: bibliography.md#interaction-live-environments-and-agents
[Nardi93]: bibliography.md#interaction-live-environments-and-agents
[Whyline04]: bibliography.md#interaction-live-environments-and-agents
[Boehm20]: bibliography.md#interaction-live-environments-and-agents
[CLIM91]: bibliography.md#interaction-live-environments-and-agents
[Raskin00]: bibliography.md#interaction-live-environments-and-agents
[Malleable25]: bibliography.md#interaction-live-environments-and-agents
[AgenticOS26]: bibliography.md#interaction-live-environments-and-agents
[Planarian26]: bibliography.md#interaction-live-environments-and-agents
[Genera87]: bibliography.md#shells-and-composition
[Oberon89]: bibliography.md#shells-and-composition
