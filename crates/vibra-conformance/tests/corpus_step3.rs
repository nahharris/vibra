//! Conformance corpus, manifest, and profile-runner tests.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_conformance::{
    CaseManifest, CaseObservation, CaseStatus, ConformanceOperation,
    ConformanceProfile, ConformanceRunner, Corpus, HandlerError, ProfileDispatcher,
    ProfileHandler, ReaderV1Handler,
};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_syntax::DocumentMode;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn workspace_root() -> PathBuf {
    Path::new(ROOT)
        .ancestors()
        .nth(2)
        .expect("the crate lives two levels below the workspace root")
        .to_path_buf()
}

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempCase {
    root: PathBuf,
}

impl TempCase {
    fn new(id: &str, manifest: &str) -> Self {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "vibra-conformance-step3-{}-{serial}",
            std::process::id()
        ));
        let directory = root.join(id);
        std::fs::create_dir_all(&directory).expect("create temporary case");
        std::fs::write(directory.join("case.toml"), manifest)
            .expect("write temporary manifest");
        Self { root }
    }

    fn corpus(&self) -> Corpus {
        Corpus::discover(&self.root).expect("temporary case is valid")
    }
}

impl Drop for TempCase {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn valid_manifest(id: &str, profile: &str) -> String {
    let rule = id.rsplit_once('-').map_or(id, |(prefix, _)| prefix);
    format!(
        "id = \"{id}\"\nrule = \"{rule}\"\nprofile = \"{profile}\"\n\n[expect]\naccepted = true\n"
    )
}

struct FixedHandler {
    observation: CaseObservation,
}

impl ProfileHandler for FixedHandler {
    fn run(
        &self,
        _case: &vibra_conformance::Case,
    ) -> Result<CaseObservation, HandlerError> {
        Ok(self.observation.clone())
    }
}

#[test]
fn the_step_three_corpus_has_a_case_manifest() {
    let path =
        workspace_root().join("conformance/cases/V1-DIAG-profile-dispatch/case.toml");

    assert!(
        path.is_file(),
        "step 3 fixture is missing: {}",
        path.display()
    );
}

#[test]
fn manifest_decodes_inputs_profile_and_explicit_diagnostic_span() {
    let manifest = CaseManifest::from_str(
        r#"
id = "V1-SRC-READER-invalid-character"
rule = "V1-SRC-READER"
profile = "reader-v1"

[inputs]
source = "input.vib"
project = "project.vibon"
data = ["one.vibon", "two.vibon"]

[expect]
accepted = false

[[expect.diagnostics]]
code = "@syntax.invalid-character-literal"
level = "@error"

[expect.diagnostics.span]
start = 4
end = 7
"#,
    )
    .expect("valid case manifest");

    assert_eq!(manifest.id(), "V1-SRC-READER-invalid-character");
    assert_eq!(manifest.rule_id(), "V1-SRC-READER");
    assert_eq!(manifest.section(), "V1-SRC-READER");
    assert_eq!(manifest.profile(), ConformanceProfile::ReaderV1);
    assert_eq!(manifest.operation(), ConformanceOperation::Reader);
    assert_eq!(manifest.inputs.data, ["one.vibon", "two.vibon"]);
    assert!(!manifest.expectations.accepted);
    assert_eq!(manifest.expectations.diagnostics.len(), 1);
    assert_eq!(
        manifest.expectations.diagnostics[0].primary_span,
        ByteSpan::new(4, 7)
    );
}

#[test]
fn manifest_selects_project_decode_and_rejects_implicit_mixed_operations() {
    let project = CaseManifest::from_str(
        r#"
id = "V1-PROJECT-operation-project-decode"
rule = "V1-PROJECT"
profile = "static-v1"
operation = "project-decode"

[inputs]
project = "project.vibon"

[expect]
accepted = true
"#,
    )
    .expect("project operation manifest");
    assert_eq!(project.operation(), ConformanceOperation::ProjectDecode);

    let mixed = CaseManifest::from_str(
        r#"
id = "V1-PROJECT-operation-mixed"
rule = "V1-PROJECT"
profile = "static-v1"

[inputs]
source = "main.vib"
project = "project.vibon"

[expect]
accepted = true
"#,
    )
    .expect_err("mixed non-reader inputs need an explicit operation");
    assert!(mixed.to_string().contains("operation is required"));
}

#[test]
fn source_graph_binds_the_project_marker_to_the_declared_tree() {
    let outside = CaseManifest::from_str(
        r#"
id = "V1-PROJECT-source-graph-outside"
rule = "V1-PROJECT"
profile = "static-v1"
operation = "source-graph"

[inputs]
project = "input.vibon"
tree = "tree"

[expect]
accepted = true
"#,
    )
    .expect_err("source graph must reject an outside project input");
    assert!(
        outside
            .to_string()
            .contains("must be exactly `tree/project.vibon`")
    );

    let sibling = CaseManifest::from_str(
        r#"
id = "V1-PROJECT-source-graph-sibling"
rule = "V1-PROJECT"
profile = "static-v1"
operation = "source-graph"

[inputs]
project = "tree/sibling.vibon"
tree = "tree"

[expect]
accepted = true
"#,
    )
    .expect_err("source graph must reject an undeclared sibling marker");
    assert!(
        sibling
            .to_string()
            .contains("must be exactly `tree/project.vibon`")
    );
}

#[test]
fn input_documents_keep_roles_and_source_identity_without_concatenation() {
    let case = TempCase::new(
        "V1-DIAG-input-documents",
        r#"
id = "V1-DIAG-input-documents"
rule = "V1-DIAG"
profile = "reader-v1"

[inputs]
source = "src/main.vib"
project = "project.vibon"
data = ["data/one.vibon", "data/two.vibon"]

[expect]
accepted = true
"#,
    );
    let directory = case.root.join("V1-DIAG-input-documents");
    std::fs::create_dir_all(directory.join("src")).expect("create source directory");
    std::fs::create_dir_all(directory.join("data")).expect("create data directory");
    std::fs::write(directory.join("src/main.vib"), "(defn main () void)")
        .expect("write source");
    std::fs::write(directory.join("project.vibon"), "(@project.v1)")
        .expect("write project");
    std::fs::write(directory.join("data/one.vibon"), "one").expect("write data one");
    std::fs::write(directory.join("data/two.vibon"), "two").expect("write data two");

    let documents = case.corpus().cases()[0]
        .input_documents()
        .expect("load declared inputs");
    let actual = documents
        .iter()
        .map(|document| {
            (
                document.source_id.as_str(),
                document.mode,
                document.text.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            ("src/main.vib", DocumentMode::Source, "(defn main () void)"),
            ("project.vibon", DocumentMode::Data, "(@project.v1)"),
            ("data/one.vibon", DocumentMode::Data, "one"),
            ("data/two.vibon", DocumentMode::Data, "two"),
        ]
    );
}

#[test]
fn manifest_rejects_unknown_codes_and_registry_level_mismatches() {
    let unknown = CaseManifest::from_str(
        r#"
id = "V1-DIAG-unknown-code"
rule = "V1-DIAG"
profile = "reader-v1"

[expect]
accepted = false

[[expect.diagnostics]]
code = "@syntax.no-such-code"
level = "@error"

[expect.diagnostics.span]
start = 0
end = 1
"#,
    )
    .expect_err("unknown codes are outside the closed registry");
    assert!(unknown.to_string().contains("closed v1 registry"));

    let mismatch = CaseManifest::from_str(
        r#"
id = "V1-DIAG-wrong-level"
rule = "V1-DIAG"
profile = "reader-v1"

[expect]
accepted = false

[[expect.diagnostics]]
code = "@syntax.invalid-character-literal"
level = "@warning"

[expect.diagnostics.span]
start = 0
end = 1
"#,
    )
    .expect_err("manifest levels must agree with the registry");
    assert!(mismatch.to_string().contains("registry fixes it at @error"));
}

#[test]
fn corpus_requires_declared_files_to_stay_inside_the_case() {
    let case = TempCase::new(
        "V1-DIAG-path-traversal",
        r#"
id = "V1-DIAG-path-traversal"
rule = "V1-DIAG"
profile = "reader-v1"

[expect]
accepted = true

[inputs]
source = "../outside.vib"
"#,
    );
    let error =
        Corpus::discover(&case.root).expect_err("parent paths must be rejected");
    assert!(error.to_string().contains("non-relative file path"));
}

#[test]
fn corpus_discovers_and_runs_a_synthetic_fixture() {
    let case = TempCase::new(
        "V1-DIAG-profile-dispatch",
        &valid_manifest("V1-DIAG-profile-dispatch", "reader-v1"),
    );
    let corpus = case.corpus();
    assert_eq!(corpus.len(), 1);
    assert_eq!(
        corpus.cases()[0].manifest().id(),
        "V1-DIAG-profile-dispatch"
    );

    let runner = ConformanceRunner::new(ProfileDispatcher::new().with_handler(
        ConformanceProfile::ReaderV1,
        FixedHandler {
            observation: CaseObservation::new(true),
        },
    ));
    let report = runner.run(&corpus);
    assert!(report.is_success());
    assert_eq!(report.passed(), 1);
}

#[test]
fn manifest_requires_an_explicit_rule() {
    let error = CaseManifest::from_str(
        r#"
id = "V1-DIAG-missing-rule"
profile = "reader-v1"

[expect]
accepted = true
"#,
    )
    .expect_err("a case must name the normative rule it covers");
    assert!(error.to_string().contains("missing field `rule`"));
}

#[test]
fn manifest_requires_the_rule_and_case_to_share_a_section() {
    let error = CaseManifest::from_str(
        r#"
id = "V1-DIAG-wrong-rule-section"
rule = "V1-SRC-READER"
profile = "reader-v1"

[expect]
accepted = true
"#,
    )
    .expect_err("a case cannot be filed under another section");
    assert!(error.to_string().contains("different normative sections"));
}

#[test]
fn rejected_cases_require_an_error_diagnostic() {
    let no_diagnostic = CaseManifest::from_str(
        r#"
id = "V1-DIAG-rejected-without-diagnostic"
rule = "V1-DIAG"
profile = "reader-v1"

[expect]
accepted = false
"#,
    )
    .expect_err("rejection must identify an error diagnostic");
    assert!(no_diagnostic.to_string().contains("at least one @error"));

    let warning_only = CaseManifest::from_str(
        r#"
id = "V1-DIAG-rejected-with-warning"
rule = "V1-DIAG"
profile = "reader-v1"

[expect]
accepted = false

[[expect.diagnostics]]
code = "@style.argument-order"
level = "@warning"

[expect.diagnostics.span]
start = 0
end = 1
"#,
    )
    .expect_err("a warning cannot be the only evidence for rejection");
    assert!(warning_only.to_string().contains("at least one @error"));
}

#[test]
fn manifest_rejects_noncanonical_aliases() {
    let aliases = [
        (
            "rule_id",
            r#"
id = "V1-DIAG-alias-rule"
rule_id = "V1-DIAG"
profile = "reader-v1"
[expect]
accepted = true
"#,
        ),
        (
            "expected table",
            r#"
id = "V1-DIAG-alias-expected"
rule = "V1-DIAG"
profile = "reader-v1"
[expected]
accepted = true
"#,
        ),
        (
            "expectations table",
            r#"
id = "V1-DIAG-alias-expectations"
rule = "V1-DIAG"
profile = "reader-v1"
[expectations]
accepted = true
"#,
        ),
        (
            "format field",
            r#"
id = "V1-DIAG-alias-format"
rule = "V1-DIAG"
profile = "reader-v1"
[expect]
accepted = true
format = "formatted.vib"
"#,
        ),
        (
            "status field",
            r#"
id = "V1-DIAG-alias-status"
rule = "V1-DIAG"
profile = "reader-v1"
[expect]
status = "accepted"
"#,
        ),
        (
            "primary span field",
            r#"
id = "V1-DIAG-alias-primary"
rule = "V1-DIAG"
profile = "reader-v1"
[expect]
accepted = false
[[expect.diagnostics]]
code = "@syntax.invalid-character-literal"
level = "@error"
[expect.diagnostics.primary]
start = 0
end = 1
"#,
        ),
        (
            "top-level span fields",
            r#"
id = "V1-DIAG-alias-span"
rule = "V1-DIAG"
profile = "reader-v1"
[expect]
accepted = false
[[expect.diagnostics]]
code = "@syntax.invalid-character-literal"
level = "@error"
start = 0
end = 1
"#,
        ),
        (
            "single data path",
            r#"
id = "V1-DIAG-alias-data"
rule = "V1-DIAG"
profile = "reader-v1"
[inputs]
data = "one.vibon"
[expect]
accepted = true
"#,
        ),
        (
            "audit field",
            r#"
id = "V1-DIAG-alias-audit"
rule = "V1-DIAG"
profile = "reader-v1"
[expect]
accepted = true
[expect.interpreter]
audit = "audit.txt"
"#,
        ),
        (
            "artifacts table",
            r#"
id = "V1-DIAG-alias-artifacts"
rule = "V1-DIAG"
profile = "reader-v1"
[expect]
accepted = true
[expect.artifacts]
hashes = ["sha256:test"]
"#,
        ),
    ];

    for (name, text) in aliases {
        assert!(
            CaseManifest::from_str(text).is_err(),
            "noncanonical {name} spelling was accepted"
        );
    }
}

#[test]
fn artifact_expectations_are_compared_only_when_declared() {
    let case = TempCase::new(
        "V1-DIAG-optional-artifact",
        &valid_manifest("V1-DIAG-optional-artifact", "reader-v1"),
    );
    let report = ConformanceRunner::new(ProfileDispatcher::new().with_handler(
        ConformanceProfile::ReaderV1,
        FixedHandler {
            observation: CaseObservation {
                accepted: true,
                artifact_hashes: vec!["sha256:produced".to_owned()],
                ..CaseObservation::default()
            },
        },
    ))
    .run(&case.corpus());

    assert!(report.is_success());
}

#[test]
fn runner_rejects_a_wrong_source_graph_snapshot() {
    let case = TempCase::new(
        "V1-PROJECT-graph-oracle",
        r#"
id = "V1-PROJECT-graph-oracle"
rule = "V1-PROJECT"
profile = "static-v1"
operation = "source-graph"

[inputs]
project = "tree/project.vibon"
tree = "tree"

[expect]
accepted = true
graph = "graph.vibon"
"#,
    );
    let directory = case.root.join("V1-PROJECT-graph-oracle");
    std::fs::create_dir(directory.join("tree")).expect("create declared tree");
    std::fs::write(directory.join("tree/project.vibon"), "project")
        .expect("write declared project");
    std::fs::write(
        directory.join("graph.vibon"),
        "(record format: @source-graph.v1)\n",
    )
    .expect("write graph oracle");

    let report = ConformanceRunner::new(ProfileDispatcher::new().with_handler(
        ConformanceProfile::StaticV1,
        FixedHandler {
            observation: CaseObservation {
                accepted: true,
                graph: Some("wrong graph\n".to_owned()),
                ..CaseObservation::default()
            },
        },
    ))
    .run(&case.corpus());

    assert!(!report.is_success());
    assert!(format!("{:?}", report.cases()).contains("graph snapshot mismatch"));
}

#[test]
fn fix_expectations_are_compared_only_when_declared() {
    let case = TempCase::new(
        "V1-DIAG-optional-fix",
        r#"
id = "V1-DIAG-optional-fix"
rule = "V1-DIAG"
profile = "reader-v1"

[expect]
accepted = true

[[expect.diagnostics]]
code = "@style.argument-order"
level = "@warning"

[expect.diagnostics.span]
start = 0
end = 1
"#,
    );
    let diagnostic = Diagnostic::new(
        DiagnosticCode::StyleArgumentOrder,
        ByteSpan::new(0, 1),
        "normalizable order",
    )
    .with_fix(vibra_diagnostics::Fix::safe(
        "reorder",
        vibra_diagnostics::DocumentRevision::new("sha256:test"),
    ));
    let report = ConformanceRunner::new(ProfileDispatcher::new().with_handler(
        ConformanceProfile::ReaderV1,
        FixedHandler {
            observation: CaseObservation {
                accepted: true,
                diagnostics: vec![diagnostic],
                ..CaseObservation::default()
            },
        },
    ))
    .run(&case.corpus());

    assert!(report.is_success());
}

#[test]
fn discovered_manifest_symlink_cannot_escape_the_corpus_root() {
    let case = TempCase::new(
        "V1-DIAG-manifest-symlink",
        &valid_manifest("V1-DIAG-manifest-symlink", "reader-v1"),
    );
    let manifest = case.root.join("V1-DIAG-manifest-symlink").join("case.toml");
    let outside = case
        .root
        .parent()
        .expect("temporary corpus has a parent")
        .join("vibra-conformance-step3-outside-case.toml");
    std::fs::write(
        &outside,
        valid_manifest("V1-DIAG-manifest-symlink", "reader-v1"),
    )
    .expect("write outside manifest");
    std::fs::remove_file(&manifest).expect("remove regular manifest");

    if let Err(error) = create_file_symlink(&outside, &manifest) {
        eprintln!("skipping symlink regression: {error}");
        let _ = std::fs::remove_file(outside);
        return;
    }

    let error =
        Corpus::discover(&case.root).expect_err("manifest escape must be rejected");
    assert!(error.to_string().contains("manifest resolves outside"));
    let _ = std::fs::remove_file(outside);
}

fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link)
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (target, link);
        Err(std::io::Error::other(
            "file symlinks unsupported on this platform",
        ))
    }
}

