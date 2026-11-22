//! The workspace revision and its `toolchain` record (`docs/spec/05-tooling.md`,
//! "Snapshot, claim and witness identity").

use std::fmt::Write as _;
use std::path::Path;

use sha2::{Digest, Sha256};
use vibra_syntax::decode;
use vibra_types::{StdlibInputs, manifest_schema};
use vibra_workspace::{
    RevisionError, RevisionKind, RevisionRecord, toolchain_record, workspace_revision,
};

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
            (RevisionKind::Source, "src/app/main.vib"),
            (RevisionKind::Toolchain, "@toolchain")
        ]
    );
    assert_eq!(records[1].bytes(), b"a  b\r\n");
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::from("sha256:");
    for byte in Sha256::digest(bytes) {
        write!(text, "{byte:02x}").expect("a string");
    }
    text
}

fn hello() -> vibra_workspace::SourceSnapshot {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello");
    capture(&root).expect("examples/hello captures")
}

#[test]
fn every_snapshot_has_the_one_toolchain_record_of_the_chapter() {
    let records = hello().revision_records();
    let toolchains = records
        .iter()
        .filter(|record| record.kind() == RevisionKind::Toolchain)
        .collect::<Vec<_>>();
    let [toolchain] = toolchains.as_slice() else {
        panic!("one toolchain record: {toolchains:?}");
    };
    assert_eq!(toolchain.source_id(), "@toolchain");
    let manifest = StdlibInputs::embedded().manifest;
    let decoded = decode(&manifest_schema(), Some("stdlib/manifest.vibon"), manifest);
    let canonical = decoded
        .document()
        .expect("the manifest decodes")
        .canonical_bytes();
    let expected = format!(
        "{{format: @toolchain.v2 version: \"0.0.2\" grammar: \"sourceV2\" checker: \
         \"staticV2\" stdlib: \"{}\" drivers: \"systemV2\" policy: \"scopePolicyV1\" \
         registry: {{major: 2u32 minor: 0u32}}}}",
        hex(canonical.as_bytes())
    );
    assert_eq!(String::from_utf8_lossy(toolchain.bytes()), expected);
}

#[test]
fn the_revision_follows_the_library() {
    let snapshot = hello();
    let manifest = StdlibInputs::embedded().manifest.to_vec();
    let text = String::from_utf8(manifest.clone()).expect("UTF-8");
    let at =
        text.find("digest: \"sha256:").expect("a digest") + "digest: \"sha256:".len();
    let mut edited = manifest.clone();
    edited[at] = if edited[at] == b'0' { b'1' } else { b'0' };
    assert_eq!(edited.len(), manifest.len());
    let embedded = toolchain_record(&manifest).expect("the manifest decodes");
    let other = toolchain_record(&edited).expect("the edited manifest decodes");
    let first = workspace_revision(&snapshot.revision_records_with(embedded))
        .expect("a revision");
    let second =
        workspace_revision(&snapshot.revision_records_with(other)).expect("a revision");
    assert_ne!(first, second, "no project byte changed, the library did");
    assert_eq!(
        first,
        workspace_revision(&snapshot.revision_records()).expect("a revision")
    );
}

#[test]
fn a_revision_needs_exactly_one_toolchain_record() {
    let without = vector(SOURCE)
        .into_iter()
        .filter(|record| record.kind() != RevisionKind::Toolchain)
        .collect::<Vec<_>>();
    assert_eq!(
        workspace_revision(&without),
        Err(RevisionError::Toolchain { count: 0 })
    );
    let mut twice = vector(SOURCE);
    twice.push(RevisionRecord::new(RevisionKind::Toolchain, "@other", "x"));
    assert_eq!(
        workspace_revision(&twice),
        Err(RevisionError::Toolchain { count: 2 })
    );
}

/// The revision `conformance/tools/revisions.ps1 -Tree examples/hello`
/// computes from the bytes alone. A change to `examples/hello/` or to the
/// library's manifest changes it; the script gives the new value.
const SCRIPT_REVISION_OF_HELLO: &str =
    "sha256:4b02171e7fab033b4046b95344bfb76af566834b335f326d790ca1d8b5585e24";

#[test]
fn the_script_agrees_with_the_engine() {
    assert_eq!(
        workspace_revision(&hello().revision_records()).expect("a revision"),
        SCRIPT_REVISION_OF_HELLO
    );
}
