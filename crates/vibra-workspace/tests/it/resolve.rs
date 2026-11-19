//! The adapter from a snapshot to the resolver's input.

use vibra_resolve::resolve;
use vibra_workspace::resolve_input;

use crate::support::{LIBRARY, TempDir, capture};

#[test]
fn the_input_carries_the_package_every_unit_with_its_bytes_and_the_library() {
    let project = TempDir::project(
        "resolve",
        LIBRARY,
        &[
            (
                "src/app/model.vib",
                "type User of U8\n  visibility @public\nendtype\n",
            ),
            ("tests/model.vib", "use @app.model.User enduse\n"),
        ],
    );
    let snapshot = capture(project.path()).expect("captured");
    let input = resolve_input(&snapshot).expect("the embedded library loads");
    assert_eq!(input.package().name(), "hello");
    assert_eq!(input.package().version(), "0.1.0");
    let units = input
        .units()
        .iter()
        .map(|unit| {
            (
                unit.name().to_owned(),
                unit.kind(),
                unit.modules()
                    .iter()
                    .map(|module| {
                        (
                            module.path().to_owned(),
                            module.source_id().to_owned(),
                            module.bytes().to_vec(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        units,
        [
            (
                "@app".to_owned(),
                vibra_resolve::UnitKind::Lib,
                vec![(
                    "@app.model".to_owned(),
                    "src/app/model.vib".to_owned(),
                    b"type User of U8\n  visibility @public\nendtype\n".to_vec()
                )]
            ),
            (
                "@tests".to_owned(),
                vibra_resolve::UnitKind::Tests,
                vec![(
                    "@tests.model".to_owned(),
                    "tests/model.vib".to_owned(),
                    b"use @app.model.User enduse\n".to_vec()
                )]
            ),
        ]
    );
    assert_eq!(input.embedded().name(), "@std");
    assert_eq!(input.embedded().modules().len(), 6);
    let resolution = resolve(&input);
    assert!(
        resolution.diagnostics().is_empty(),
        "{:?}",
        resolution.diagnostics()
    );
    assert!(resolution.imports()[0].to().is_some());
}
