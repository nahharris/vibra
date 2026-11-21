//! The runner rows of the step 02 test matrix, through the binary, and the
//! comparison through test handlers.

use std::path::Path;
use std::process::{Command, Output};

use crate::support::{reader_case, rules, scratch_directory, write};

use vibra_conformance::{
    Case, CaseObservation, CaseStatus, ConformanceProfile, ConformanceRunner, Corpus,
    HandlerError, ProfileDispatcher, ProfileHandler, Verdict, standard_dispatcher,
};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, DocumentRevision, Fix};

fn conformance(arguments: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vibra-conformance"))
        .args(arguments)
        .output()
        .expect("the runner starts")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A corpus of one valid reader case, with the rule index beside it.
fn one_reader_case(manifest_extra: &str) -> std::path::PathBuf {
    let id = "V2-SRC-READER-x";
    let base = scratch_directory("runner");
    write(&base, "rules.tsv", &rules(&[(id, "active")]));
    write(
        &base,
        &format!("cases/SRC-READER/{id}/case.toml"),
        &format!("{}{manifest_extra}", reader_case(id, id)),
    );
    write(&base, &format!("cases/SRC-READER/{id}/input.vibon"), "{}");
    base.join("cases")
}

#[test]
fn the_binary_takes_only_root_and_rejects_a_write_option() {
    let output = conformance(&["--write-native-parity".as_ref(), "out.tsv".as_ref()]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("unknown option `--write-native-parity`"),
        "{stderr}"
    );
    let output = conformance(&["positional".as_ref()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("--root"));
}

/// The one case of `root` moved to `profile`, which no standard handler
/// covers.
fn under_profile(root: &Path, profile: &str) {
    let manifest = root.join("SRC-READER/V2-SRC-READER-x/case.toml");
    let text = std::fs::read_to_string(&manifest).expect("the manifest");
    std::fs::write(&manifest, text.replace("readerV2", profile)).expect("rewrite");
}

#[test]
fn a_case_with_no_handler_is_unavailable_and_fails_the_run() {
    let root = one_reader_case("");
    under_profile(&root, "staticV2");
    let output = conformance(&["--root".as_ref(), root.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = text(&output.stdout);
    assert!(
        stdout.lines().any(|line| line
            == "staticV2 conformance: 0 passed, 0 failed, 0 unknown, 1 unavailable"),
        "{stdout}"
    );
    assert!(stdout.contains("UNAVAILABLE V2-SRC-READER-x"), "{stdout}");
    assert!(!stdout.to_lowercase().contains("skipped"), "{stdout}");
}

#[test]
fn an_empty_corpus_fails_the_run() {
    let root = scratch_directory("empty-run");
    let output = conformance(&["--root".as_ref(), root.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = text(&output.stderr);
    assert!(stderr.contains("corpus is empty"), "{stderr}");
}

#[test]
fn the_standard_dispatcher_runs_reader_cases_only() {
    let root =
        one_reader_case("tree = \"tree.vibon\"\nformatted = \"formatted.vibon\"\n");
    write(
        &root,
        "SRC-READER/V2-SRC-READER-x/tree.vibon",
        "{\n  root: {kind: @data children: [{kind: @tuple children: [\
         {kind: @punctuation text: \"{\"} {kind: @punctuation text: \"}\"}]}]}\n  \
         format: @tree.v2\n}\n",
    );
    write(&root, "SRC-READER/V2-SRC-READER-x/formatted.vibon", "{}\n");
    let corpus = Corpus::discover(&root).expect("the corpus loads");
    let report = ConformanceRunner::new(standard_dispatcher()).run(&corpus);
    assert_eq!(report.cases()[0].status, CaseStatus::Passed);
    assert_eq!(
        report.cases()[0].provided_profile,
        Some(ConformanceProfile::ReaderV2)
    );
    under_profile(&root, "staticV2");
    let corpus = Corpus::discover(&root).expect("the corpus loads");
    let report = ConformanceRunner::new(standard_dispatcher()).run(&corpus);
    assert!(matches!(
        report.cases()[0].status,
        CaseStatus::Unavailable { .. }
    ));
    assert!(!report.is_success());
}

#[test]
fn a_tree_snapshot_is_compared_as_a_decoded_value() {
    let root = one_reader_case("tree = \"tree.vibon\"\n");
    write(
        &root,
        "SRC-READER/V2-SRC-READER-x/tree.vibon",
        "// written by hand\n{root: [\"a\": 1u8 \"b\": 2u8] format: @tree.v2}\n",
    );
    let mut observation = CaseObservation::new(Verdict::Accepted);
    observation.tree =
        Some("{format: @tree.v2 root: [\"b\": 2u8 \"a\": 1u8]}".to_owned());
    assert_eq!(
        status_under(&root, ConformanceProfile::ReaderV2, observation.clone()),
        CaseStatus::Passed,
        "fields and entries in another order and another layout are one value"
    );
    observation.tree =
        Some("{format: @tree.v2 root: [\"b\": 2u16 \"a\": 1u8]}".to_owned());
    let status = status_under(&root, ConformanceProfile::ReaderV2, observation.clone());
    assert!(
        matches!(&status, CaseStatus::Failed { reason } if reason.contains("tree snapshot mismatch")),
        "{status:?}"
    );
    observation.tree = Some("{format: @tree.v2".to_owned());
    let status = status_under(&root, ConformanceProfile::ReaderV2, observation);
    assert!(
        matches!(&status, CaseStatus::Failed { reason } if reason.contains("not a valid VIBON document")),
        "{status:?}"
    );
}

/// A handler that returns a fixed observation for every reader case.
struct Fixed(CaseObservation);

impl ProfileHandler for Fixed {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == vibra_conformance::ConformanceOperation::Reader
    }

    fn run(&self, _case: &Case) -> Result<CaseObservation, HandlerError> {
        Ok(self.0.clone())
    }
}

/// The status of the one case of `root` under a handler that observes
/// `observation`.
fn status_under(
    root: &Path,
    profile: ConformanceProfile,
    observation: CaseObservation,
) -> CaseStatus {
    let corpus = Corpus::discover(root).expect("the corpus loads");
    let dispatcher = ProfileDispatcher::new().with_handler(profile, Fixed(observation));
    let report = ConformanceRunner::new(dispatcher).run(&corpus);
    report.cases()[0].status.clone()
}

#[test]
fn a_matching_observation_passes_and_counts_as_passed() {
    let root = one_reader_case("");
    let corpus = Corpus::discover(&root).expect("the corpus loads");
    let dispatcher = ProfileDispatcher::new().with_handler(
        ConformanceProfile::StaticV2,
        Fixed(CaseObservation::new(Verdict::Accepted)),
    );
    let report = ConformanceRunner::new(dispatcher).run(&corpus);
    assert_eq!(report.cases()[0].status, CaseStatus::Passed);
    assert_eq!(
        report.cases()[0].provided_profile,
        Some(ConformanceProfile::StaticV2),
        "a broader profile runs a narrower case"
    );
    assert!(report.is_success());
    let counts = report.profile_counts();
    assert_eq!(counts.len(), 1);
    assert_eq!(counts[0].0, ConformanceProfile::ReaderV2);
    assert_eq!(
        counts[0].1.to_string(),
        "1 passed, 0 failed, 0 unknown, 0 unavailable"
    );
}

#[test]
fn a_narrower_profile_does_not_run_a_broader_case() {
    let root = one_reader_case("").parent().expect("a base").to_path_buf();
    let manifest = root.join("cases/SRC-READER/V2-SRC-READER-x/case.toml");
    let text = std::fs::read_to_string(&manifest).expect("the manifest");
    std::fs::write(&manifest, text.replace("readerV2", "staticV2")).expect("rewrite");
    let status = status_under(
        &root.join("cases"),
        ConformanceProfile::ReaderV2,
        CaseObservation::new(Verdict::Accepted),
    );
    assert!(
        matches!(status, CaseStatus::Unavailable { .. }),
        "{status:?}"
    );
}

#[test]
fn a_mismatched_verdict_fails() {
    let root = one_reader_case("");
    let status = status_under(
        &root,
        ConformanceProfile::ReaderV2,
        CaseObservation::new(Verdict::Refuted),
    );
    let CaseStatus::Failed { reason } = status else {
        panic!("{status:?}");
    };
    assert!(reason.contains("verdict mismatch"), "{reason}");
}

#[test]
fn an_unknown_observation_is_counted_unknown_not_passed() {
    let root = one_reader_case("");
    let status = status_under(
        &root,
        ConformanceProfile::ReaderV2,
        CaseObservation::new(Verdict::Unknown),
    );
    assert!(matches!(status, CaseStatus::Unknown { .. }), "{status:?}");
}

#[test]
fn a_diagnostic_span_mismatch_fails_and_reports_both_spans() {
    let root = one_reader_case(
        "\n[[expect.diagnostics]]\ncode = \"@syntax.invalidForm\"\nlevel = \"@error\"\n\
         span = { start = 3, end = 5 }\n",
    );
    let mut observation = CaseObservation::new(Verdict::Accepted);
    observation.diagnostics.push(Diagnostic::new(
        DiagnosticCode::SyntaxInvalidForm,
        ByteSpan::new(3, 4),
        "not admitted",
    ));
    let status = status_under(&root, ConformanceProfile::ReaderV2, observation);
    let CaseStatus::Failed { reason } = status else {
        panic!("{status:?}");
    };
    assert!(
        reason.contains("3..5") && reason.contains("3..4"),
        "{reason}"
    );
}

#[test]
fn a_stated_empty_fix_list_fails_when_a_fix_is_offered() {
    let root = one_reader_case(
        "\n[[expect.diagnostics]]\ncode = \"@syntax.invalidForm\"\nlevel = \"@error\"\n\
         span = { start = 3, end = 5 }\nfixes = []\n",
    );
    let diagnostic = Diagnostic::new(
        DiagnosticCode::SyntaxInvalidForm,
        ByteSpan::new(3, 5),
        "not admitted",
    );
    let mut observation = CaseObservation::new(Verdict::Accepted);
    observation.diagnostics.push(diagnostic.clone());
    assert_eq!(
        status_under(&root, ConformanceProfile::ReaderV2, observation),
        CaseStatus::Passed
    );
    let mut observation = CaseObservation::new(Verdict::Accepted);
    observation.diagnostics.push(
        diagnostic.with_fix(Fix::safe("rewrite the form", DocumentRevision::new("r1"))),
    );
    let status = status_under(&root, ConformanceProfile::ReaderV2, observation);
    let CaseStatus::Failed { reason } = status else {
        panic!("{status:?}");
    };
    assert!(reason.contains("fix count mismatch"), "{reason}");
}

#[test]
fn a_missing_or_different_snapshot_fails() {
    let root = one_reader_case("formatted = \"formatted.vibon\"\n");
    write(&root, "SRC-READER/V2-SRC-READER-x/formatted.vibon", "{}\n");
    let missing = status_under(
        &root,
        ConformanceProfile::ReaderV2,
        CaseObservation::new(Verdict::Accepted),
    );
    assert!(
        matches!(&missing, CaseStatus::Failed { reason } if reason.contains("no formatted observation")),
        "{missing:?}"
    );
    let mut observation = CaseObservation::new(Verdict::Accepted);
    observation.formatted = Some("{ }\n".to_owned());
    let different =
        status_under(&root, ConformanceProfile::ReaderV2, observation.clone());
    assert!(
        matches!(&different, CaseStatus::Failed { reason } if reason.contains("formatted snapshot mismatch")),
        "{different:?}"
    );
    observation.formatted = Some("{}\n".to_owned());
    assert_eq!(
        status_under(&root, ConformanceProfile::ReaderV2, observation),
        CaseStatus::Passed
    );
}

#[test]
fn two_closest_handlers_are_a_failure_not_a_choice() {
    let root = one_reader_case("");
    let corpus = Corpus::discover(&root).expect("the corpus loads");
    let dispatcher = ProfileDispatcher::new()
        .with_handler(
            ConformanceProfile::ReaderV2,
            Fixed(CaseObservation::new(Verdict::Accepted)),
        )
        .with_handler(
            ConformanceProfile::ReaderV2,
            Fixed(CaseObservation::new(Verdict::Accepted)),
        );
    let report = ConformanceRunner::new(dispatcher).run(&corpus);
    assert!(
        matches!(&report.cases()[0].status, CaseStatus::Failed { reason } if reason.contains("several handlers")),
        "{:?}",
        report.cases()[0].status
    );
}
