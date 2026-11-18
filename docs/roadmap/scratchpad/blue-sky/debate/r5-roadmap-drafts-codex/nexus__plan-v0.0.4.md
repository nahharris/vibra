# v0.0.4 library and local-concurrency packages

Status: planned. The owner delegated the joint default: actor kernel substrate, scoped child/join and bounded ordered pure map first library paths, receive-loop long-lived actors with explicit lifetimes. This replaces the unqualified actor-first source mandate; no new source form is selected in implementation.

## Dependencies and ownership

| Package | Prerequisite | Entry points / outcome |
| --- | --- | --- |
| N4.1.1 exact dependencies | v0.0.2 snapshots/identity | package graph/acquisition/locks; canonical acyclic units and offline exact inputs |
| N4.1.2–N4.1.3 integration/upgrade | dependency contract | typed real integration, authority/model boundary and protected repair |
| N4.1.4–N4.1.5 adapters | v0.0.2 query/eval/test/edit machine engine | thin MCP/selected LSP clients, identical facts/revision checks |
| N4.2.1 kernel/simulation | concurrency specification prerequisite | vibra-actors lifecycle/mailbox/time/events, no OS threads/clock in simulator |
| N4.2.2 scope/join | kernel + approved scope contract | interpreter/kernel children, bounded admission, explicit cleanup |
| N4.2.3 typed endpoints | approved type/effect/grant contracts | checker/target admission, broker/kernel protocol checks |
| N4.2.4 pure ordered map | scope/transfer contract | bounded ordered execution and sequential fallback |
| N4.2.5 interpreter/replay | preceding semantic slices | source executor, schedule schema and causal observations |
| N4.3.1–N4.3.2 native executor/transfer | M2 + qualified v0.0.3 | Nexus Wait/actions/resume, copied values and delegated grants/captures |
| N4.3.3 workers | native executor | fixed pinned OS-thread pool/exclusive contexts |
| N4.3.4–N4.3.5 parity/overlap | complete workers | replay/stress/baseline and actual two-worker progress |

Library/adapters and interpreter kernel simulation can progress without native code generation when their own prerequisites are approved. Native workers do require the qualified native profile. Record this dependency overlap rather than claiming two unfinished milestones have passed.

## Specification prerequisite

Before checker/runtime source work, specify exact callable/library forms, scope lifetime/join/cancel semantics, typed protocol/endpoints, message and mapper-capture transfer, mailbox capacity/FIFO, full/closed/refused sends, replies/duplicates, exit/wait/timeout/blocked state, target effect admission and machine schedule/event schemas. Update affected chapters/diagnostics/corpus together. The plan does not authorize an invented keyword or a placeholder function as an accepted source contract.

The lifecycle machine must identify spawned/runnable/waiting/exited states and legal transitions. Choose explicit one-shot reply-slot outcomes without implying static must-reply or uniqueness. No-data-completion, cancellation and actor failure are distinct. Each valid/invalid transition gets independently authored cases.

## Shared-kernel rules

One backend-neutral implementation owns lifecycle, mailboxes, typed authorization, reply bookkeeping, timers and event order. Interpreter/Nexus execute until a kernel action and resume with its answer. Receive loops may park inside receive; no concurrent activations touch a unit's context.

Bound every mailbox and scope fan-out. Promise at most per-sender/receiver FIFO unless a stronger order is separately specified. A valid out-of-scope request is typed refusal, not a forged internal-token defect. Send authority does not grant the sender the receiver's host rights; target admission/reporting includes all reachable startable host operations.

Schedules record runnable/action choices and virtual-time decisions, not equal instruction reductions. Replay rejects impossible actions, non-runnable selections, truncated or leftover decisions. A seeded finite campaign reports domain/assumptions, not all-schedules proof. Import/build DAGs do not remove runtime wait cycles; blocked execution must be observable.

## Transfer and native worker rules

Messages copy neutral data across context stores; source release must not invalidate receiver data. Deep transfer uses an explicit worklist. Host/actor grants use checked broker/kernel delegation with world/generation/policy/lineage, never raw offsets or bearer-token serialization.

Pure parallel-map dispatch needs a separately checked code/type identity and immutable capture-transfer contract. It must preserve exact callable type/latent metadata; an opaque live Fn serialized as ordinary VIBON is not that mechanism. Define unsupported capture failure/admission before workers start, not an accidental crash half-way through a map.

The initial pool is fixed; place units before initialization and pin them, exclusive context ownership, no stealing/migration/shared arena. Parent scope joins/ends children under stated cleanup rules; cancellation after dispatch does not undo external actions. Separate late/unknown outcomes and no hidden retry. Ordered map preserves input order, not a universal completion order or speedup.

## Required validation

- Source and scripted kernel cases: full/closed mailbox, self-send, duplicate reply, wait exit/timeout, operation/scope refusal and no lost/duplicated completion.
- Nested scopes, failure/cancellation before and after submission, bounded pending children and explicit late outcomes.
- Pure mapper order, bound, sequential fallback, empty input, capture transfer and early failure; forbidden effectful mapper and row erasure reject.
- Same replay on interpreter/stepper/native, equal outcomes/events/origins; invalid schedule explicit error.
- Copied messages share no block, grants share inherited accounting, release/parked-memory balance and 100,000-deep transfer without native stack growth.
- Exclusive-owner stress, race instrumentation where supported and clear not-run gaps.
- Real two-worker CPU progress with hardware/intervals/placement and single-worker comparison; no event-order or universal-speedup assertion.
- Both independent suites, schema/diagnostic/approval regressions and shared-adapter equivalence.

The document-service stale-save and crash/recovery work is [v0.0.5](../v0.0.5/README.md); a local actor implementation alone does not pass that gate.
