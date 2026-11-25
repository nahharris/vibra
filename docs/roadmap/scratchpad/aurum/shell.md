# A shell for the fabric

A Unix shell does four things: it launches programs, it names things, it
glues programs together with pipes, and it controls jobs. Each assumes the
conventional machine. Launching assumes a process to create, naming assumes
one global tree of byte files, pipes assume that programs share nothing but
bytes, and job control assumes that programs take turns on a processor.

This note redoes the four for the [operating system](operating-system.md)
of the previous note. Its position is that the shell needs no language of
its own and almost no mechanism of its own. The command language is Vibra,
a session is a document and a log, and the toolchain's existing machine
interface is already most of a shell.

## Theory

- **A pipeline is a Kahn network.** Processes joined by unbounded
  first-in-first-out channels compute one result whatever the scheduling
  [Kahn74]. Unix pipes made that model ordinary [RitchieThompson74].
- **Least authority at the command line.** A script should receive exactly
  the capabilities its caller hands it, and be unable to make or keep any
  other [Shill14]. Ambient authority is what turns a helpful program into a
  confused deputy [Hardy88].
- **A set of definitions with cached results is a build system.** What is
  rebuilt after a change is decided by the dependency graph and a record of
  inputs [BuildSystems18].
- **An effect that cannot be rolled back can sometimes be compensated.** A
  long activity is a sequence of steps, each paired with a step that undoes
  it [Sagas87].
- **Liveness and effects pull against each other.** An environment that
  reruns a program as it is edited is safe only when rerunning is harmless
  [Tanimoto90] [ShellVis26].

## One language

The command language is Vibra. A command is an expression, a script is a
module, and a tool is a function.

The charter gives the reasons. It prefers "one canonical way to express an
idea, built from shapes the language already has", and it describes source
as "primarily written and changed by coding agents". A second, terser
language for interactive use would be a second thing for an agent to get
wrong, checked by a second and weaker checker, and the tooling chapter
rules that out: "No tool uses a weaker checker for snippets".

The precedents are the systems that had one language all the way down. In
Oberon, any exported procedure is a command, activated from text anywhere
on the screen [Oberon89]. The Lisp machine's listener evaluated the system's
own language [Genera87]. PowerShell and Nushell kept a separate command
language and replaced byte streams with typed values, which is the half of
the idea that became mainstream [Monad02] [Nushell].

The cost is that Vibra is verbose to type: `U64.add(1u64 2u64)` where a
calculator wants `1+2`. The recommendation is to add no sugar. The typist
is usually an agent, and for a person the remedy belongs in the editor:
completion from types, and typed holes that keep an unfinished entry
checkable [Hazel19].

## A session is a document and a log

The specification rules out the usual design. "Eval is not a REPL and
redefines nothing", and for source in general "there is no staged codebase,
no live redefinition, no publication step". A shell cannot be a prompt that
mutates an environment of bindings.

It can be two things the specification already has.

- **The document** is a Vibra module in a project. Its entries are
  declarations: a `let` for a value, a `fn` or a `proc` for a tool. Editing
  the session edits source, each edit is a new workspace revision, and the
  session's history is the history of that file.
- **The log** is the ordered list of runs: which entry was run, with which
  bindings and arguments, and the audit trace it produced. It is
  append-only, because what was done to the world cannot be edited.

```vibra
fn double
  takes {count: U64}
  returns twice: U64
  do
    return U64.add(count count)
endfn

let answer: U64 be double(21u64) endlet

fn prepareNote
  takes {writer: DirWriteText at: RelativePath contents: Str}
  returns plan: Intent(Result(Void FsError) {DirWriteText})
  do
    return DirWriteText.write(writer at contents)
endfn
```

`answer` is a value entry, and the shell shows `42u64` beside it. `double`
and `prepareNote` are tools. Nothing in the document can write a file,
because no source expression makes a grant.

