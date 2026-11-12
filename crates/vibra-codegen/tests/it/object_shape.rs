//! The symbols and relocations of the emitted object, and the descriptor
//! decoded back from its data sections.

use std::collections::BTreeSet;
use std::mem::{offset_of, size_of};

use object::{BinaryFormat, Object, ObjectSection, ObjectSymbol, RelocationTarget};
use vibra_exec::{Constant, MAX_SLOTS, Program, Terminator, verified};
use vibra_nexus_bridge::{
    ABI_VERSION, CONSTANT_ATOM, CONSTANT_BYTES, CONSTANT_STR, MAX_FRAME_SLOTS,
    PROGRAM_SYMBOL, ROUTINES, VibraConstant, VibraFunction, VibraOrigin, VibraProgram,
    VibraTable,
};

use crate::image::{self, Image, Place};
use crate::programs::{self, block, function, program, ret, scalar};
use crate::{TARGETS, emitted, inputs};

fn routine_names() -> BTreeSet<&'static str> {
    ROUTINES.iter().map(|routine| routine.name).collect()
}

/// The registry symbols a program's host calls name, each once, in first-use
/// order.
fn host_symbols(program: &Program) -> Vec<&'static str> {
    let mut symbols: Vec<&'static str> = Vec::new();
    for function in &program.functions {
        for block in &function.blocks {
            if let Terminator::Host { row, .. } = &block.term
                && !symbols.contains(&row.symbol)
            {
                symbols.push(row.symbol);
            }
        }
    }
    symbols
}

#[test]
fn the_object_defines_exactly_the_expected_symbols() {
    for (name, program) in programs::all() {
        for target in TARGETS {
            let object = emitted(&program, &target);
            let file = image::parse(object.as_bytes());
            let context = format!("{name} for {}", target.triple());

            assert_eq!(
                image::defined_globals(&file),
                BTreeSet::from([PROGRAM_SYMBOL.to_owned()]),
                "{context}: `vibra_program` is the only global definition"
            );

            let routines = routine_names();
            let undefined = image::undefined(&file);
            for symbol in &undefined {
                assert!(
                    routines.contains(symbol.as_str()),
                    "{context}: `{symbol}` is undefined and is no routine of the bridge"
                );
            }

            let functions = (0..program.functions.len())
                .map(|index| format!("vibra_fn_{index}"))
                .collect::<BTreeSet<_>>();
            let locals = image::locals(&file);
            assert!(
                functions.is_subset(&locals),
                "{context}: every function is a local symbol"
            );
            for local in &locals {
                // On COFF, Cranelift reaches an imported routine through a
                // pointer slot named `.refptr.<symbol>` in read-only data, and
                // reaches the operand lists the same way.
                let slot = local.strip_prefix(".refptr.");
                assert!(
                    local.starts_with("vibra_")
                        || slot.is_some_and(|data| routines.contains(data)
                            || data.starts_with("vibra_list_")),
                    "{context}: unexpected local `{local}`"
                );
            }
        }
    }
}

/// The routines the instructions and terminators of `program` need, and the
/// trap every step uses for a block it has no code for.
fn needed_routines(program: &Program) -> BTreeSet<&'static str> {
    let mut needed = BTreeSet::from(["nexus_trap"]);
    for function in &program.functions {
        for block in &function.blocks {
            for inst in &block.insts {
                needed.insert(crate::clif::inst_routine(inst));
            }
            needed.extend(crate::clif::terminator_routine(&block.term));
        }
    }
    needed
}

#[test]
fn the_undefined_symbols_are_exactly_the_routines_the_program_needs() {
    for (name, program) in programs::all() {
        for target in TARGETS {
            let object = emitted(&program, &target);
            let file = image::parse(object.as_bytes());
            let undefined = image::undefined(&file);
            let needed = needed_routines(&program)
                .into_iter()
                .map(str::to_owned)
                .collect::<BTreeSet<_>>();
            assert_eq!(undefined, needed, "{name} for {}", target.triple());
        }
    }
}