#[test]
fn runner_dispatches_the_closest_capable_profile() {
    let case = TempCase::new(
        "V1-SRC-READER-dispatch",
        &valid_manifest("V1-SRC-READER-dispatch", "reader-v1"),
    );
    let corpus = case.corpus();
    let dispatcher = ProfileDispatcher::new()
        .with_handler(
            ConformanceProfile::ReaderV1,
            FixedHandler {
                observation: CaseObservation::new(true),
            },
        )
        .with_handler(
            ConformanceProfile::StaticV1,
            FixedHandler {
                observation: CaseObservation::new(false),
            },
        );

    let report = ConformanceRunner::new(dispatcher).run(&corpus);
    assert!(report.is_success());
    assert_eq!(report.passed(), 1);
    assert_eq!(
        report.cases()[0].provided_profile,
        Some(ConformanceProfile::ReaderV1)
    );
}

#[test]
fn runner_reports_a_profile_without_a_handler_as_unavailable() {
    let case = TempCase::new(
        "V1-SRC-READER-no-handler",
        &valid_manifest("V1-SRC-READER-no-handler", "reader-v1"),
    );
    let report = ConformanceRunner::new(ProfileDispatcher::new()).run(&case.corpus());

    assert_eq!(report.unavailable(), 1);
    assert!(!report.is_success());
    assert!(matches!(
        report.cases()[0].status,
        CaseStatus::Unavailable { .. }
    ));
}

