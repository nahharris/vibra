//! M3 Step 16: the checked-in M3 demo checks, passes its tests, runs, and is
//! in canonical format with the actual binary; its index and a position query
//! report the Stage 3B facts.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::process::{Command, Output};

use vibra_workspace::WorkspaceSnapshot;

const SOURCES: [&str; 3] = [
    "src/catalog/stock.vib",
    "src/app/main.vib",
    "tests/stock.vib",
];

fn demo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/m3-catalog")
        .canonicalize()
        .expect("demo project")
}

fn vibra(arguments: &[&str]) -> Output {
    let workspace = demo().to_string_lossy().into_owned();
    Command::new(env!("CARGO_BIN_EXE_vibra"))
        .args(["--format", "json", "--workspace", &workspace])
        .args(arguments)
        .output()
        .expect("vibra binary")
}

fn envelope(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("json envelope")
}

#[test]
fn the_m3_demo_checks_tests_and_runs() {
    let check = vibra(&["check"]);
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    assert_eq!(envelope(&check)["payload"]["accepted"], true);

    let test = vibra(&["test"]);
    assert_eq!(test.status.code(), Some(0), "{test:?}");
    let test = envelope(&test);
    assert_eq!(test["result"], "@command.ok");
    assert_eq!(test["payload"]["selected"], 7);
    assert_eq!(test["payload"]["passed"], 7);

    let run = vibra(&["run", "src/app"]);
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    let run = envelope(&run);
    assert_eq!(run["result"], "@command.ok");
    assert!(
        run["payload"]["programResult"]
            .as_str()
            .is_some_and(|result| result.contains("variant: @ok")),
        "{run}"
    );
}

#[test]
fn the_m3_demo_is_in_canonical_format() {
    for source in SOURCES {
        let written = std::fs::read_to_string(demo().join(source)).expect("source");
        let output = Command::new(env!("CARGO_BIN_EXE_vibra"))
            .args(["--workspace", &demo().to_string_lossy(), "fmt", source])
            .output()
            .expect("vibra binary");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        // Compare without the host's line endings: a checkout may rewrite them.
        let formatted = String::from_utf8(output.stdout).expect("utf-8");
        assert_eq!(
            formatted.replace("\r\n", "\n"),
            written.replace("\r\n", "\n"),
            "{source}"
        );
    }
}

#[test]
fn the_m3_demo_index_names_every_implementation() {
    let workspace = WorkspaceSnapshot::load_confined(demo()).expect("workspace");
    let index =
        vibra_workspace::index::index_with_embedded_stdlib(&workspace).expect("index");
    let again =
        vibra_workspace::index::index_with_embedded_stdlib(&workspace).expect("index");
    assert_eq!(index.canonical_vibon(), again.canonical_vibon());

    let blocks = index
        .implementations()
        .iter()
        .map(|block| (block.receiver(), block.interface()))
        .collect::<Vec<_>>();
    for (receiver, interface) in [
        ("@catalog.stock.sku", "@std.core.ordered"),
        (
            "@catalog.stock.sku",
            "(record type: @std.core.from arguments: (array @u32))",
        ),
        ("@catalog.stock.part", "@catalog.stock.priced"),
        ("@catalog.stock.service", "@catalog.stock.priced"),
        (
            "(record type: @catalog.stock.stack arguments: (array (record type: @param name: @t)))",
            "(record type: @std.iter.iter arguments: (array (record type: @param name: @t)))",
        ),
    ] {
        assert!(
            blocks.contains(&(receiver, interface)),
            "{receiver} {interface}: {blocks:?}"
        );
    }

    let total = index
        .declarations()
        .iter()
        .find(|declaration| declaration.id() == "catalog.stock.total")
        .expect("total");
    assert_eq!(total.module(), "catalog.stock");
}

#[test]
fn a_position_query_reports_the_selected_conversion() {
    let workspace = WorkspaceSnapshot::load_confined(demo()).expect("workspace");
    let source = std::fs::read_to_string(demo().join(SOURCES[0]))
        .expect("source")
        .replace("\r\n", "\n");
    let query = workspace
        .query_position_with_embedded_stdlib(
            SOURCES[0],
            source.find("(from.convert number)").expect("conversion"),
        )
        .expect("query");
    let application = query.application().value().expect("application");
    let dispatch = application.dispatch().expect("dispatch");
    assert_eq!(dispatch.interface(), "std.core.from");
    assert_eq!(dispatch.receiver().name(), "catalog.stock.sku");
    assert!(dispatch.destination());
}
