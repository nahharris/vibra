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
        Capability::new(
            0,
            0x10000,
            Permissions::EXECUTE | Permissions::DERIVE,
            0,
        )
        .unwrap()
    );
    assert_eq!(
        machine.hpc(),
        Capability::new(
            0x4000,
            0x4000,
            Permissions::READ
                | Permissions::WRITE
                | Permissions::ALLOCATE
                | Permissions::DERIVE,
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
        Instruction::i(Opcode::MovH, 0, 1, 0, 0x1fff).unwrap().encode(),
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
        Instruction::r(Opcode::BNot, 0, 2, 1, 0, 0).unwrap().encode(),
    ];
    let mut machine = Machine::boot(&program).unwrap();
    machine.step().unwrap();
    let trace = machine.step().unwrap();
    assert_eq!(trace.trap, Some(0x01));
}