#[test]
fn no_generated_function_calls_another_generated_function() {
    for (name, program) in programs::all() {
        for target in TARGETS {
            let object = emitted(&program, &target);
            let file = image::parse(object.as_bytes());
            let routines = routine_names();
            let mut to_routines = 0;
            let mut to_data = 0;
            for section in image::code_sections(&file) {
                for (_, relocation) in section.relocations() {
                    match relocation.target() {
                        RelocationTarget::Symbol(index) => {
                            let symbol = file.symbol_by_index(index).expect("a symbol");
                            let name_of = symbol.name().expect("a name");
                            if symbol.is_undefined() {
                                assert!(
                                    routines.contains(name_of),
                                    "{name}: code relocates to `{name_of}`, which is no routine"
                                );
                                to_routines += 1;
                            } else {
                                let object::SymbolSection::Section(index) =
                                    symbol.section()
                                else {
                                    panic!("{name}: `{name_of}` has no section");
                                };
                                let kind = file
                                    .section_by_index(index)
                                    .expect("section")
                                    .kind();
                                assert_ne!(
                                    kind,
                                    object::SectionKind::Text,
                                    "{name}: code relocates to the code symbol `{name_of}`"
                                );
                                // On COFF a routine is reached through the
                                // pointer slot `.refptr.<routine>`.
                                if name_of
                                    .strip_prefix(".refptr.")
                                    .is_some_and(|routine| routines.contains(routine))
                                {
                                    to_routines += 1;
                                } else {
                                    to_data += 1;
                                }
                            }
                        }
                        RelocationTarget::Section(index) => {
                            let kind =
                                file.section_by_index(index).expect("section").kind();
                            assert_ne!(
                                kind,
                                object::SectionKind::Text,
                                "{name}: code relocates to a code section"
                            );
                            to_data += 1;
                        }
                        other => {
                            panic!("{name}: unexpected relocation target {other:?}")
                        }
                    }
                }
            }
            assert!(to_routines > 0, "{name}: code calls the routines");
            // Operand lists are read-only data. Every program here has a call
            // or a build, except none; `deep` and the others do.
            assert!(to_data > 0, "{name}: operand lists are data symbols");
        }
    }
}

#[test]
fn the_object_is_in_the_format_of_its_target() {
    let program = programs::demo(1, false);
    let windows = emitted(&program, &TARGETS[0]);
    assert_eq!(
        image::parse(windows.as_bytes()).format(),
        BinaryFormat::Coff
    );
    let linux = emitted(&program, &TARGETS[1]);
    assert_eq!(image::parse(linux.as_bytes()).format(), BinaryFormat::Elf);
}

#[test]
fn the_descriptor_layout_is_the_bridges() {
    assert_eq!(size_of::<VibraProgram>(), 96);
    assert_eq!(size_of::<VibraFunction>(), 24);
    assert_eq!(size_of::<VibraConstant>(), 24);
    assert_eq!(size_of::<VibraOrigin>(), 24);
    assert_eq!(size_of::<VibraTable<u8>>(), 16);
    let program = programs::demo(1, false);
    for target in TARGETS {
        let object = emitted(&program, &target);
        let file = image::parse(object.as_bytes());
        let place = image::symbol_place(&file, PROGRAM_SYMBOL).expect("placed");
        assert_eq!(place.1 % 8, 0, "the descriptor is 8-aligned");
        let image = Image::new(&file);
        assert_eq!(image.bytes(place, size_of::<VibraProgram>()).len(), 96);
    }
}

fn text(image: &Image, table: (Option<Place>, usize)) -> String {
    match table {
        (Some(place), len) => {
            String::from_utf8(image.bytes(place, len).to_vec()).expect("UTF-8")
        }
        (None, _) => String::new(),
    }
}

