//! Discovery (`docs/spec/04-programs-and-packages.md`, "Discovery and the
//! source snapshot").

use std::fs;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_workspace::{discover_project, discover_project_at};

use crate::support::{LIBRARY, TempDir, failure, faults, link_file};

#[test]
fn discovery_finds_the_nearest_marker_from_a_directory_or_a_file() {
    let project = TempDir::project("nearest", LIBRARY, &[("src/app/a/b.vib", "")]);
    let from_directory = discover_project(project.join("src/app/a")).expect("found");
    assert_eq!(from_directory.root(), project.path());
    assert_eq!(from_directory.bytes(), LIBRARY.as_bytes());
    let from_file = discover_project(project.join("src/app/a/b.vib")).expect("found");
    assert_eq!(from_file.root(), project.path());
}

#[test]
fn a_directory_named_like_the_marker_is_not_a_marker() {
    let project =
        TempDir::project("not-file", LIBRARY, &[("inner/project.vibon/", "")]);
    let found = discover_project(project.join("inner")).expect("the outer marker");
    assert_eq!(found.root(), project.path());
}

#[test]
fn a_malformed_nearest_marker_stops_the_search() {
    let project = TempDir::project(
        "malformed",
        LIBRARY,
        &[("inner/project.vibon", "{format: @project.v1}")],
    );
    let diagnostics = failure(discover_project(project.join("inner")));
    assert_eq!(
        faults(&diagnostics),
        [(
            DiagnosticCode::DataInvalidValue,
            Some("project.vibon".to_owned())
        )]
    );
}

#[test]
fn no_marker_is_not_found_without_a_source() {
    let empty = TempDir::new("none");
    let diagnostics = failure(discover_project_at(empty.path()));
    assert_eq!(
        faults(&diagnostics),
        [(DiagnosticCode::ProjectNotFound, None)]
    );
    assert_eq!(diagnostics[0].primary_span(), ByteSpan::empty_at(0));
}

#[test]
fn a_linked_marker_is_a_project_io_error() {
    let outside = TempDir::new("marker-target");
    outside.write("real.vibon", LIBRARY);
    let project = TempDir::new("marker-link");
    link_file(&outside.join("real.vibon"), &project.join("project.vibon"));
    let diagnostics = failure(discover_project(project.path()));
    assert_eq!(
        faults(&diagnostics),
        [(
            DiagnosticCode::ProjectIoError,
            Some("project.vibon".to_owned())
        )]
    );
    assert!(fs::symlink_metadata(project.join("project.vibon")).is_ok());
}
