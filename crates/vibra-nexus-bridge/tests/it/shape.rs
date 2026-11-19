//! The default (shipped) build exposes the launcher types it exposed before the
//! harness reported a result: the dump and the counters are read inside the
//! `harness` feature, and nothing about them is part of the shipped surface.

use vibra_host::SystemProvider;
use vibra_nexus_bridge::{
    Execution, Finished, SYSTEM_MEMORY_LIMIT, execute, launch_system,
};

use crate::fixture;

#[test]
fn the_default_build_launcher_surface_is_unchanged() {
    let fixture = fixture::entry_ok();
    let descriptor = fixture.descriptor();
    let mut host = SystemProvider::with_arguments(Vec::new()).capturing();
    // SAFETY: the fixture owns every table the descriptor addresses.
    let execution = unsafe { execute(&descriptor, &mut host, SYSTEM_MEMORY_LIMIT, 1) };
    // An exhaustive pattern: a field added to `Execution` stops this compiling.
    let Execution {
        finished,
        audit,
        live_size,
        trap_origin,
        result_observed,
    } = execution;
    assert_eq!(finished, Finished::Completed);
    assert!(audit.is_empty());
    assert!(live_size > 0, "the result is still held");
    assert_eq!(trap_origin, None);
    assert!(result_observed);

    // The shipped launcher is still silent and exits as before.
    let mut stderr = Vec::new();
    // SAFETY: as above.
    let code = unsafe { launch_system(&descriptor, Vec::new(), &mut stderr) };
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
}

#[cfg(not(feature = "harness"))]
#[test]
fn the_default_build_has_no_dump_or_counter_code() {
    // The harness module is not compiled, and no source outside it names the
    // dump, the counters, or the walk of a result.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().is_some_and(|name| name == "harness.rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let code = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for word in ["dump_value", "Counters", "@truncated", "peak-live"] {
            assert!(!code.contains(word), "{} names `{word}`", path.display());
        }
    }
}
