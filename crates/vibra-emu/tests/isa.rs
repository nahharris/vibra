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
