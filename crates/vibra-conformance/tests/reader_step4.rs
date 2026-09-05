//! The dedicated reader-v1 corpus path exercises the real syntax and formatter.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_conformance::{
    ConformanceProfile, ConformanceRunner, Corpus, ProfileDispatcher, ReaderV1Handler,
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempCase {
    root: PathBuf,
}

impl TempCase {
    fn new(id: &str, manifest: &str, input_name: &str, input: &str) -> Self {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "vibra-conformance-reader-step4-{serial}-{}",
            std::process::id()
        ));
        let directory = root.join(id);
        std::fs::create_dir_all(&directory).expect("create temporary case");
        std::fs::write(directory.join("case.toml"), manifest)
            .expect("write temporary manifest");
        std::fs::write(directory.join(input_name), input)
            .expect("write temporary input");
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

#[test]
fn synthetic_reader_case_runs_through_real_reader_and_formatter() {
    let case = TempCase::new(
        "V1-SRC-FMT-synthetic-reader",
        r#"
id = "V1-SRC-FMT-synthetic-reader"
rule = "V1-SRC-FMT"
profile = "reader-v1"

[inputs]
source = "input.vib"

[expect]
accepted = true
formatted = "formatted.vib"
"#,
        "input.vib",
        "(alpha   beta)\n",
    );
    std::fs::write(
        case.root
            .join("V1-SRC-FMT-synthetic-reader")
            .join("formatted.vib"),
        "(alpha beta)\n",
    )
    .expect("write temporary formatting snapshot");
    let corpus = case.corpus();
    let dispatcher = ProfileDispatcher::new()
        .with_handler(ConformanceProfile::ReaderV1, ReaderV1Handler);
    let report = ConformanceRunner::new(dispatcher).run(&corpus);

    assert!(report.is_success(), "synthetic reader report: {report:?}");
    assert_eq!(report.failed(), 0);
    assert_eq!(report.unavailable(), 0);
    assert_eq!(report.passed(), corpus.len());
}

#[test]
fn internal_entrypoint_returns_nonzero_when_a_case_fails() {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "vibra-conformance-step4-failing-{serial}-{}",
        std::process::id()
    ));
    let case = root.join("V1-SRC-READER-entrypoint-failure");
    std::fs::create_dir_all(&case).expect("create temporary case");
    std::fs::write(
        case.join("case.toml"),
        r#"id = "V1-SRC-READER-entrypoint-failure"
rule = "V1-SRC-READER"
profile = "reader-v1"

[inputs]
source = "input.vib"

[expect]
accepted = false

[[expect.diagnostics]]
code = "@syntax.unmatched-delimiter"
level = "@error"

[expect.diagnostics.span]
start = 0
end = 1
"#,
    )
    .expect("write temporary manifest");
    std::fs::write(case.join("input.vib"), "(valid)\n")
        .expect("write temporary source");

    let status = Command::new(env!("CARGO_BIN_EXE_vibra-conformance"))
        .args(["--root"])
        .arg(&root)
        .status()
        .expect("run internal conformance entrypoint");
    assert!(!status.success(), "a failed conformance case must fail CI");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn internal_entrypoint_returns_nonzero_for_an_empty_corpus() {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "vibra-conformance-step4-empty-{serial}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("create empty corpus root");

    let status = Command::new(env!("CARGO_BIN_EXE_vibra-conformance"))
        .args(["--root"])
        .arg(&root)
        .status()
        .expect("run internal conformance entrypoint");
    assert!(!status.success(), "an empty corpus must fail CI");

    let _ = std::fs::remove_dir_all(root);
}
