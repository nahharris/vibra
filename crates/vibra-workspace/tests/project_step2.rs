//! Step 2 host tests for the typed `@project.v1` decoder.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::Path;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_syntax::{DataValue, parse_data};
use vibra_workspace::project::{
    EntityKind, ProjectAtomRole, ProjectDecoder, ProjectOrigin, TargetKind,
};

fn decode(source: &str) -> vibra_workspace::project::ProjectDecode {
    let document = parse_data(Path::new("project.vibon"), source).expect("data loader");
    assert!(
        document.accepted(),
        "generic data failed: {:?}",
        document.diagnostics()
    );
    let root = document.data().expect("generic data root");
    ProjectDecoder::decode(root, ProjectOrigin::new("project.vibon"))
}

#[test]
fn decodes_a_binary_project_without_resolving_reference_atoms() {
    let decoded = decode(
        "(record format: @project.v1 package: (record version: \"0.1.0\" name: \"hello\") targets: (array (record effects: (array @std.fs.read) entry: @hello.main.main root: \"src/hello\" kind: @bin name: @hello)) dependencies: (map @std (record target: @core rev: \"0123456789abcdef0123456789abcdef01234567\" git: \"https://example.com/std.git\" kind: @git)))",
    );

    assert!(decoded.accepted(), "{:?}", decoded.diagnostics());
    let project = decoded.project().expect("typed project");
    assert_eq!(project.format().atom().value(), "project.v1");
    assert_eq!(project.package().name().value(), "hello");
    assert_eq!(project.fields().len(), 4);
    assert_eq!(project.fields()[0].label().value(), "format");
    assert_eq!(project.fields()[0].origin().source_id(), "project.vibon");
    assert_eq!(project.targets().len(), 1);
    let target = &project.targets()[0];
    assert_eq!(target.kind(), TargetKind::Bin);
    assert_eq!(target.kind_atom().raw(), "@bin");
    assert_eq!(target.kind_atom().role(), ProjectAtomRole::Value);
    assert_eq!(target.fields().len(), 5);
    assert_eq!(
        target.entry().expect("binary entry").atom().value(),
        "hello.main.main"
    );
    assert_eq!(
        target.entry().expect("binary entry").role(),
        ProjectAtomRole::Reference(EntityKind::Declaration)
    );
    assert_eq!(
        target.entry().expect("binary entry").origin().source_id(),
        "project.vibon"
    );
    assert_eq!(project.dependencies().len(), 1);
    assert_eq!(project.dependencies()[0].alias().atom().value(), "std");
    assert_eq!(
        project.dependencies()[0]
            .target()
            .expect("dependency target")
            .role(),
        ProjectAtomRole::Reference(EntityKind::LibraryTarget)
    );
}

#[test]
fn accepts_a_library_omitting_binary_only_fields_and_empty_dependencies() {
    let decoded = decode(
        "(record dependencies: (map) targets: (array (record root: \"src\" kind: @lib name: @core)) package: (record name: \"my-lib\" version: \"1.2.3-alpha.1\") format: @project.v1)",
    );

    assert!(decoded.accepted(), "{:?}", decoded.diagnostics());
    let target = &decoded.project().expect("typed project").targets()[0];
    assert_eq!(target.kind(), TargetKind::Lib);
    assert!(target.entry().is_none());
    assert!(target.effects().is_none());
}

#[test]
fn rejects_unknown_and_missing_fields_at_nested_record_boundaries() {
    let unknown = decode(
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\" extra: \"x\") targets: (array (record name: @hello kind: @bin root: \"src\" entry: @hello.main effects: (array))) dependencies: (map))",
    );
    assert!(!unknown.accepted());
    assert_eq!(
        unknown.diagnostics()[0].code(),
        DiagnosticCode::DataInvalidShape
    );

    let missing = decode(
        "(record format: @project.v1 package: (record name: \"hello\") targets: (array (record name: @hello kind: @bin root: \"src\" entry: @hello.main effects: (array))) dependencies: (map))",
    );
    assert!(!missing.accepted());
    assert_eq!(
        missing.diagnostics()[0].code(),
        DiagnosticCode::DataInvalidShape
    );
}

