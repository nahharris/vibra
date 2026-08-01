use vibra_emu::{Capability, Instruction, Machine, Opcode, Permissions, Tag, Word};

#[test]
fn boot_matches_isa_v01_reset_state() {
    let machine = Machine::boot(&[]).expect("empty image is a valid machine");

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
    assert_eq!(
        machine.pcc(),
        Capability::new(0, 0x10000, Permissions::EXECUTE | Permissions::DERIVE, 0,).unwrap()
    );
    assert_eq!(
        machine.hpc(),
        Capability::new(
            0x4000,
            0x4000,
            Permissions::READ | Permissions::WRITE | Permissions::ALLOCATE | Permissions::DERIVE,
            0,
        )
        .unwrap()
    );
}

#[test]
fn halt_produces_a_trace_and_stops_the_machine() {
    let mut machine = Machine::boot(&[Instruction::halt().encode()]).unwrap();
    let trace = machine.step().unwrap();
    assert_eq!(trace.pc, 0);
    assert_eq!(trace.insn, Instruction::halt().encode());
    assert_eq!(trace.trap, None);
    assert!(machine.status().is_halted());
    assert_eq!(machine.status().exit_code(), Some(0));
}

#[test]
fn movi_and_add_execute_with_int_tags() {
    let program = [
        Instruction::l(Opcode::MovI, 0, 1, 2).unwrap().encode(),
        Instruction::l(Opcode::MovI, 0, 2, 3).unwrap().encode(),
        Instruction::r(Opcode::Add, 0, 3, 1, 2, 0).unwrap().encode(),
        Instruction::halt().encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();
    let report = machine.run(4).unwrap();
    assert!(report.status.is_halted());
    assert_eq!(machine.reg(3), Word::int(5));
}

#[test]
fn poison_operands_trap_instead_of_producing_a_value() {
    let program = [Instruction::r(Opcode::Add, 0, 2, 1, 0, 0).unwrap().encode()];
    let mut machine = Machine::boot(&program).unwrap();
    let trace = machine.step().unwrap();
    assert_eq!(trace.trap, Some(0x02));
    assert_eq!(machine.status().trap_cause(), Some(0x02));
    assert_eq!(machine.reg(2).tag(), Tag::Poison);
}

#[test]
fn signed_overflow_traps_and_does_not_write_the_destination() {
    let program = [
        Instruction::l(Opcode::MovI, 0, 1, -1).unwrap().encode(),
        Instruction::i(Opcode::MovH, 0, 1, 0, 0x1fff)
            .unwrap()
            .encode(),
        Instruction::l(Opcode::MovI, 0, 2, 1).unwrap().encode(),
        Instruction::r(Opcode::Add, 0, 3, 1, 2, 0).unwrap().encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();
    for _ in 0..3 {
        machine.step().unwrap();
    }
    let trace = machine.step().unwrap();
    assert_eq!(trace.trap, Some(0x03));
    assert_eq!(machine.reg(3), Word::poison());
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
    for _ in 0..4 {
        machine.step().unwrap();
    }
    assert_eq!(machine.reg(3), Word::bool(true));
    assert_eq!(machine.reg(4), Word::int(Tag::Bool as i32));
}

#[test]
fn boolean_operations_require_bool_operands() {
    let program = [
        Instruction::l(Opcode::MovI, 0, 1, 1).unwrap().encode(),
        Instruction::r(Opcode::BNot, 0, 2, 1, 0, 0)
            .unwrap()
            .encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();
    machine.step().unwrap();
    let trace = machine.step().unwrap();
    assert_eq!(trace.trap, Some(0x01));
}

#[test]
fn load_and_store_use_word_addressed_capability_memory() {
    let program = [
        Instruction::l(Opcode::MovI, 0, 1, 0).unwrap().encode(),
        Instruction::l(Opcode::MovI, 0, 2, 42).unwrap().encode(),
        Instruction::m(Opcode::Store, 0, 2, 1, 1, 0)
            .unwrap()
            .encode(),
        Instruction::m(Opcode::Load, 0, 3, 1, 1, 0)
            .unwrap()
            .encode(),
        Instruction::halt().encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();

    let report = machine.run(5).unwrap();

    assert!(report.status.is_halted());
    assert_eq!(machine.reg(3), Word::int(42));
    assert_eq!(report.traces[2].mem_write, Some((0, Word::int(42))));
}

#[test]
fn attenuated_mmio_capability_denies_an_out_of_bounds_address() {
    let program = [
        Instruction::l(Opcode::MovI, 0, 1, 0x10).unwrap().encode(),
        Instruction::l(Opcode::MovI, 0, 2, 1).unwrap().encode(),
        Instruction::c(Opcode::CDerive, 0, 4, 2, 1, 2)
            .unwrap()
            .encode(),
        Instruction::c(Opcode::CDrop, 0, 2, 0, 0, 0)
            .unwrap()
            .encode(),
        Instruction::l(Opcode::MovI, 0, 5, 7).unwrap().encode(),
        Instruction::m(Opcode::Store, 1, 5, 4, 0, -16)
            .unwrap()
            .encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();

    let trace = machine.run(6).unwrap().traces.last().unwrap().clone();

    assert_eq!(trace.trap, Some(0x05));
    assert_eq!(machine.tval(), Word::int(0xF00000));
    assert!(machine.cap(2).is_null());
}

#[test]
fn withe_zero_denies_io_even_with_a_valid_write_capability() {
    let program = [
        Instruction::l(Opcode::Withe, 0, 0, 0).unwrap().encode(),
        Instruction::l(Opcode::MovI, 0, 5, 65).unwrap().encode(),
        Instruction::m(Opcode::Store, 1, 5, 2, 0, 0)
            .unwrap()
            .encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();

    let trace = machine.run(3).unwrap().traces.last().unwrap().clone();

    assert_eq!(trace.trap, Some(0x07));
    assert_eq!(machine.emr(), 0);
}

#[test]
fn allocation_writes_a_header_and_poison_fills_the_body() {
    let program = [
        Instruction::l(Opcode::MovI, 0, 1, 2).unwrap().encode(),
        Instruction::i(Opcode::Alloc, 4, 2, 1, 7).unwrap().encode(),
        Instruction::halt().encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();

    let report = machine.run(3).unwrap();

    assert!(report.status.is_halted());
    assert_eq!(machine.reg(2), Word::try_new(Tag::Ptr, 0x4001).unwrap());
    assert_eq!(machine.hp(), 0x4003);
    assert_eq!(
        machine.memory_word(0x4000),
        Some(Word::try_new(Tag::Header, (2 << 12) | 7).unwrap())
    );
    assert_eq!(machine.memory_word(0x4001), Some(Word::poison()));
    assert_eq!(machine.memory_word(0x4002), Some(Word::poison()));
}

#[test]
fn allocation_traps_when_the_heap_capability_would_be_exceeded() {
    let program = [
        Instruction::l(Opcode::MovI, 0, 1, 0x4000).unwrap().encode(),
        Instruction::i(Opcode::Alloc, 4, 2, 1, 1).unwrap().encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();

    let trace = machine.run(2).unwrap().traces.last().unwrap().clone();

    assert_eq!(trace.trap, Some(0x08));
    assert_eq!(machine.hp(), 0x4000);
}

#[test]
fn call_and_return_restore_the_effect_mask_and_private_frame() {
    let program = [
        Instruction::l(Opcode::Withe, 0, 0, 0x000f)
            .unwrap()
            .encode(),
        Instruction::j(Opcode::Call, 0, 3).unwrap().encode(),
        (Opcode::EndE as u32) << 26,
        Instruction::halt().encode(),
        Instruction::l(Opcode::Withe, 0, 0, 0).unwrap().encode(),
        (Opcode::EndE as u32) << 26,
        (Opcode::Ret as u32) << 26,
        Instruction::halt().encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();

    let report = machine.run(8).unwrap();

    assert_eq!(report.traces[4].emr, 0x000f);
    assert_eq!(report.traces[4].fsp, 1);
    assert_eq!(machine.emr(), 0xffff);
    assert_eq!(machine.fsp(), 0);
    assert!(machine.status().is_halted());
}

#[test]
fn io_store_requires_the_io_effect_and_emits_uart_output() {
    let program = [
        Instruction::l(Opcode::MovI, 0, 1, 0).unwrap().encode(),
        Instruction::l(Opcode::MovI, 0, 5, 65).unwrap().encode(),
        Instruction::m(Opcode::Store, 1, 5, 2, 1, 0)
            .unwrap()
            .encode(),
        Instruction::halt().encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();

    let report = machine.run(4).unwrap();

    assert!(report.status.is_halted());
    assert_eq!(machine.uart_output(), &[65]);
    assert_eq!(report.traces[2].mem_write, Some((0xF00000, Word::int(65))));
}

#[test]
fn conditional_branch_and_code_jump_validate_the_program_counter() {
    let branch_program = [
        Instruction::l(Opcode::MovI, 0, 1, 1).unwrap().encode(),
        Instruction::i(Opcode::Mktag, 9, 1, 1, Tag::Bool as i32)
            .unwrap()
            .encode(),
        Instruction::b(Opcode::BranchTrue, 0, 1, 2)
            .unwrap()
            .encode(),
        Instruction::l(Opcode::MovI, 0, 2, 0).unwrap().encode(),
        Instruction::l(Opcode::MovI, 0, 2, 7).unwrap().encode(),
        Instruction::halt().encode(),
    ];
    let mut branch_machine = Machine::boot(&branch_program).unwrap();
    branch_machine.run(5).unwrap();
    assert_eq!(branch_machine.reg(2), Word::int(7));

    let jump_program = [
        Instruction::l(Opcode::MovI, 0, 1, 4).unwrap().encode(),
        Instruction::i(Opcode::Mktag, 9, 1, 1, Tag::Code as i32)
            .unwrap()
            .encode(),
        Instruction::r(Opcode::Jump, 0, 1, 0, 0, 0)
            .unwrap()
            .encode(),
        Instruction::l(Opcode::MovI, 0, 2, 0).unwrap().encode(),
        Instruction::l(Opcode::MovI, 0, 2, 9).unwrap().encode(),
        Instruction::halt().encode(),
    ];
    let mut jump_machine = Machine::boot(&jump_program).unwrap();
    jump_machine.run(4).unwrap();
    assert_eq!(jump_machine.reg(2), Word::int(9));
}

#[test]
fn capability_introspection_and_special_capabilities_are_typed() {
    let program = [
        Instruction::c(Opcode::CSpecial, 0, 3, 0, 0, 1)
            .unwrap()
            .encode(),
        Instruction::c(Opcode::CGet, 0, 1, 3, 0, 0)
            .unwrap()
            .encode(),
        Instruction::c(Opcode::CGet, 0, 2, 3, 0, 1)
            .unwrap()
            .encode(),
        Instruction::halt().encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();

    machine.run(4).unwrap();

    assert_eq!(machine.reg(1), Word::int(0x4000));
    assert_eq!(machine.reg(2), Word::int(0x4000));
}

#[test]
fn trap_and_bad_program_counter_latch_architectural_causes() {
    let mut user_trap =
        Machine::boot(&[Instruction::j(Opcode::Trap, 0, 1).unwrap().encode()]).unwrap();
    let trace = user_trap.step().unwrap();
    assert_eq!(trace.trap, Some(0x81));
    assert_eq!(user_trap.tcause(), 0x81);

    let mut bad_pc =
        Machine::boot(&[Instruction::j(Opcode::Branch, 0, 0x10000).unwrap().encode()]).unwrap();
    let trace = bad_pc.step().unwrap();
    assert_eq!(trace.trap, Some(0x0c));
    assert_eq!(bad_pc.tval(), Word::int(0x10000));
}
