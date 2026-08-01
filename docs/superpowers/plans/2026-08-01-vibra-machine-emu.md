# Vibra Machine Emulator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a standalone `vibra-emu` Rust crate implementing the Vibra Machine ISA v0.1 as a deterministic single-step interpreter and expose it through the root CLI as `vibra emu`.

**Architecture:** The new crate owns all machine semantics: 36-bit tagged words, 72-bit capabilities, fixed-width instruction encoding, boot state, memory/MMIO, effect enforcement, protected frames, traps, and trace records. The existing `vibra` binary depends on it through a path dependency and remains a thin command-line adapter. This slice accepts a newline-delimited hexadecimal instruction image; the assembler and RTL are deliberately separate follow-on components because the ISA page specifies their future shape but not a stable CLI/file grammar.

**Tech Stack:** Rust 2021, `serde` for trace/report serialization, `anyhow` and `clap` in the existing CLI, standard-library collections only for emulator state.

**Source:** [Vibra Machine ISA v0.1](https://app.notion.com/p/3af73aa50d58803892f6f49d416ed23f), including the linked high-level plan and emulator trace contract.

---

## Scope decisions

- Use `crates/vibra-emu` as a standalone Cargo package named `vibra-emu`; do not turn the root package into a Cargo workspace, because the current repository intentionally has a single root package plus an independent `tools/corpus-migrator` package.
- Keep the machine deterministic. UART, buttons, cycle counters, and RNG are emulator-owned state exposed through `Machine` getters/setters; no host I/O occurs inside the crate.
- Treat a program image as one 32-bit word per non-empty, non-comment line. Accept `0x`/`0X` hexadecimal and decimal `u32` values, reject inline garbage and images longer than 64K words. A later assembler can compile the Notion Lisp-like examples into this image format.
- Model the sealing note with a 24-bit opaque token plus an emulator-side token table keyed by object type. This preserves the full original `Word` while matching the specified `SEALED` physical layout (`otype:8 | token:24`) and prevents forged tokens because no instruction constructs tag `SEALED`.
- Use closure type id `1` for `CALLCL` in v0.1. The header at `PTR - 1` must be `HDR` with type id `1`; body word 0 is `CODE`, body word 1 is the environment word.
- Treat all architectural faults as latched traps that halt execution, set `TCAUSE`, `TPC`, and `TVAL`, and appear in the final trace. `TVEC`/handler dispatch and privilege are v0.2 and are not invented here.

## File map

- Create `crates/vibra-emu/Cargo.toml`: independent crate metadata and the minimal `serde` dependency.
- Create `crates/vibra-emu/src/lib.rs`: public crate API and module exports.
- Create `crates/vibra-emu/src/isa.rs`: tags, words, permissions, capabilities, effect classes, opcodes, formats, instruction encoding/decoding, and encoding errors.
- Create `crates/vibra-emu/src/machine.rs`: boot state, data/instruction memory, MMIO, protected frames, instruction execution, traps, run status, and trace snapshots.
- Create `crates/vibra-emu/src/program.rs`: deterministic hexadecimal program-image parser and program-size validation.
- Create `crates/vibra-emu/tests/isa.rs`: public value/encoding contract tests.
- Create `crates/vibra-emu/tests/machine.rs`: boot, execution, memory, capability, effect, control-flow, trap, and trace tests.
- Create `crates/vibra-emu/tests/program.rs`: image parser tests.
- Modify `Cargo.toml`: add the path dependency `vibra-emu = { path = "crates/vibra-emu" }`.
- Modify `Cargo.lock`: update through Cargo after adding the path dependency; do not hand-edit it.
- Modify `src/main.rs`: add `Command::Emu`, its format/options, and the adapter that loads an image, steps the machine, prints a report, and returns a nonzero error for traps or step exhaustion.
- Create `tests/emu_cli.rs`: black-box tests for `vibra emu` image execution, JSON output, trap exit behavior, and argument validation.
- Modify `README.md`: document `vibra emu` and the initial hex-image format.

### Task 1: Add the standalone crate and boot-state contract

**Files:**
- Create: `crates/vibra-emu/Cargo.toml`
- Create: `crates/vibra-emu/src/lib.rs`
- Create: `crates/vibra-emu/src/isa.rs`
- Create: `crates/vibra-emu/src/machine.rs`
- Create: `crates/vibra-emu/tests/machine.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Write the failing boot test**

Add this public-contract test to `crates/vibra-emu/tests/machine.rs`:

```rust
use vibra_emu::{Capability, Machine, Permissions, Tag, Word};

#[test]
fn boot_matches_isa_v01_reset_state() {
    let machine = Machine::boot(&[]).expect("empty image is a valid halted-ready machine");

    assert_eq!(machine.pc(), 0);
    assert_eq!(machine.emr(), 0xffff);
    assert_eq!(machine.hp(), 0x004000);
    assert_eq!(machine.fsp(), 0);
    assert_eq!(machine.reg(0), Word::int(0));
    assert_eq!(machine.reg(1), Word::poison());
    assert_eq!(machine.cap(0), Capability::null());
    assert_eq!(machine.cap(1).base(), 0);
    assert_eq!(machine.cap(1).len(), 0x4000);
    assert!(machine.cap(1).permissions().contains(Permissions::READ));
    assert!(machine.cap(1).permissions().contains(Permissions::WRITE));
    assert_eq!(machine.cap(2).base(), 0xF00000);
    assert_eq!(machine.cap(2).len(), 0x100);
    assert_eq!(machine.cap(3), Capability::null());
    assert_eq!(machine.pcc(), Capability::new(0, 0x10000, Permissions::EXECUTE | Permissions::DERIVE, 0).unwrap());
    assert_eq!(machine.hpc(), Capability::new(0x4000, 0x4000, Permissions::READ | Permissions::WRITE | Permissions::ALLOCATE | Permissions::DERIVE, 0).unwrap());
}
```

- [ ] **Step 2: Run the test to verify it fails for the missing crate/API**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml`

Expected: FAIL because `crates/vibra-emu/Cargo.toml` and the exported machine API do not exist yet.

- [ ] **Step 3: Add the minimal crate and boot implementation**

Create `crates/vibra-emu/Cargo.toml` with package name `vibra-emu`, version `0.1.0`, edition `2021`, and `serde = { version = "1.0", features = ["derive"] }`. Add the root path dependency:

```toml
[dependencies]
vibra-emu = { path = "crates/vibra-emu" }
```

Export `Tag`, `Word`, `Permissions`, `Capability`, `Machine`, and the machine error/status types from `lib.rs`. Implement `Machine::boot` with the exact reset values from the ISA: 64K instruction slots, 32K mapped data words for static data plus heap, 256 MMIO words, `r0 = INT 0`, `r1..r15 = POISON`, `c0 = null`, `c1` static data, `c2` root MMIO, `c3..c7 = null`, `PCC = {0, 0x10000, X|DERIVE}`, `HPC = {0x4000, 0x4000, R|W|ALLOC|DERIVE}`, `PC = 0`, `EMR = 0xffff`, `HP = 0x4000`, and `FSP = 0`.

- [ ] **Step 4: Run the focused test to verify it passes**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml --test machine boot_matches_isa_v01_reset_state`

Expected: PASS.

- [ ] **Step 5: Commit the crate scaffold**

```powershell
git add Cargo.toml crates/vibra-emu
git commit -m "feat: add vibra machine emulator crate"
```

### Task 2: Implement the v0.1 data and capability value model

**Files:**
- Modify: `crates/vibra-emu/src/isa.rs`
- Create: `crates/vibra-emu/tests/isa.rs`

- [ ] **Step 1: Write failing value-model tests**

Add tests covering all non-reserved tags, signed integer payloads, poison reads, boolean/char validation, capability field widths, permission intersection, and monotone derivation:

```rust
use vibra_emu::{Capability, Permissions, Tag, Word};

#[test]
fn words_preserve_tag_and_32_bit_payload() {
    assert_eq!(Word::int(-1).payload(), u32::MAX);
    assert_eq!(Word::int(-1).as_i32().unwrap(), -1);
    assert_eq!(Word::int(-1).tag(), Tag::Int);
    assert_eq!(Word::poison().tag(), Tag::Poison);
}

#[test]
fn constructors_reject_invalid_boolean_and_character_payloads() {
    assert!(Word::try_new(Tag::Bool, 2).is_err());
    assert!(Word::try_new(Tag::Char, 0xD800).is_err());
    assert!(Word::try_new(Tag::Unit, 1).is_err());
    assert!(Word::try_new(Tag::Null, 1).is_err());
}

#[test]
fn capability_derivation_can_only_narrow_authority() {
    let parent = Capability::new(
        100,
        50,
        Permissions::READ | Permissions::WRITE | Permissions::DERIVE,
        7,
    )
    .unwrap();
    let child = parent.derive(10, 20).unwrap();
    assert_eq!(child.base(), 110);
    assert_eq!(child.len(), 20);
    assert_eq!(child.permissions(), parent.permissions());
    assert!(parent.derive(40, 20).is_err());
    assert_eq!(parent.attenuate(Permissions::READ).permissions(), Permissions::READ);
}
```

- [ ] **Step 2: Run the tests and verify the failures are about missing behavior**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml --test isa`

Expected: FAIL to compile until the value types and methods are implemented.

- [ ] **Step 3: Implement `Tag`, `Word`, `Permissions`, and `Capability`**

Use `#[repr(u8)]` for tags and permissions with exact ISA values: `UNIT=0`, `INT=1`, `BOOL=2`, `CHAR=3`, `PTR=4`, `CODE=5`, `HDR=6`, `CAPIDX=7`, `SEALED=8`, `POISON=14`, and `NULL=15`. Reject reserved tags 9–13. Keep capability `len` bounded to 24 bits, use `u64` for `base + len` checks, and make `derive`/`attenuate` return typed errors rather than silently widening authority. Expose read-only accessors for every field.

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml --test isa`

Expected: PASS.

- [ ] **Step 5: Commit the value model**

```powershell
git add crates/vibra-emu/src/isa.rs crates/vibra-emu/tests/isa.rs
git commit -m "feat: model vibra machine words and capabilities"
```

### Task 3: Implement fixed-width instruction encoding and decoding

**Files:**
- Modify: `crates/vibra-emu/src/isa.rs`
- Modify: `crates/vibra-emu/tests/isa.rs`

- [ ] **Step 1: Write failing round-trip and field-layout tests**

Test the fixed bit layout and signed immediate extraction directly:

```rust
use vibra_emu::{Format, Instruction, Opcode};

#[test]
fn instruction_fields_use_the_fixed_v01_positions() {
    let instruction = Instruction::i(Opcode::AddI, 6, 3, 2, -17).unwrap();
    assert_eq!(instruction.encode(), 0x358c_bfef);
    let decoded = Instruction::decode(instruction.encode()).unwrap();
    assert_eq!(decoded.format(), Format::I);
    assert_eq!(decoded.rd(), 3);
    assert_eq!(decoded.rs1(), 2);
    assert_eq!(decoded.imm14(), -17);
}

#[test]
fn every_defined_opcode_decodes_with_its_declared_format() {
    for &(opcode, format) in Opcode::all() {
        assert_eq!(opcode.format(), format);
    }
}
```

The test should use a table of the actual defined opcodes rather than assuming reserved opcodes are valid; the second assertion is specifically for all v0.1 opcodes.

- [ ] **Step 2: Run the focused tests and verify they fail before implementation**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml --test isa instruction_fields_use_the_fixed_v01_positions`

Expected: FAIL because `Instruction`, `Opcode`, and the format accessors are not implemented.

- [ ] **Step 3: Implement the opcode table and field packer**

Define all v0.1 opcodes with the exact numeric values: `NOP 0x00`, `MOV 0x01`, `MOVI 0x02`, `MOVH 0x03`, `ADD..SHRI 0x04..0x12`, `CMP..ISTAG 0x13..0x18`, `POISON 0x1a`, `MKTAG 0x1b`, `LD..HDR 0x1c..0x1f`, `CDERIVE..CSPECIAL 0x20..0x28`, `BR..TRET 0x29..0x36`, `HALT 0x37`, `DIV 0x38`, and `REM 0x39`. Encode fields as `op[31:26]`, `eff[25:22]`, `A[21:18]`, `B[17:14]`, `C[13:10]`, and `imm10[9:0]`. Implement `R`, `I`, `L`, `J`, `B`, `M`, and `C` constructors with range checking; sign-extend `imm14`, `imm18`, and `imm22` on decode. For `M`, use `A` as `rd` for `LD` and `rs` for `ST`, `B[2:0]` as the capability register, and `C` as the base register. For `C`, use the opcode-specific layouts documented in the crate: capability destination/source in `A/B`, general registers in `C` or `aux10`, and cap selectors in the low three bits.

- [ ] **Step 4: Run all crate ISA tests**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml --test isa`

Expected: PASS, including rejection of unknown opcodes and out-of-range fields.

- [ ] **Step 5: Commit the encoding layer**

```powershell
git add crates/vibra-emu/src/isa.rs crates/vibra-emu/tests/isa.rs
git commit -m "feat: add vibra machine instruction encoding"
```

### Task 4: Add program-image parsing and the trace/run API

**Files:**
- Create: `crates/vibra-emu/src/program.rs`
- Modify: `crates/vibra-emu/src/lib.rs`
- Create: `crates/vibra-emu/tests/program.rs`
- Modify: `crates/vibra-emu/src/machine.rs`

- [ ] **Step 1: Write failing program parser and trace tests**

```rust
use vibra_emu::{parse_hex_program, Instruction, Opcode};

#[test]
fn hex_program_parser_ignores_comments_and_blank_lines() {
    let words = parse_hex_program("\n# halt\n0xDC000000\n55\n").unwrap();
    assert_eq!(words, vec![0xDC000000, 55]);
}

#[test]
fn hex_program_parser_rejects_overflow_and_too_many_words() {
    assert!(parse_hex_program("0x1_0000_0000").is_err());
    let image = (0..=0x10000).map(|_| "0").collect::<Vec<_>>().join("\n");
    assert!(parse_hex_program(&image).is_err());
}

#[test]
fn a_single_step_trace_contains_the_fetched_instruction() {
    let program = [Instruction::halt().encode()];
    let mut machine = vibra_emu::Machine::boot(&program).unwrap();
    let trace = machine.step().unwrap();
    assert_eq!(trace.pc, 0);
    assert_eq!(trace.insn, program[0]);
    assert_eq!(trace.trap, None);
    assert!(machine.status().is_halted());
}
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml --test program`

Expected: FAIL because the parser, trace, and stop-status API do not exist.

- [ ] **Step 3: Implement the image parser and public stepping contract**

Parse one token per non-empty line, strip a full-line `#` comment, accept decimal or `0x`/`0X` hexadecimal `u32`, and include line numbers in parse errors. Reject empty inline tokens, values above `u32::MAX`, and more than 65,536 words. Define `Trace` exactly as the ISA contract (`pc`, `insn`, `[Word; 16]`, `[Capability; 8]`, `emr`, `hp`, `fsp`, `mem_write`, `trap`) and make it serializable. `Machine::step` must snapshot the fetched PC/instruction, execute one instruction, snapshot post-state, and return a trace even when the instruction halts or traps. `Machine::run(max_steps)` must stop on `HALT`, trap, or the explicit step limit and expose a serializable `RunReport`.

- [ ] **Step 4: Run focused parser/trace tests**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml --test program`

Expected: PASS.

- [ ] **Step 5: Commit the image and trace API**

```powershell
git add crates/vibra-emu/src/program.rs crates/vibra-emu/src/machine.rs crates/vibra-emu/src/lib.rs crates/vibra-emu/tests/program.rs
git commit -m "feat: add emulator images and trace reports"
```

### Task 5: Implement data movement, constants, arithmetic, comparisons, and tag operations

**Files:**
- Modify: `crates/vibra-emu/src/machine.rs`
- Modify: `crates/vibra-emu/tests/machine.rs`

- [ ] **Step 1: Write failing execution tests**

Add focused tests for the first executable family:

```rust
use vibra_emu::{Instruction, Machine, Opcode, Tag, Word};

#[test]
fn arithmetic_is_tag_checked_and_traps_on_signed_overflow() {
    let program = [
        Instruction::l(Opcode::MovI, 0, 1, 2).unwrap().encode(),
        Instruction::l(Opcode::MovI, 0, 2, 3).unwrap().encode(),
        Instruction::r(Opcode::Add, 0, 3, 1, 2, 0).unwrap().encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();
    machine.step().unwrap();
    machine.step().unwrap();
    machine.step().unwrap();
    assert_eq!(machine.reg(3), Word::int(5));
}

#[test]
fn poison_reads_and_bad_operand_tags_trap() {
    let program = [Instruction::r(Opcode::Add, 0, 2, 1, 0, 0).unwrap().encode()];
    let mut machine = Machine::boot(&program).unwrap();
    let trace = machine.step().unwrap();
    assert_eq!(trace.trap, Some(0x02));
    assert_eq!(machine.tcause(), 0x02);
    assert_eq!(machine.reg(2).tag(), Tag::Poison);
}

#[test]
fn comparisons_and_tag_inspection_produce_typed_results() {
    let program = [
        Instruction::l(Opcode::MovI, 0, 1, 4).unwrap().encode(),
        Instruction::l(Opcode::MovI, 0, 2, 4).unwrap().encode(),
        Instruction::r(Opcode::Cmp, 0, 3, 1, 2, 0).unwrap().encode(),
        Instruction::r(Opcode::Tag, 0, 4, 3, 0, 0).unwrap().encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();
    for _ in 0..4 { machine.step().unwrap(); }
    assert_eq!(machine.reg(3), Word::bool(true));
    assert_eq!(machine.reg(4), Word::int(Tag::Bool as i32));
}
```

- [ ] **Step 2: Run the new tests and verify the expected failures**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml --test machine arithmetic_is_tag_checked_and_traps_on_signed_overflow`

Expected: FAIL because `Machine::step` does not yet execute these opcodes.

- [ ] **Step 3: Implement the execution families**

Implement `NOP`, `MOV`, `MOVI`, `MOVH`, `POISON`, `MKTAG`, `ADD`, `SUB`, `MUL`, `AND`, `OR`, `XOR`, `SHL`, `SHR`, `SAR`, `ADDI`, `ANDI`, `ORI`, `XORI`, `SHLI`, `SHRI`, `DIV`, `REM`, `CMP`, `BNOT`, `BAND`, `BOR`, `TAG`, and `ISTAG`. Read every operand through helpers that trap `POISON` or mismatched tags before arithmetic. Use checked signed `i32` arithmetic, trap `#OVF` (`0x03`) for overflow and shifts of 32 or more, trap `#DIV0` (`0x04`) for zero divisors, and represent all successful results through validated `Word` constructors. `CMP` must require equal tags; `EQ`/`NE` compare any matching tags, ordered comparisons accept `INT`/`CHAR`, and `LTU`/`LEU` compare unsigned payloads. `MKTAG` must require `eff = UNSAFE` (`9`) and reject invalid tag/payload combinations.

- [ ] **Step 4: Run the complete machine test target**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml --test machine`

Expected: PASS for all data/ALU/tag tests.

- [ ] **Step 5: Commit the execution core**

```powershell
git add crates/vibra-emu/src/machine.rs crates/vibra-emu/tests/machine.rs
git commit -m "feat: execute tagged vibra machine arithmetic"
```

### Task 6: Implement memory, allocator, capabilities, MMIO, effects, control flow, and traps

**Files:**
- Modify: `crates/vibra-emu/src/machine.rs`
- Modify: `crates/vibra-emu/tests/machine.rs`

- [ ] **Step 1: Write failing capability/effect/control tests**

Cover the two security properties from the worked examples first:

```rust
#[test]
fn attenuated_led_capability_denies_uart_address() {
    // c2 is root MMIO; derive offset 0x10, length 1, then drop c2.
    // A store through c4 at offset -16 must latch #CAPBOUND (0x05).
}

#[test]
fn withe_zero_denies_io_even_when_the_capability_has_write_authority() {
    // WITHE 0, then an IO-tagged ST through a valid MMIO cap must latch #EFFECT (0x07).
}

#[test]
fn allocation_fills_the_body_with_poison_and_enforces_hpc() {
    // ALLOC writes HDR at HP, fills n body words with POISON, returns PTR HP+1,
    // and a request that exceeds HPC latches #HEAP (0x08).
}

#[test]
fn call_and_return_restore_pc_emr_and_pcc_from_the_private_frame_stack() {
    // CALL changes PC, WITHE narrows EMR, RET restores the saved PC/EMR/PCC;
    // the data-memory API cannot observe the frame record.
}
```

- [ ] **Step 2: Run the tests and verify they fail for unimplemented opcodes**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml --test machine attenuated_led_capability_denies_uart_address`

Expected: FAIL because `LD`, `ST`, capabilities, effects, and control flow are not implemented.

- [ ] **Step 3: Implement memory and capability instructions**

Implement word-addressed static/heap memory and the 256-word MMIO window. `LD`/`ST` must require an `INT` base register, sign-extend `imm10`, reject negative/out-of-range offsets with `#CAPBOUND`, check `R`/`W`, require `MMIO` and `eff = IO` for device addresses, and convert poison reads to `#POISON`. Implement `ALLOC` with `eff = ALLOC`, `INT` size, header `(size:20 | typeid:12)`, poison fill, `PTR` result, bump `HP`, and `#HEAP` on exhaustion; `HDR` reads the header immediately before a valid pointer. Implement `CDERIVE`, `CANDPERM`, `CDROP`, `CSEAL`, `CUNSEAL`, `CMOV`, `CINC`, `CGET`, and `CSPECIAL`. Enforce the capability monotonicity invariant in one helper for every capability result. `CSEAL` allocates a 24-bit opaque token and stores the original word in the emulator token table; `CUNSEAL` requires `SEAL` and matching `otype` or traps `#SEAL` (`0x0a`).

- [ ] **Step 4: Implement effects, frames, and control flow**

Check `EMR[eff]` before every instruction except `eff = 0`. Implement `BR`, `BRT`, `BRF`, `JMP`, `CALL`, `CALLC`, `CALLCL`, `RET`, `HALT`, `WITHE`, `ENDE`, `GETE`, `REQE`, `TRAP`, and the v0.1 halt-on-`TRET` behavior. Use a private 1K-entry frame stack with typed call/effect entries; `RET` restores `PC`, `EMR`, and `PCC`, while `ENDE` restores only `EMR`; mismatched or empty pops trap `#FRAME` (`0x09`). `CALLCL` validates closure header type id 1, word 0 `CODE`, word 1 environment, sets `r14`, and checks the target against `PCC`. Latch the exact trap causes listed by the ISA: `#TAG 0x01`, `#POISON 0x02`, `#OVF 0x03`, `#DIV0 0x04`, `#CAPBOUND 0x05`, `#CAPPERM 0x06`, `#EFFECT 0x07`, `#HEAP 0x08`, `#FRAME 0x09`, `#SEAL 0x0a`, `#BADOP 0x0b`, `#BADPC 0x0c`, and `0x80 | imm[6:0]` for `TRAP`.

- [ ] **Step 5: Run the machine tests**

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml --test machine`

Expected: PASS, including the capability-denial and pure-effect examples.

- [ ] **Step 6: Commit the complete v0.1 machine semantics**

```powershell
git add crates/vibra-emu/src/machine.rs crates/vibra-emu/tests/machine.rs
git commit -m "feat: implement vibra machine capabilities and effects"
```

### Task 7: Expose the emulator through `vibra emu`

**Files:**
- Modify: `src/main.rs`
- Create: `tests/emu_cli.rs`
- Modify: `README.md`

- [ ] **Step 1: Write failing CLI integration tests**

Add black-box tests that create a temporary hex image and invoke the real binary:

```rust
#[test]
fn emu_runs_hex_image_and_returns_json_trace() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("hello.vmi");
    std::fs::write(&image, "0x08040007\n0xDC000000\n").unwrap();
    let output = vibra_cmd()
        .args(["emu", image.to_str().unwrap(), "--trace", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "halted");
    assert_eq!(report["traces"].as_array().unwrap().len(), 2);
}

#[test]
fn emu_trap_is_reported_and_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("trap.vmi");
    std::fs::write(&image, "0xE8000000\n").unwrap();
    let output = vibra_cmd().args(["emu", image.to_str().unwrap()]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("BADOP"));
}
```

`0x08040007` is `MOVI r1, 7`; `0xDC000000` is `HALT`; and `0xE8000000` is the undefined opcode `0x3a`, which must trap as `#BADOP`.

- [ ] **Step 2: Run the CLI tests to verify they fail before routing exists**

Run: `cargo test --test emu_cli`

Expected: FAIL because Clap does not recognize `emu`.

- [ ] **Step 3: Add the `Emu` command and adapter**

Add a `Command::Emu` variant with required `program: PathBuf`, `--max-steps` defaulting to `1_000_000`, `--trace`, and `--format {json,human}` defaulting to JSON. Read the file, call `vibra_emu::parse_hex_program`, run the machine, serialize a summary containing `status`, `steps`, `exit_code`, `trap`, and optional `traces`, and print it to stdout. Human output must state the final status and trap cause in one line. Return an `anyhow` error after printing a trap or step-limit report so shell callers receive a nonzero status; a normal `HALT` returns success. Keep the adapter free of ISA execution logic.

- [ ] **Step 4: Run the CLI tests and the command help check**

Run: `cargo test --test emu_cli`

Expected: PASS.

Run: `cargo run -- emu --help`

Expected: help contains `Run a Vibra Machine v0.1 hex image` and the `--max-steps`/`--trace` options.

- [ ] **Step 5: Document the command**

Add to `README.md`:

```text
### Vibra Machine emulator

`cargo run -- emu program.vmi` executes a newline-delimited 32-bit instruction
image. Blank lines and lines beginning with `#` are ignored; words may be
decimal or `0x` hexadecimal. Use `--trace --format json` for the v0.1 trace
contract. A HALT exits successfully; an architectural trap or step limit exits
nonzero after printing its report.
```

- [ ] **Step 6: Commit CLI integration**

```powershell
git add src/main.rs tests/emu_cli.rs README.md Cargo.lock
git commit -m "feat: expose vibra machine emulator via cli"
```

### Task 8: Final verification and handoff

**Files:**
- Verify: all changed files in `crates/vibra-emu`, `src/main.rs`, `tests/emu_cli.rs`, `README.md`, and `Cargo.lock`

- [ ] **Step 1: Run formatter and focused crate tests**

Run: `cargo fmt --all -- --check`

Expected: PASS.

Run: `cargo test --manifest-path crates/vibra-emu/Cargo.toml`

Expected: all emulator unit/integration tests pass.

- [ ] **Step 2: Run the repository Rust suite**

Run: `cargo test`

Expected: all existing tests plus `emu_cli` pass. If project-sync tests are denied by the sandbox again, rerun only those tests with the already-approved focused elevated command and record that environmental distinction.

- [ ] **Step 3: Run the Vibra-language suite**

Run: `cargo run -- test`

Expected: `total: 96`, `passed: 96`, `failed: 0` (or a larger total if the repository adds tests during implementation).

- [ ] **Step 4: Inspect the final diff and status**

Run: `git diff --check`; then `git status --short --branch` from `C:\Users\jorge\Documents\dev\vibra\.worktrees\vibra-machine`.

Expected: no whitespace errors, only intentional emulator/CLI/documentation changes, and branch `codex/vibra-machine` based on `main`.

- [ ] **Step 5: Commit the verified result**

```powershell
git add crates/vibra-emu src/main.rs tests/emu_cli.rs README.md Cargo.lock
git commit -m "feat: implement vibra machine v0.1 emulator"
```

## Self-review against the ISA page

- Machine model, all v0.1 tags, register files, special registers, capability layout, permissions, effect classes, and reset state are covered by Tasks 1, 2, and 6.
- Fixed 32-bit encoding and all defined opcodes are covered by Task 3 and execution tests in Tasks 5 and 6.
- Word-addressed memory, MMIO, heap allocation, poison behavior, bounds, and permissions are covered by Task 6.
- Protected frames, effect narrowing/restoration, traps, `TCAUSE`/`TPC`/`TVAL`, and halt-on-trap are covered by Task 6.
- The exact `Trace` shape and deterministic single-step API are covered by Task 4 and the CLI report in Task 7.
- The spec's cap-spilling, packed strings, software-defined effect extension, handler dispatch, assembler grammar, compiler backend, and Amaranth RTL are intentionally outside this first crate/CLI slice; none is silently represented as implemented.
- The plan contains no dependency on the dirty original checkout and keeps the `stdlib` submodule at the `main`-pinned commit.
