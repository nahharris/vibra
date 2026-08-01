use vibra_emu::{Capability, Machine, Permissions, Word};

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