#[test]
fn validates_version_names_dependency_forms_and_binary_library_exclusivity() {
    let cases = [
        (
            "(record format: @project.v2 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @bin root: \"src\" entry: @hello.main effects: (array))) dependencies: (map))",
            DiagnosticCode::DataInvalidValue,
        ),
        (
            "(record format: @project.v1 package: (record name: \"Hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @bin root: \"src\" entry: @hello.main effects: (array))) dependencies: (map))",
            DiagnosticCode::DataInvalidValue,
        ),
        (
            "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1\") targets: (array (record name: @hello kind: @bin root: \"src\" entry: @hello.main effects: (array))) dependencies: (map))",
            DiagnosticCode::DataInvalidValue,
        ),
        (
            "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @lib root: \"src\" entry: @hello.main effects: (array))) dependencies: (map))",
            DiagnosticCode::DataInvalidShape,
        ),
        (
            "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @bin root: \"src\" entry: @hello.main effects: (array))) dependencies: (map @std (record kind: @git git: \"http://example.com/std.git\" rev: \"0123456789abcdef0123456789abcdef01234567\")))",
            DiagnosticCode::DataInvalidValue,
        ),
    ];
    for (source, code) in cases {
        let decoded = decode(source);
        assert!(!decoded.accepted(), "accepted invalid project: {source}");
        assert_eq!(decoded.diagnostics()[0].code(), code, "{source}");
    }
}

#[test]
fn rejects_wrong_kinds_empty_targets_and_unknown_dependency_fields() {
    let cases = [
        "(record format: @project.v1 package: (array) targets: (array (record name: @hello kind: @lib root: \"src\")) dependencies: (map))",
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array 1) dependencies: (map))",
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array) dependencies: (map))",
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @lib root: \"src\")) dependencies: (map @local (record kind: @path path: \"../local\" extra: true)))",
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @lib root: \"src\")) dependencies: (map @remote (record kind: @git git: \"https://example.com/repo.git\" rev: \"0123456789abcdef0123456789abcdef01234567\" extra: true)))",
    ];
    for source in cases {
        let decoded = decode(source);
        assert!(!decoded.accepted(), "accepted malformed project: {source}");
        assert_eq!(
            decoded.diagnostics()[0].code(),
            DiagnosticCode::DataInvalidShape
        );
    }
}

#[test]
fn unit_names_are_single_kebab_atom_components() {
    let qualified_target = decode(
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello.main kind: @lib root: \"src\")) dependencies: (map))",
    );
    assert!(!qualified_target.accepted());
    assert_eq!(
        qualified_target.diagnostics()[0].code(),
        DiagnosticCode::DataInvalidValue
    );

    let qualified_alias = decode(
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @lib root: \"src\")) dependencies: (map @std.core (record kind: @path path: \"../std\")))",
    );
    assert!(!qualified_alias.accepted());
    assert_eq!(
        qualified_alias.diagnostics()[0].code(),
        DiagnosticCode::DataInvalidValue
    );
}

#[test]
fn rejects_non_record_dependencies_and_invalid_git_revisions() {
    let non_record = decode(
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @lib root: \"src\")) dependencies: (map @local (array)))",
    );
    assert!(!non_record.accepted());
    assert_eq!(
        non_record.diagnostics()[0].code(),
        DiagnosticCode::DataInvalidShape
    );

    let invalid_revision = decode(
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @lib root: \"src\")) dependencies: (map @remote (record kind: @git git: \"https://example.com/repo.git\" rev: \"ABC\")))",
    );
    assert!(!invalid_revision.accepted());
    assert_eq!(
        invalid_revision.diagnostics()[0].code(),
        DiagnosticCode::DataInvalidValue
    );
}

#[test]
fn diagnostics_keep_the_project_source_identity_and_precise_value_span() {
    let source = "(record format: @project.v2 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @bin root: \"src\" entry: @hello.main effects: (array))) dependencies: (map))";
    let decoded = decode(source);
    let diagnostic = &decoded.diagnostics()[0];
    assert_eq!(diagnostic.source_id(), Some("project.vibon"));
    assert_eq!(diagnostic.primary_span(), ByteSpan::new(16, 27));
}

#[test]
fn canonical_project_format_uses_schema_order_and_round_trips() {
    let source = "(record dependencies: (map) targets: (array (record effects: (array) root: \"src\" kind: @bin entry: @hello.main name: @hello)) package: (record version: \"0.1.0\" name: \"hello\") format: @project.v1)";
    let decoded = decode(source);
    let project = decoded.project().expect("typed project");
    let canonical = project.canonical_vibon();
    assert!(canonical.starts_with("(record format: @project.v1 package:"));
    assert!(canonical.contains("targets: (array (record name: @hello kind: @bin root: \"src\" entry: @hello.main effects: (array)))"));

    let reparsed =
        parse_data(Path::new("project.vibon"), &canonical).expect("data loader");
    assert!(reparsed.accepted(), "{:?}", reparsed.diagnostics());
    let round_trip = ProjectDecoder::decode(
        reparsed.data().expect("reparsed root"),
        ProjectOrigin::new("project.vibon"),
    );
    assert!(round_trip.accepted(), "{:?}", round_trip.diagnostics());
    assert_eq!(
        round_trip
            .project()
            .expect("round-trip project")
            .canonical_vibon(),
        canonical
    );
}