#[test]
fn runner_compares_diagnostic_codes_levels_and_spans() {
    let case = TempCase::new(
        "V1-DIAG-diagnostic-observation",
        r#"
id = "V1-DIAG-diagnostic-observation"
rule = "V1-DIAG"
profile = "reader-v1"

[expect]
accepted = false

[[expect.diagnostics]]
code = "@syntax.invalid-character-literal"
level = "@error"

[expect.diagnostics.span]
start = 2
end = 5
"#,
    );
    let diagnostic = Diagnostic::new(
        DiagnosticCode::SyntaxInvalidCharacterLiteral,
        ByteSpan::new(2, 5),
        "invalid character",
    );
    let report = ConformanceRunner::new(ProfileDispatcher::new().with_handler(
        ConformanceProfile::ReaderV1,
        FixedHandler {
            observation: CaseObservation {
                accepted: false,
                diagnostics: vec![diagnostic],
                ..CaseObservation::default()
            },
        },
    ))
    .run(&case.corpus());

    assert!(report.is_success());
    assert_eq!(report.passed(), 1);
}

#[test]
fn runner_keeps_equal_offsets_distinct_by_source_identity() {
    let case = TempCase::new(
        "V1-DIAG-source-identity",
        r#"
id = "V1-DIAG-source-identity"
rule = "V1-DIAG"
profile = "reader-v1"

[inputs]
source = "one.vib"
data = ["two.vibon"]

[expect]
accepted = false

[[expect.diagnostics]]
code = "@syntax.invalid-character-literal"
level = "@error"
source = "one.vib"
[expect.diagnostics.span]
start = 0
end = 1

[[expect.diagnostics]]
code = "@syntax.invalid-character-literal"
level = "@error"
source = "two.vibon"
[expect.diagnostics.span]
start = 0
end = 1
"#,
    );
    let directory = case.root.join("V1-DIAG-source-identity");
    std::fs::write(directory.join("one.vib"), "x").expect("write source");
    std::fs::write(directory.join("two.vibon"), "x").expect("write data");

    let diagnostic = |source_id: &str| {
        Diagnostic::new(
            DiagnosticCode::SyntaxInvalidCharacterLiteral,
            ByteSpan::new(0, 1),
            "invalid character",
        )
        .with_source_id(source_id)
    };
    let handler = FixedHandler {
        observation: CaseObservation {
            accepted: false,
            diagnostics: vec![diagnostic("one.vib"), diagnostic("two.vibon")],
            ..CaseObservation::default()
        },
    };
    let report = ConformanceRunner::new(
        ProfileDispatcher::new().with_handler(ConformanceProfile::ReaderV1, handler),
    )
    .run(&case.corpus());

    assert!(report.is_success());
    assert_eq!(report.passed(), 1);

    let swapped = ConformanceRunner::new(ProfileDispatcher::new().with_handler(
        ConformanceProfile::ReaderV1,
        FixedHandler {
            observation: CaseObservation {
                accepted: false,
                diagnostics: vec![diagnostic("two.vibon"), diagnostic("one.vib")],
                ..CaseObservation::default()
            },
        },
    ))
    .run(&case.corpus());
    assert!(!swapped.is_success());
    assert!(format!("{:?}", swapped.cases()).contains("source mismatch"));

    let missing = ConformanceRunner::new(ProfileDispatcher::new().with_handler(
        ConformanceProfile::ReaderV1,
        FixedHandler {
            observation: CaseObservation {
                accepted: false,
                diagnostics: vec![
                    Diagnostic::new(
                        DiagnosticCode::SyntaxInvalidCharacterLiteral,
                        ByteSpan::new(0, 1),
                        "invalid character",
                    ),
                    diagnostic("two.vibon"),
                ],
                ..CaseObservation::default()
            },
        },
    ))
    .run(&case.corpus());
    assert!(!missing.is_success());
    assert!(format!("{:?}", missing.cases()).contains("source mismatch"));
}

#[test]
fn reader_attaches_source_ids_to_multi_input_diagnostics() {
    let case = TempCase::new(
        "V1-DIAG-reader-source-identity",
        r#"
id = "V1-DIAG-reader-source-identity"
rule = "V1-DIAG"
profile = "reader-v1"

[inputs]
source = "source.vib"
data = ["data.vibon"]

[expect]
accepted = false

[[expect.diagnostics]]
code = "@syntax.invalid-name"
level = "@error"
source = "source.vib"
[expect.diagnostics.span]
start = 0
end = 2

[[expect.diagnostics]]
code = "@data.invalid-shape"
level = "@error"
source = "data.vibon"
[expect.diagnostics.span]
start = 5
end = 7
"#,
    );
    let directory = case.root.join("V1-DIAG-reader-source-identity");
    std::fs::write(directory.join("source.vib"), "-a").expect("write source");
    std::fs::write(directory.join("data.vibon"), "(map @a)").expect("write data");

    let report = ConformanceRunner::new(
        ProfileDispatcher::new()
            .with_handler(ConformanceProfile::ReaderV1, ReaderV1Handler),
    )
    .run(&case.corpus());
    assert!(report.is_success(), "reader report: {report:?}");
}
