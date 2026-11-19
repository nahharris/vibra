//! The workspace revision (`docs/spec/05-tooling.md`, "Snapshot, claim and
//! witness identity").

use vibra_workspace::{RevisionKind, RevisionRecord, workspace_revision};

use crate::support::{LIBRARY, TempDir, capture};

fn vector(source: &str) -> Vec<RevisionRecord> {
    vec![
        RevisionRecord::new(RevisionKind::Source, "src/main.vib", source),
        RevisionRecord::new(RevisionKind::Toolchain, "@toolchain", "toolchain"),
        RevisionRecord::new(RevisionKind::Project, "project.vibon", "project"),
    ]
}

const SOURCE: &str = "fn f returns answer: Str do return \"ok\" endfn";

#[test]
fn the_chapters_test_vector() {
    assert_eq!(
        workspace_revision(&vector(SOURCE)).expect("unique records"),
        "sha256:5fdc01744c45544c5765d4ec386b7a45bb990bc4b4c0ca415493546c7f36c8ec"
    );
}

#[test]
fn an_equal_length_edit_changes_the_revision() {
    let edited = SOURCE.replace("\"ok\"", "\"no\"");
    assert_eq!(edited.len(), SOURCE.len());
    assert_ne!(
        workspace_revision(&vector(&edited)).expect("unique"),
        workspace_revision(&vector(SOURCE)).expect("unique")
    );
}

#[test]
fn two_records_with_one_identity_are_refused() {
    let mut records = vector(SOURCE);
    records.push(RevisionRecord::new(
        RevisionKind::Source,
        "src/main.vib",
        "x",
    ));
    assert!(workspace_revision(&records).is_err());
}

#[test]
fn the_snapshot_hashes_its_exact_captured_bytes() {
    let project =
        TempDir::project("revision", LIBRARY, &[("src/app/main.vib", "a  b\r\n")]);
    let snapshot = capture(project.path()).expect("captured");
    let records = snapshot.revision_records();
    let ids = records
        .iter()
        .map(|record| (record.kind(), record.source_id()))
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            (RevisionKind::Project, "project.vibon"),
            (RevisionKind::Source, "src/app/main.vib")
        ]
    );
    assert_eq!(records[1].bytes(), b"a  b\r\n");
}
