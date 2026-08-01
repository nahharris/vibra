use vibra_emu::{Capability, Format, Instruction, Opcode, Permissions, Tag, Word};

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
fn typed_constructors_build_valid_boolean_and_character_words() {
    assert_eq!(Word::bool(true).payload(), 1);
    assert_eq!(Word::bool(false).payload(), 0);
    assert_eq!(Word::bool(true).tag(), Tag::Bool);
    assert_eq!(Word::char('λ').unwrap().payload(), 'λ' as u32);
    assert_eq!(Word::char('λ').unwrap().tag(), Tag::Char);
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

#[test]
fn tags_reject_reserved_values() {
    assert!(Tag::try_from(9).is_err());
    assert_eq!(Tag::try_from(Tag::Sealed as u8).unwrap(), Tag::Sealed);
}

#[test]
fn instruction_fields_use_the_fixed_v01_positions() {
    let instruction = Instruction::i(Opcode::AddI, 6, 3, 2, -17).unwrap();
    assert_eq!(instruction.encode(), 0x358c_bfef);

    let decoded = Instruction::decode(instruction.encode()).unwrap();
    assert_eq!(decoded.format(), Format::I);
    assert_eq!(decoded.eff(), 6);
    assert_eq!(decoded.rd(), 3);
    assert_eq!(decoded.rs1(), 2);
    assert_eq!(decoded.imm14(), -17);
}

#[test]
fn every_defined_opcode_has_a_stable_format() {
    for &(opcode, format) in Opcode::all() {
        assert_eq!(opcode.format(), format);
    }
    assert_eq!(Opcode::try_from(0x3a).is_err(), true);
}

#[test]
fn instruction_builders_reject_values_that_do_not_fit_their_fields() {
    assert!(Instruction::r(Opcode::Mov, 16, 0, 0, 0, 0).is_err());
    assert!(Instruction::r(Opcode::Mov, 0, 16, 0, 0, 0).is_err());
    assert!(Instruction::i(Opcode::AddI, 0, 0, 0, 8192).is_err());
    assert!(Instruction::l(Opcode::MovI, 0, 0, 131072).is_err());
    assert!(Instruction::j(Opcode::Branch, 0, 0x20_0000).is_err());
}

#[test]
fn m_and_c_formats_keep_capability_selectors_in_the_documented_fields() {
    let memory = Instruction::m(Opcode::Load, 1, 4, 2, 3, -7).unwrap();
    assert_eq!(Instruction::decode(memory.encode()).unwrap(), memory);
    assert_eq!(memory.rd(), 4);
    assert_eq!(memory.cap(), 2);
    assert_eq!(memory.base_reg(), 3);
    assert_eq!(memory.imm10(), -7);

    let derive = Instruction::c(Opcode::CDerive, 0, 4, 2, 3, 1).unwrap();
    assert_eq!(Instruction::decode(derive.encode()).unwrap(), derive);
    assert_eq!(derive.cd(), 4);
    assert_eq!(derive.cs(), 2);
    assert_eq!(derive.aux_reg(), 1);
}