This is a spreadsheet or a reactive notebook, with the dependency graph the
compiler already computes. A witness identity pins a declaration and
everything it resolves to, so an edit changes the witnesses of exactly the
entries that depend on it, and those are the entries to evaluate again.
Everything else is answered from the [memo table](operating-system.md#the-store).
A session has no hidden state and no order of evaluation to remember, which
are the two things that make a long REPL session unreproducible.

The shell changes the document the way every tool does, through a
[transactional edit plan](../../../spec/05-tooling.md#transactional-edit-plans):
"a mutating command previews unless `--write` is given".

## Three kinds of entry

The language's split between `fn`, `proc`, and plan decides how the shell
treats an entry.

| The entry is | What the shell does with it |
| --- | --- |
| A pure value | Evaluates it at once on the fabric, shows it, and stores it. It can be stopped, rerun, or computed early without asking, because nothing outside it can change |
| A tool: a `fn` or a `proc` | Shows its signature, contracts, and written ceiling. It is something to apply |
| A plan: an `Intent` built by applying a `proc` to arguments and grants | Shows it as pending. Its type carries its complete row, and its grants have projections that say which resource and policy they cover. Nothing has happened: "Building a plan MUST NOT enter the body, run a guard that has an `else`, dispatch a host action, or consume any limit" |

Running a plan is a separate, explicit act that appends to the log. The gap
between describing an action and performing it is in the language, where
other systems add it afterwards: a plan-and-apply step in deployment tools,
`try` for single commands [Try], an overlay under a live editor
[ShellVis26].

## What the shell knows before anything runs

Every fact in this table comes from the checker or from a
[workspace query](../../../spec/05-tooling.md#effect-and-authority-projections),
and none needs the entry to be run.

| Question | Source of the answer |
| --- | --- |
| What type is the result, and what must hold of the arguments? | The signature and its contracts |
| What kinds of operation could this perform at all? | The written ceiling and the performed row |
| On which resources, within which limits? | The grant projections at that position, which state "explicitly, what is unknown" |
| Can a retry repeat an operation? | The delivery class of each operation it reaches |
| Will it run on the fabric? | The [availability fact](vibra-target.md#reporting-what-is-unavailable) of each function: its fragment, or the reason it stays on the host |
| Has this been computed before? | The memo key |

Two more facts appear while an entry runs: its floor and width, from the
[monitor](operating-system.md#allotting-the-fabric), and its work and
depth. The prospectus bounds parallel time from below by the larger of
work divided by elements and depth. An entry whose depth is close to its
work is a chain, and the shell can say that more room will not make it
faster. That is the lesson of [tree-shaped code](vibra-target.md#parallelism-the-programmer-has-to-write)
delivered at the moment it matters.

## Authority at the prompt

**Where authority starts.** "A binary target names an entry and states, as
data, everything the program is allowed to do": an `effects` ceiling and a
dictionary of binding recipes, each with a provider, a resource, a policy,
and limits. A session is a project, so what the session may touch is that
data. Adding a directory to a session is an edit to the project file, made
by a previewed plan, and "a plan that … changes a policy without approval
fails with nothing written".

**How it reaches a tool.** By argument, narrowed first, as in this line
from the effects chapter:

```vibra
let onlyReports: DirReadText be try DirReadText.sub(notes try RelativePath(["reports"]))
endlet
```

A tool given `onlyReports` can read below `reports` and nothing else. The
caller hands over authority by naming what to work on. This is Shill's
model with the checking moved to compile time [Shill14], and it is why the
charter can exclude permission prompts: nothing ever has to ask.

**What does not exist.** No command reads a path the user did not pass, no
tool inherits the session's reach, and no `sudo` exists because there is no
ambient level to rise to. "A permission makes no grant, widens no resource
scope, and replaces no target approval".

**Withdrawal.** A grant cannot be revoked singly. Ending the run ends its
world, and "a grant from another world, or one released at shutdown, is
unusable".

## Pipes are wires

Composition is function application. `c(b(a(x)))` is a pipeline, and on the
fabric it is one net in which the three stages reduce at the same time with
no process boundary between them. The values between stages are typed, so
no stage prints and no stage parses.

PaSh shows what the byte-stream shell pays for lacking this. It turns
scripts into dataflow graphs and parallelizes them, for speedups of 0.89 to
61.1 times, 6.7 on average, over 44 scripts. To do it, someone must
annotate each command by hand with how it treats its input, because a
command is an opaque program; the system parallelizes nothing it lacks an
annotation for; and the largest class of commands is the side-effecting
one, which it cannot parallelize at all [PaSh21]. A Vibra `fn` carries that
annotation in its kind.

Three limits keep this honest.

- **A pipeline over a `List` is still a chain.** The stages overlap, but
  each walks its input in order. Parallelism inside a stage needs
  [tree-shaped data](vibra-target.md#parallelism-the-programmer-has-to-write)
  [Steele09]. PaSh's gains come from splitting inputs and merging results
  with per-command aggregators, which is the same requirement in another
  form.
- **Effects sit at the ends.** A pipeline reads through a grant, transforms
  purely, and writes through a grant. Only the middle is on the fabric.
- **Streams from the outside are not in the language yet.** The effects
  chapter excludes streaming operations, so a pipeline over unbounded input
  is a recursive `proc` that reads a bounded count at a time.

## Jobs without time

There is no foreground and no background, because every entry reduces at
once. What the user controls is attention, room, and lifetime.

| Verb | What happens | Safe because |
| --- | --- | --- |
| Show, hide | The shell starts or stops reading an entry's region | Observation is outside the program |
| Give room, take room | The monitor changes the entry's allotment | A squeezed net is slower and computes the same result |
| Pause, resume | The region stops firing, or starts again | An idle net is inert data |
| Put away | The net is read out to the store as an image | An image is a cache |
| Drop | The root wire is erased and the region reclaimed | A pure entry changed nothing outside itself |

A budget that runs out pauses an entry; it does not fail it. "A scheduler
quota only suspends and resumes; it is never a program result", and the
shell shows the entry as waiting for room or for budget.

Dropping a run that has effects is different. Every completed host
operation stays done, the trace says exactly which, and the one in flight
may be `outcomeUnknown`. The tooling chapter already says so for its own
server: "A disconnect or cancellation is an explicit incomplete outcome. It
does not show that an action already dispatched did not happen".

Divergence cannot be detected. What the shell can show is an entry that
keeps rewriting and emits nothing.

## History, rehearsal, and undo

**Pure work is undone by editing.** The previous revision of the document
has its values in the memo table.

**Effects are not undone by going back.** The prospectus says that rolling
graph memory back does not undo an external effect, and the runtime chapter
forbids presenting a release as "rollback of an external effect already
dispatched". What a shell can offer honestly comes in three kinds.

| Kind | Applies to | How |
| --- | --- | --- |
| Rehearse | Resources the store holds | Run the plan against a fork of the world. Forking a root costs nothing, and the result is an exact difference between two roots. Test worlds work this way already: "Effects change that copy only" |
| Compensate | Operations with a declared inverse | Record the inverse beside each operation and run the inverses backwards [Sagas87]. Planarian does this for agents on a conventional system: it proxies remote calls, records compensating actions, refuses a request it cannot make compensable, and adds 1% to 3% to completion time [Planarian26] |
| Hold | Everything else | Queue the operation until the user commits it, and rehearse with a stand-in result. ShellVis proposes this for network effects and has not tested it [ShellVis26] |

The first row is why the design pushes state into the store. A change to
the user's own data is a new root, and undoing it is publishing the old
one. Only operations on things other people hold need the second and third
rows.

Rehearsal changes how a live environment feels. ShellVis ran shell scripts
continuously against an overlay of the file system. Its seven participants,
with one to twenty years of shell experience, all found it helpful, and
they reported feeling safer [ShellVis26]. It sandboxes only the file
system. Here every effect passes through the broker, so the same treatment
covers everything the broker can model.

**Speculation.** hS runs shell commands out of order in isolation and rolls
back the ones that conflict [hS23]. For pure entries on the fabric, running
early is safe by construction, and its only cost is room. The shell can
spend idle elements on the entries most likely to be wanted next.

## What exists and what is missing

The toolchain already has an agent's shell. `vibra mcp` serves `query`,
`eval`, the edit plans, `test`, and `run` over one workspace, reads only by
default, and gives no tool authority by being allowed to run it. The Aurum
shell is that interface with a session, a store, and a fabric behind it.

| Need | Status |
| --- | --- |
| Evaluate a pure entry | `eval` does this for one function and checked arguments, against a fresh snapshot |
| Run an effect under stated authority | `run`, with the target's recipes |
| Return a typed value from a run | Missing. An entry returns `ExitCode` only, so a run can report a value only by writing it somewhere |
| Rehearse a run | Missing from the shipped tools: "the shipped launcher has no user-selectable world or replay switch". The harness profile has the mechanism |
| Several live entries at once | Waits for the concurrency release |
| Interactive input and streams | Not in the operation inventory |
| Session, store, memo table, log | Missing; none needs a language change |

## What to measure

| Claim | Measurement | Fails if |
| --- | --- | --- |
| A session document stays reproducible | Re-evaluate recorded sessions from the document alone | A value depends on the order of past edits |
| Rehearsal is faithful | Compare rehearsed and real differences for runs over stored data | They differ for any modelled resource |
| Preview does not slow the user down | Time and errors on file-management tasks, with and without rehearsal, for people and for agents | Tasks take longer with no fewer destructive mistakes |
| Verbosity is tolerable | The share of entries written by hand, and their error rate | People avoid the shell for small tasks |
| Early evaluation pays | The share of speculated entries later used, against the room spent | Most speculation is discarded while wanted entries wait for room |

[Kahn74]: bibliography.md#dataflow-spatial-and-statically-scheduled-machines
[Steele09]: bibliography.md#resources-ownership-and-reuse
[BuildSystems18]: bibliography.md#operating-systems-capabilities-and-persistence
[Hardy88]: bibliography.md#operating-systems-capabilities-and-persistence
[Sagas87]: bibliography.md#operating-systems-capabilities-and-persistence
[RitchieThompson74]: bibliography.md#shells-and-composition
[Shill14]: bibliography.md#shells-and-composition
[Oberon89]: bibliography.md#shells-and-composition
[Genera87]: bibliography.md#shells-and-composition
[Monad02]: bibliography.md#shells-and-composition
[Nushell]: bibliography.md#shells-and-composition
[PaSh21]: bibliography.md#shells-and-composition
[Try]: bibliography.md#shells-and-composition
[hS23]: bibliography.md#shells-and-composition
[ShellVis26]: bibliography.md#shells-and-composition
[Tanimoto90]: bibliography.md#interaction-live-environments-and-agents
[Hazel19]: bibliography.md#interaction-live-environments-and-agents
[Planarian26]: bibliography.md#interaction-live-environments-and-agents
