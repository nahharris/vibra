# Secure compilation and sandboxed runtimes: six-paper distillation

Reading notes for Vibra's runtime/compiler design effort. Sources are in
`C:\Users\jorge\Documents\papers\vibra-research\environment\`. All paraphrased.

## SECOMP: secure compilation of compartmentalized C (Thibault, Blanco, Lee et al. — CCS 2024)

- **Problem.** Undefined behavior voids every C compiler guarantee program-wide.
  Compartmentalization limits the blast radius in practice, but no mainstream
  compiler had proved that it does.

- **Core mechanism.** All ten CompCert languages (Clight → RISC-V) gain static
  compartments: each procedure and global belongs to exactly one, each memory
  block is owned by its allocator, foreign-block accesses fail, and syscalls are
  allowlisted per compartment. The only legal interaction is a call or return
  matching declared imports/exports carrying **scalars only** — no pointer
  passing, so no shared memory. A RISC-V shadow stack forces well-bracketed
  control transfer, and spilled cross-compartment arguments are read-only. The
  property proved is Abate et al.'s robustly safe compilation with dynamic
  compromise and mutual distrust, via compiler correctness plus back-translation,
  recomposition, and blame.

- **Guarantee.** If a low-level attacker inside compartments compromised by their
  own UB can break a safety property for the still-clean compartments, a
  source-level attacker with the same interfaces, staying in the UB-free fragment
  of C, could have too. Assumes static compartments, scalars only, bounded
  traces, and an unproved (heavily tested) assumption that back-translations
  compile.

- **Evidence / cost.** ~43k LoC atop CompCert; the compartment extension alone
  added ~7k LoC of specs and ~11k of proofs (7.2% / 22.3% growth), plus ~25k for
  the security steps and a ~13k unverified CHERI backend. No performance data
  — only tiny programs compiled, and the theorem is not yet one Coq artifact.

- **Implications for Vibra.**
  - Static, code-based compartments with declared interfaces suffice for the
    strongest secure-compilation theorem yet proved for a real language.
  - A scalar-only cross-compartment ABI is what makes proofs tractable. Wasm's
    numeric params and structured control flow already supply that plus
    well-bracketed call/return; do not add implicit pointer or handle sharing.
  - Use per-compartment host-import allowlists, with dynamic compromise as the
    contract: a module breaking its own invariants loses only its own memory.

- **Caveats.** No inter-compartment memory sharing; shared pointers remain open.
  Safety only. Privileges fixed at compile time; backend unverified.

## Secure composition of robust and optimising compilers (Kruse, Backes, Patrignani — arXiv 2024)

- **Problem.** Individual passes had been proved to robustly preserve individual
  security properties, but nobody knew what a *chain* preserves, especially with
  optimizations interleaved.

- **Core mechanism.** Robust preservation: a compiler is secure for a property
  class if a source component satisfying it against every source attacker context
  compiles to one satisfying it against every target context, with cross-language
  trace relations moving properties between languages. Main theorem: composing
  compilers preserving C1 and C2 yields one preserving C1 ∩ C2, provided the
  trace relations are *well-formed* with respect to each other's class. The case
  study chains six passes over five languages to reach memory safety ∩ strict
  constant time ∩ speculative safety: boundary typechecks, bounds checks, DCE and
  constant folding, a CPU data-independent-timing mode re-enabled at every entry,
  and speculation barriers after branches.

- **Guarantee.** Prove each pass secure in isolation and mechanically conclude
  what the pipeline guarantees — if you discharge the well-formedness condition
  between adjacent passes. That is where ordering bugs live: speculation barriers
  before bounds-check insertion is unsound, because the later pass introduces new
  unprotected branches.

- **Evidence / cost.** Key results mechanized in Coq. No benchmarks: the
  languages are idealized (no pointer arithmetic, no structures) and memory
  safety is a bounds check on every access. Explicitly a theoretical foundation.

- **Implications for Vibra.**
  - Give each pass a written contract naming the property class it preserves, and
    treat ordering as a security constraint: hardening runs last, and any
    security mode a caller can switch off must be re-established at every entry.
  - The trace model needs a blame tag separating component- from context-emitted
    actions, or a hostile context makes the theorem vacuous.

- **Caveats.** Toy languages, no cost model, one Spectre variant. Their
  constant-time property is stricter than standard CCT. Composition yields the
  *intersection*, which shrinks with each pass.

## Type-preserving compilation for end-to-end verification (Chen, Chugh, Swamy — PLDI 2010)

- **Problem.** Rich source type systems verify security policies, but compilation
  discards the evidence, so a consumer of mobile code must trust the producer's
  compiler and solver.

- **Core mechanism.** FINE has dependent refinement types plus affine
  (use-at-most-once) types for evolving authorization state, with an external SMT
  solver discharging refinement obligations. A source-to-source pass,
  *derefinement*, reconstructs LCF-style proof terms from the solver's
  derivations and turns refined values into dependent value-plus-proof pairs, so
  a proof becomes an ordinary program value. The result compiles to DCIL, a .NET
  CIL extension with affine types, type-level functions, and value-parameterized
  classes. Both derefinement and the translation are proved type-preserving.

- **Guarantee.** The binary's recipient re-verifies, using only the DCIL
  typechecker, that the module enforces the stated authorization or
  information-flow policy. The solver and most of the compiler leave the TCB; the
  bytecode verifier and VM stay trusted. Conditional on shipped proof terms
  typechecking and on calls from untyped .NET being mediated.

- **Evidence / cost.** ~20k LoC F# compiler, ~12k LoC of benchmark reference
  monitors. Checking is cheap: a 51 MB proof library verifies in under seven
  seconds. Proof *carrying* is the cost — assemblies grow ~21x on average, up to
  53x, though a hand-written prover produced proofs ~25x smaller than Z3's, so
  the blow-up is a solver artifact rather than a limit.

- **Implications for Vibra.**
  - If Vibra's effect/capability system is expressible in the target type system,
    the sandbox boundary can *re-check* policy rather than trust the compiler.
    Plain Wasm validation cannot; this needs a typed-Wasm checker host-side.
  - Affine types are the right encoding for one-shot capabilities and
    state-transition tokens, much cheaper than full linearity. Keep the witness
    separable from the runtime representation so an erasure pass can drop it, and
    budget for proof *size*, not checking time.

- **Caveats.** 2010, tied to .NET generics, which restrict proofs to first-order
  logic. Recursion makes the proof language logically inconsistent. Policy
  assumptions fixed at typechecking time, so dynamic policies are out of scope.

## Memory simulations, security and optimization in a verified compiler (Monniaux — arXiv 2023)

- **Problem.** Verified compilers lack the hardening (canaries, pointer
  authentication) and optimizations (tail-recursion elimination) mainstream
  compilers ship, because those change memory layout and are hard to prove.

- **Core mechanism.** CompCert models memory as disjoint blocks and proves
  transformations by simulation. A *memory extension* lets the transformed
  program have longer blocks and more-defined values, which justifies canaries:
  the canary slot is provably inaccessible in the original memory, so any store
  that succeeded before cannot touch it. A *memory injection* maps blocks into
  sub-segments of other blocks, justifying tail-recursion elimination — it maps
  the would-be new frame onto the current one and drops the old. Pointer
  authentication is axiomatized Dolev-Yao style; its encode/decode pair
  deliberately does not commute with injections, harmless only because PAC is
  introduced after every injection-using pass.

- **Guarantee.** Machine-checked evidence that inserting the hardening preserves
  semantics, with essentially no TCB growth (five new PAC axioms, satisfiable by
  instantiating the crypto with the identity). Crucially *not* a proof the
  mitigation works: canary adequacy is unproved, and under normal semantics the
  stack-smashing handler is provably unreachable.

- **Evidence / cost.** Canaries: 97 LoC implementation, 1689 LoC proof.
  Tail-recursion elimination: 69 LoC, 2641 LoC proof. PAC costs under 1% on
  Cortex-A53 and Apple M1; canaries under 1% on functions with stack-allocated
  arrays, ~5% applied to all; tail-recursion elimination gained 14% on a RISC-V
  Rocket core and 19% on x86-64.

- **Implications for Vibra.**
  - Proof cost runs roughly 20–40x implementation size for a layout-changing
    pass. Quote that before promising anything verified, and put opaque or
    cryptographic primitives after all layout-changing passes.
  - Separate "the transformation preserves semantics" from "the mitigation stops
    the attack" in every claim. Selective application is what makes hardening
    free: targeting only at-risk functions turned 5% into under 1%.

- **Caveats.** Single compiler, C-specific, block memory model, plus the adequacy
  gap. Canary register erasure is optimized away by register allocation, a real
  divergence from gcc, and CompCert's external-call axioms are too permissive to
  prove some frame invariants.

## SandCell: sandboxing Rust beyond unsafe code (Zhang, Gülmez, Nyman, Tan — arXiv 2026)

- **Problem.** Existing Rust isolation draws the boundary between safe and unsafe
  code — the wrong shape (two mutually distrustful unsafe libraries share a
  sandbox) and too generous (all safe code, `rustc`, and the standard library
  land in the TCB despite known soundness holes in each).

- **Core mechanism.** The developer names existing *syntactic* units — functions,
  types, modules, crates — in a small spec file. A `rustc` MIR plugin inserts
  domain-switch wrappers at those units' public interfaces, and an
  interprocedural backward reachability analysis finds allocation sites whose
  objects can cross a boundary; those are rewritten to an allocator that places
  the object in a shared data domain, avoiding deep copying. A `transient` flag
  creates a fresh sandbox instance per call. Enforcement is in-process isolation
  on x86 memory protection keys: per-domain stack and heap, allocator metadata
  confined to a monitor domain, plus binary scanning and syscall interposition
  against PKU-bypass gadgets and calls.

- **Guarantee.** A memory-safety bug inside a sandbox — including from a compiler
  soundness hole or a stdlib bug, since the boundary is not safe-versus-unsafe —
  corrupts only that sandbox's memory, and reaching outside faults. Explicitly
  *not* provided: interface safety, side-channel protection, or defence against
  attacks on the interposition layer itself.

- **Evidence / cost.** Eight real applications (Servo, ripgrep, rust-openssl,
  rouille, elf-rs, oq, async-graphql, transpose), each needing **two lines** of
  spec; three proof-of-concept exploits were all contained. TCB is ~10k LoC
  allocator plus ~5k LoC monitor. Overheads, deep-copy versus shared-heap mode:
  rouille 89% → 3%, elf-rs 96.9% → 15.8%, ripgrep 13.4% → 10.3%, Servo 9.4% max.
  One regression, rust-openssl 17.7% → 22.2%, where data volumes are too small to
  repay the allocator change.

- **Implications for Vibra.**
  - Let the isolation boundary follow existing module structure — two lines of
    spec per application is the whole usability story. Per-invocation instances
    suit request handlers; make instance lifetime part of a compartment
    declaration, not a runtime flag.
  - The dominant cost is crossing frequency and data volume, not the enforcement
    primitive. Design the ABI so cross-boundary data is *allocated* in a shared
    region rather than marshalled — shared Wasm memory segments or handle tables,
    not copy-in/copy-out.
  - Do not trust your own stdlib: if Vibra's is compiled into every compartment,
    a bug there is a bug in every compartment at once.

- **Caveats.** x86 MPK only, capped at 16 domains, Linux only. Policy fixed at
  compile time. Syscall filtering covers only memory-relevant calls, and there is
  no formal proof of anything.

## A type system for safe memory management (Montenegro, Peña, Segura — PPDP 2008)

- **Problem.** GC gives unpredictable pauses and unanalysable memory consumption;
  manual deallocation is unsafe. Goal: programmer-directed destruction with a
  static no-dangling-pointer guarantee and constant-time allocation.

- **Core mechanism.** *Safe* is a first-order, eager, polymorphic functional
  language. The heap splits into regions; each call allocates a working region on
  entry and frees it on return, so region lifetimes track the call stack, and
  regions are inferred rather than written. On top sits explicit destruction: a
  destructive match frees the matched cell, a reuse operator rebuilds into it, a
  copy operator relocates a structure's spine. Types carry three qualifiers —
  safe, condemned (directly destroyed), and *in-danger* (transitively sharing a
  recursive descendant with something condemned, computed by an
  abstract-interpretation sharing analysis). In-danger types never appear in
  signatures, and region deallocation is sound because the working region
  variable may not occur in the result type.

- **Guarantee.** A well-typed program never dereferences a dangling pointer,
  despite explicitly freeing cells and implicitly freeing a whole region on every
  return. Cell and region allocation and deallocation are constant-time, tail
  recursion runs in constant stack space, and no garbage collector is needed.

- **Evidence / cost.** Paper proofs only, nothing machine-checked. The
  implementation is a ~5000-line Haskell front end covering region inference,
  sharing analysis, and destruction-annotation inference. Small examples only, no
  benchmarks, no space bounds.

- **Implications for Vibra.**
  - Call-scoped regions give constant-time allocation and no GC — viable for a
    Wasm runtime needing predictable latency, and far simpler than ownership.
  - The reusable idea is *in-danger* propagation: when a value is destroyed,
    taint everything statically known to share structure with it. That buys much
    of what a borrow checker buys without linearity. The sharing analysis, not
    the typing rules, is the expensive part — design and test it first.
  - Destruction as a *typed operation on a value* rather than a `free()` call is
    what keeps the property checkable.

- **Caveats.** First-order only, no polymorphic recursion over regions, strictly
  nested region lifetimes that can leak. Unmechanized and largely superseded by
  Rust-style ownership; its residual value is as a design for a *managed*
  language wanting determinism without a borrow checker.

## Cross-cutting synthesis

**Security per unit cost.** SandCell contains arbitrary memory-safety bugs —
including compiler and stdlib bugs — for two lines of configuration and single-
to low-double-digit percent overhead, with zero proof. Monniaux's hardening
passes cost ~20–40 lines of proof per line of implementation and under 1%
runtime, delivering machine-checked *semantic preservation* but not attack
prevention. SECOMP cost ~43k LoC and a multi-year team, with no performance story
yet. Formal guarantees cost two to three orders of magnitude more engineering
than the mitigation itself, and buy something qualitatively different — a
statement about *all* attackers, not the exploits you anticipated.

**Agreement.** Everyone who succeeded restricted the interface first: SECOMP
allows only scalars across compartments, SandCell's shared-heap optimization
exists because cross-boundary data is the cost centre, Safe forbids in-danger
types in signatures, FINE makes capability transfer explicit in the type. Narrow,
explicitly-typed boundaries are the one technique that makes both proofs and
performance tractable. All agree the boundary should follow existing program
structure rather than a semantic property like safe-versus-unsafe, and three
converge on the same ordering rule: transformations that do not commute with the
rest of the pipeline must run last.

**Disagreement.** The enforcement substrate. SECOMP's backend is CHERI
(unverified); SandCell uses MPK, capped at 16 domains, x86 Linux only; Kruse et
al. assume a bounds check on every access. Nobody has a verified, efficient,
portable enforcement layer — the real open problem, and Wasm is a strong
candidate answer none of these papers evaluates. They also split on memory
sharing: SECOMP forbids it to keep proofs possible while SandCell's performance
argument depends on it.

**Small team versus research programme.** Implementable now: static compartments
with declared interfaces; a scalar- or handle-only cross-compartment ABI;
per-compartment host-import allowlists; per-invocation instances; shared regions
for cross-boundary data; affine types for one-shot capabilities; region arenas
with explicit destruction; per-pass property contracts and security-aware
ordering. Research-scale and not worth attempting: mechanized
robust-preservation proofs, back-translation and recomposition arguments,
verified lowering to a real ISA, proof-carrying binaries. The right posture is to
make the *design* proof-compatible — narrow boundaries, no implicit sharing, no
dynamic privileges, explicit pass contracts — and to claim only what is actually
enforced.