fn data(image: &Image, table: (Option<Place>, usize)) -> Vec<u8> {
    match table {
        (Some(place), len) => image.bytes(place, len).to_vec(),
        (None, _) => Vec::new(),
    }
}

/// Reads every field of the descriptor in `bytes` and compares it with
/// `program`.
fn check_descriptor(label: &str, program: &Program, bytes: &[u8]) {
    let file = image::parse(bytes);
    let image = Image::new(&file);
    let base = image::symbol_place(&file, PROGRAM_SYMBOL).expect("the descriptor");
    let at = |offset: usize| (base.0, base.1 + u64::try_from(offset).expect("small"));
    let given = inputs(program);

    assert_eq!(
        image.u32(at(offset_of!(VibraProgram, abi))),
        ABI_VERSION,
        "{label}"
    );
    assert_eq!(
        image.u32(at(offset_of!(VibraProgram, registry_minor))),
        given.registry_minor,
        "{label}"
    );
    assert_eq!(
        image.u32(at(offset_of!(VibraProgram, entry))),
        program.entry.0,
        "{label}"
    );
    assert_eq!(
        image.u32(at(offset_of!(VibraProgram, entry_outcome))),
        given.entry_outcome,
        "{label}"
    );

    // The functions.
    let (functions, count) = image.table(at(offset_of!(VibraProgram, functions)));
    assert_eq!(count, program.functions.len(), "{label}: function count");
    let functions = functions.expect("a function table");
    for (index, source) in program.functions.iter().enumerate() {
        let row = (
            functions.0,
            functions.1
                + u64::try_from(index * size_of::<VibraFunction>()).expect("small"),
        );
        let field =
            |offset: usize| (row.0, row.1 + u64::try_from(offset).expect("small"));
        assert_eq!(
            image.u32(field(offset_of!(VibraFunction, slots))),
            source.slots
        );
        assert_eq!(
            image.u32(field(offset_of!(VibraFunction, params))),
            source.params
        );
        assert_eq!(
            image.u32(field(offset_of!(VibraFunction, env))),
            u32::from(source.env)
        );
        assert_eq!(
            usize::try_from(image.u32(field(offset_of!(VibraFunction, blocks))))
                .expect("small"),
            source.blocks.len(),
            "{label}: the block count of function {index}"
        );
        assert!(
            source.slots <= MAX_SLOTS && source.slots <= MAX_FRAME_SLOTS,
            "{label}: the frame bound"
        );
        let step = image
            .pointer(field(offset_of!(VibraFunction, step)))
            .expect("a step pointer");
        let local = image::symbol_place(&file, &format!("vibra_fn_{index}"))
            .expect("the step function");
        assert_eq!(step, local, "{label}: the step of function {index}");
    }

    // The module values.
    let (globals, count) = image.table(at(offset_of!(VibraProgram, globals)));
    assert_eq!(count, program.globals.len(), "{label}");
    for (index, initializer) in program.globals.iter().enumerate() {
        let place = globals.expect("a table");
        assert_eq!(
            image.u32((place.0, place.1 + u64::try_from(index * 4).expect("small"))),
            initializer.0,
            "{label}: initializer {index}"
        );
    }

    // The constants.
    let (constants, count) = image.table(at(offset_of!(VibraProgram, constants)));
    assert_eq!(count, program.constants.len(), "{label}");
    for (index, source) in program.constants.iter().enumerate() {
        let place = constants.expect("a table");
        let row = (
            place.0,
            place.1 + u64::try_from(index * size_of::<VibraConstant>()).expect("small"),
        );
        let field =
            |offset: usize| (row.0, row.1 + u64::try_from(offset).expect("small"));
        let (kind, expected) = match source {
            Constant::Str(text) => (CONSTANT_STR, text.as_bytes().to_vec()),
            Constant::Bytes(bytes) => (CONSTANT_BYTES, bytes.clone()),
            Constant::Atom(text) => (CONSTANT_ATOM, text.as_bytes().to_vec()),
        };
        assert_eq!(
            image.u32(field(offset_of!(VibraConstant, kind))),
            kind,
            "{label}"
        );
        assert_eq!(
            image.u32(field(offset_of!(VibraConstant, reserved))),
            0,
            "{label}"
        );
        let bytes = data(&image, image.table(field(offset_of!(VibraConstant, data))));
        assert_eq!(bytes, expected, "{label}: constant {index}");
    }

    // The host symbols.
    let (symbols, count) = image.table(at(offset_of!(VibraProgram, symbols)));
    let expected = host_symbols(program);
    assert_eq!(count, expected.len(), "{label}");
    for (index, symbol) in expected.iter().enumerate() {
        let place = symbols.expect("a table");
        let row = (
            place.0,
            place.1
                + u64::try_from(index * size_of::<VibraTable<u8>>()).expect("small"),
        );
        assert_eq!(
            text(&image, image.table(row)),
            *symbol,
            "{label}: symbol {index}"
        );
    }

    // The origins.
    let (origins, count) = image.table(at(offset_of!(VibraProgram, origins)));
    assert_eq!(count, program.origins.len(), "{label}");
    for (index, source) in program.origins.iter().enumerate() {
        let place = origins.expect("a table");
        let row = (
            place.0,
            place.1 + u64::try_from(index * size_of::<VibraOrigin>()).expect("small"),
        );
        let field =
            |offset: usize| (row.0, row.1 + u64::try_from(offset).expect("small"));
        assert_eq!(
            text(&image, image.table(field(offset_of!(VibraOrigin, source)))),
            source.source_id(),
            "{label}: origin {index}"
        );
        assert_eq!(
            usize::try_from(image.u32(field(offset_of!(VibraOrigin, start))))
                .expect("small"),
            source.span().start()
        );
        assert_eq!(
            usize::try_from(image.u32(field(offset_of!(VibraOrigin, end))))
                .expect("small"),
            source.span().end()
        );
    }

    // The reserved bytes of the descriptor are zero: the descriptor's four
    // scalar fields and five tables account for all 96 bytes.
    assert_eq!(
        offset_of!(VibraProgram, origins) + size_of::<VibraTable<VibraOrigin>>(),
        size_of::<VibraProgram>()
    );
}

