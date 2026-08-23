use vibra_emu::{parse_hex_program, Instruction, Machine, Opcode};

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
    let mut machine = Machine::boot(&program).unwrap();
    let trace = machine.step().unwrap();
    assert_eq!(trace.pc, 0);
    assert_eq!(trace.insn, program[0]);
    assert_eq!(trace.trap, None);
    assert!(machine.status().is_halted());
}

#[test]
fn parser_output_can_be_loaded_as_a_machine_image() {
    let words = parse_hex_program("0xDC000000").unwrap();
    let mut machine = Machine::boot(&words).unwrap();
    assert_eq!(machine.step().unwrap().insn, Instruction::halt().encode());
    assert_eq!(machine.status().exit_code(), Some(0));
    assert_eq!(Opcode::Halt as u8, 0x37);
}

#[test]
fn run_collects_steps_until_halt_or_the_explicit_limit() {
    let mut machine = Machine::boot(&[Instruction::halt().encode()]).unwrap();
    let report = machine.run(1).unwrap();
    assert_eq!(report.steps, 1);
    assert!(!report.step_limit_reached);
    assert!(report.status.is_halted());
    assert_eq!(report.traces.len(), 1);

    let mut machine = Machine::boot(&[Instruction::nop().encode()]).unwrap();
    let report = machine.run(1).unwrap();
    assert_eq!(report.steps, 1);
    assert!(report.step_limit_reached);
    assert!(report.status.is_running());
}