#[test]
fn canonical_project_format_preserves_comments_while_ordering_records_and_maps() {
    let source = r#"(record
  ; project fields stay attached to their records
  dependencies: (map
    @z (record target: @core rev: "0123456789abcdef0123456789abcdef01234567" git: "https://example.com/z.git" kind: @git) ; z dependency
    @a (record kind: @path path: "../a") ; a dependency
  )
  targets: (array
    (record effects: (array) entry: @hello.main root: "src" kind: @bin name: @hello))
  package: (record version: "0.1.0" ; package comment
    name: "hello")
  format: @project.v1)"#;
    let decoded = decode(source);
    let project = decoded.project().expect("typed project");
    let canonical = project.canonical_vibon();
    assert!(canonical.contains("; project fields stay attached to their records"));
    assert!(canonical.contains("; package comment"));
    assert!(canonical.contains("; z dependency"));
    assert!(canonical.contains("; a dependency"));
    assert!(
        canonical.find("format:").expect("format")
            < canonical.find("package:").expect("package")
    );
    assert!(
        canonical.find("@a").expect("a dependency")
            < canonical.find("@z").expect("z dependency")
    );
    assert!(
        canonical.find("@z").expect("z dependency")
            < canonical.find("; z dependency").expect("z comment")
    );
    assert!(
        canonical.find("\"0.1.0\"").expect("version")
            < canonical
                .find("; package comment")
                .expect("package comment")
    );
    let reparsed =
        parse_data(Path::new("project.vibon"), &canonical).expect("data loader");
    assert!(reparsed.accepted(), "{:?}", reparsed.diagnostics());
    let round_trip = ProjectDecoder::decode(
        reparsed.data().expect("reparsed root"),
        ProjectOrigin::new("project.vibon"),
    );
    assert!(round_trip.accepted(), "{:?}", round_trip.diagnostics());
    assert_eq!(
        round_trip
            .project()
            .expect("round-trip project")
            .canonical_vibon(),
        canonical
    );
}

#[test]
fn canonical_project_format_keeps_moved_final_comment_with_its_field() {
    let source = r#"(record
  package: (record name: "hello" version: "0.1.0")
  targets: (array (record name: @hello kind: @lib root: "src"))
  dependencies: (map)
  format: @project.v1 ; format comment
)"#;
    let decoded = decode(source);
    let project = decoded.project().expect("typed project");
    let canonical = project.canonical_vibon();
    assert!(canonical.contains("; format comment"));
    let reparsed = parse_data(Path::new("project.vibon"), &canonical)
        .expect("canonical data loader");
    assert!(reparsed.accepted(), "{:?}", reparsed.diagnostics());
    assert_eq!(
        ProjectDecoder::decode(
            reparsed.data().expect("canonical root"),
            ProjectOrigin::new("project.vibon"),
        )
        .project()
        .expect("canonical project")
        .canonical_vibon(),
        canonical
    );
}

#[test]
fn canonical_project_format_keeps_mixed_comment_boundaries_parseable() {
    let source = r#"(record
  package: (record name: "hello" version: "0.1.0")
  targets: (array (record name: @hello kind: @lib root: "src"))
  format: @project.v1 ; format comment
  ; dependencies comment
  dependencies: (map))"#;
    let decoded = decode(source);
    let project = decoded.project().expect("typed project");
    let canonical = project.canonical_vibon();
    assert!(canonical.contains("; format comment"));
    assert!(canonical.contains("; dependencies comment"));
    let reparsed = parse_data(Path::new("project.vibon"), &canonical)
        .expect("canonical data loader");
    assert!(reparsed.accepted(), "{:?}", reparsed.diagnostics());
}

#[test]
fn target_and_dependency_aliases_share_a_checked_namespace() {
    let decoded = decode(
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @std kind: @lib root: \"src\")) dependencies: (map @std (record kind: @path path: \"../std\")))",
    );
    assert!(!decoded.accepted());
    let diagnostic = &decoded.diagnostics()[0];
    assert_eq!(diagnostic.code(), DiagnosticCode::NameMemberCollision);
    assert_eq!(diagnostic.related().len(), 1);
    assert_eq!(diagnostic.related()[0].span, ByteSpan::new(107, 111));
}

#[test]
fn generic_data_nodes_stay_grammar_owned() {
    let document =
        parse_data(Path::new("project.vibon"), "(array @value)").expect("data loader");
    let root = document.data().expect("generic root");
    assert!(matches!(root.value(), DataValue::Array(_)));
}

#[test]
fn diagnostics_preserve_unicode_project_origins() {
    let document =
        parse_data(Path::new("project.vibon"), "(record format: @project.v1)")
            .expect("data loader");
    let decoded = ProjectDecoder::decode(
        document.data().expect("generic root"),
        ProjectOrigin::new("資料/project.vibon"),
    );
    assert!(!decoded.accepted());
    assert_eq!(
        decoded.diagnostics()[0].source_id(),
        Some("資料/project.vibon")
    );
}