#[test]
fn the_descriptor_bytes_decode_to_the_program() {
    for (name, program) in programs::all() {
        for target in TARGETS {
            let object = emitted(&program, &target);
            check_descriptor(
                &format!("{name} for {}", target.triple()),
                &program,
                object.as_bytes(),
            );
        }
    }
}

#[test]
fn the_host_symbols_are_deduplicated_in_first_use_order() {
    let program = programs::everything();
    // `main` names stdout, then stdin; no later function repeats either.
    assert_eq!(host_symbols(&program), ["io.stdout-text", "io.stdin-text"]);
    let repeated = programs::demo(2, false);
    assert_eq!(host_symbols(&repeated), ["io.stdout-text"]);
    for target in TARGETS {
        let object = emitted(&program, &target);
        check_descriptor("everything", &program, object.as_bytes());
    }
}

#[test]
fn a_frame_at_the_bound_is_emitted_and_a_frame_over_it_never_reaches_the_generator() {
    assert_eq!(MAX_SLOTS, MAX_FRAME_SLOTS);
    let at_bound = program(vec![function(
        "main",
        0,
        false,
        MAX_SLOTS,
        vec![block(vec![scalar(0, 0)], ret(0))],
    )]);
    for target in TARGETS {
        let object = emitted(&at_bound, &target);
        check_descriptor("at the bound", &at_bound, object.as_bytes());
    }
    let over = program(vec![function(
        "main",
        0,
        false,
        MAX_SLOTS + 1,
        vec![block(vec![scalar(0, 0)], ret(0))],
    )]);
    assert!(
        verified(&over).is_err(),
        "`emit` takes a `Verified`, and an oversized frame cannot be one"
    );
}
