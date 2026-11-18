//! The runner rows of the step 02 test matrix, through the binary, and the
//! comparison through test handlers.

use std::path::Path;
use std::process::{Command, Output};

use crate::support::{reader_case, rules, scratch_directory, write};

use vibra_conformance::{
    Case, CaseObservation, CaseStatus, ConformanceProfile, ConformanceRunner, Corpus,
    HandlerError, ProfileDispatcher, ProfileHandler, Verdict, standard_dispatcher,
};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

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

#[test]
fn a_case_with_no_handler_is_unavailable_and_fails_the_run() {
    let root = one_reader_case("");
    let output = conformance(&["--root".as_ref(), root.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = text(&output.stdout);
    assert!(
        stdout.lines().any(|line| line
            == "readerV2 conformance: 0 passed, 0 failed, 0 unknown, 1 unavailable"),
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
fn the_standard_dispatcher_registers_no_handler() {
    let root = one_reader_case("");
    let corpus = Corpus::discover(&root).expect("the corpus loads");
    let report = ConformanceRunner::new(standard_dispatcher()).run(&corpus);
    assert!(matches!(
        report.cases()[0].status,
        CaseStatus::Unavailable { .. }
    ));
    assert!(!report.is_success());
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
