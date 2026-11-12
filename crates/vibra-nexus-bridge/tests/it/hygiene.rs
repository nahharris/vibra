//! The exported surface: symbol hygiene, the descriptor's layout, and the
//! launcher profiles.

use std::mem::{offset_of, size_of};
use std::path::PathBuf;

use vibra_nexus_bridge::{
    ROUTINES, VibraConstant, VibraFunction, VibraOrigin, VibraProgram, VibraTable,
};

fn sources() -> Vec<(PathBuf, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            (path, text)
        })
        .collect()
}

/// The names of the items the sources export with `no_mangle`.
fn exported(source: &str) -> Vec<String> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut names = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.trim() != "#[unsafe(no_mangle)]" {
            continue;
        }
        let item = lines[index + 1..]
            .iter()
            .find(|line| line.contains("fn ") || line.contains("static "))
            .expect("an item after no_mangle");
        let name = item
            .split("fn ")
            .nth(1)
            .or_else(|| item.split("static ").nth(1))
            .unwrap()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap();
        names.push(name.to_owned());
    }
    names
}

#[test]
fn the_routine_table_is_the_set_of_exported_symbols() {
    let mut in_sources = sources()
        .iter()
        .flat_map(|(_, source)| exported(source))
        .collect::<Vec<_>>();
    in_sources.sort();
    let mut table = ROUTINES
        .iter()
        .map(|routine| routine.name.to_owned())
        .collect::<Vec<_>>();
    table.sort();
    let launcher = in_sources.iter().position(|name| name == "main").unwrap();
    assert_eq!(
        in_sources.remove(launcher),
        "main",
        "the launcher is the one export that is not a routine"
    );
    assert_eq!(in_sources, table);
    for routine in ROUTINES {
        assert!(routine.name.starts_with("nexus_"), "{}", routine.name);
        assert!(
            routine.signature.contains(&format!(" {}(", routine.name)),
            "{}",
            routine.signature
        );
    }
    let mut unique = table.clone();
    unique.dedup();
    assert_eq!(unique, table, "no routine is listed twice");
}

#[test]
fn nothing_but_the_routines_and_main_is_exported() {
    for (path, source) in sources() {
        assert!(
            !source.contains("export_name"),
            "{} exports by another name",
            path.display()
        );
        assert!(
            !source.contains("#[no_mangle]"),
            "{} uses the pre-2024 attribute",
            path.display()
        );
    }
}

#[test]
fn the_descriptor_has_the_documented_layout() {
    assert_eq!(size_of::<VibraTable<u8>>(), 16);
    assert_eq!(offset_of!(VibraTable<u8>, len), 8);

    assert_eq!(size_of::<VibraProgram>(), 96);
    assert_eq!(offset_of!(VibraProgram, abi), 0);
    assert_eq!(offset_of!(VibraProgram, registry_minor), 4);
    assert_eq!(offset_of!(VibraProgram, entry), 8);
    assert_eq!(offset_of!(VibraProgram, reserved), 12);
    assert_eq!(offset_of!(VibraProgram, functions), 16);
    assert_eq!(offset_of!(VibraProgram, globals), 32);
    assert_eq!(offset_of!(VibraProgram, constants), 48);
    assert_eq!(offset_of!(VibraProgram, symbols), 64);
    assert_eq!(offset_of!(VibraProgram, origins), 80);

    assert_eq!(size_of::<VibraFunction>(), 24);
    assert_eq!(offset_of!(VibraFunction, step), 0);
    assert_eq!(offset_of!(VibraFunction, slots), 8);
    assert_eq!(offset_of!(VibraFunction, params), 12);
    assert_eq!(offset_of!(VibraFunction, env), 16);
    assert_eq!(offset_of!(VibraFunction, reserved), 20);

    assert_eq!(size_of::<VibraConstant>(), 24);
    assert_eq!(offset_of!(VibraConstant, kind), 0);
    assert_eq!(offset_of!(VibraConstant, reserved), 4);
    assert_eq!(offset_of!(VibraConstant, data), 8);

    assert_eq!(size_of::<VibraOrigin>(), 24);
    assert_eq!(offset_of!(VibraOrigin, source), 0);
    assert_eq!(offset_of!(VibraOrigin, start), 16);
    assert_eq!(offset_of!(VibraOrigin, end), 20);
}

#[test]
fn the_launcher_symbol_and_the_wait_status_are_what_the_generator_expects() {
    assert_eq!(vibra_nexus_bridge::PROGRAM_SYMBOL, "vibra_program");
    assert_eq!(vibra_nexus_bridge::STATUS_WAIT, 4);
}

#[cfg(not(feature = "harness"))]
#[test]
fn the_default_launcher_has_no_way_to_select_a_host_world() {
    assert_eq!(vibra_nexus_bridge::entry::PROFILE, "system");
    // The harness module is not compiled, and no source outside it names a
    // host world.
    for (path, source) in sources() {
        let is_harness = path.file_name().is_some_and(|name| name == "harness.rs");
        let code = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        if !is_harness {
            assert!(
                !code.contains("HostWorld") && !code.contains("WorldProvider"),
                "{} can select a host world",
                path.display()
            );
        }
    }
}

#[cfg(feature = "harness")]
#[test]
fn the_harness_profile_is_compiled_only_with_its_feature() {
    assert_eq!(vibra_nexus_bridge::entry::PROFILE, "harness");
}

#[cfg(feature = "harness")]
#[test]
fn the_harness_writes_one_observation_document_in_a_host_world() {
    use vibra_nexus_bridge::harness::launch_harness;

    let fixture = crate::fixture::demo(5, true);
    let descriptor = fixture.descriptor();
    for quota in [u64::MAX, 1] {
        let mut out = Vec::new();
        let mut diagnostics = Vec::new();
        // SAFETY: the fixture owns every table the descriptor addresses.
        let code = unsafe {
            launch_harness(
                &descriptor,
                "(record format: @host-world.v1)",
                quota,
                &mut out,
                &mut diagnostics,
            )
        };
        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&diagnostics));
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("(record outcome: @trapped result: @none trap: @runtime.division-by-zero origin: 2 audit: (record format: @audit-trace.v1"), "{text}");
        assert_eq!(text.lines().count(), 1);
    }
    let mut out = Vec::new();
    let mut diagnostics = Vec::new();
    // SAFETY: as above.
    let code =
        unsafe { launch_harness(&descriptor, "(((", 1, &mut out, &mut diagnostics) };
    assert_eq!(code, 2);
    assert!(out.is_empty());
}
