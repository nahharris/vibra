//! The differential harness (`docs/spec/07-diagnostics-and-conformance.md`,
//! "Differential execution"; milestone 4 Step 4): every executable case runs in
//! both backends against its one expectation, and the report says what each
//! backend did.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Type, Value,
};

use vibra_conformance::{
    CaseObservation, CaseReport, CaseStatus, ConformanceProfile, ConformanceRunner,
    Corpus, ExecutionObservation, HandlerError, ProfileDispatcher, ProfileHandler,
    WasmObservation, WasmStatus, observe_wasm, standard_dispatcher,
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

const VOID_RESULT: &str = "(record type: @void value: void)\n";
const NO_EVENTS: &str = "(record format: @audit-trace.v1 events: (array))\n";

/// A temporary corpus root that removes itself.
struct TempCorpus {
    root: PathBuf,
}

impl TempCorpus {
    fn new() -> Self {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "vibra-differential-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("temporary corpus");
        Self { root }
    }

    /// Adds one `interpret` case that expects `accepted` and, when `result` is
    /// given, that result and no audit events.
    fn interpret(
        &self,
        id: &str,
        source: &str,
        accepted: bool,
        result: Option<&str>,
        extra_expect: &str,
    ) {
        let directory = self.root.join(id);
        std::fs::create_dir_all(&directory).expect("case directory");
        let snapshots = if result.is_some() {
            "interpreter = { result = \"result.vibon\", audit_trace = \"audit.vibon\" }\n"
        } else {
            ""
        };
        std::fs::write(
            directory.join("case.toml"),
            format!(
                "id = \"{id}\"\nrule = \"V1-RUNTIME\"\nprofile = \"interpreter-v1\"\noperation = \"interpret\"\n\n[inputs]\nsource = \"input.vib\"\n\n[expect]\naccepted = {accepted}\n{snapshots}{extra_expect}"
            ),
        )
        .expect("manifest");
        std::fs::write(directory.join("input.vib"), source).expect("input");
        if let Some(result) = result {
            std::fs::write(directory.join("result.vibon"), result).expect("result");
            std::fs::write(directory.join("audit.vibon"), NO_EVENTS).expect("audit");
        }
    }

    fn corpus(&self) -> Corpus {
        Corpus::discover(&self.root).expect("a valid corpus")
    }
}

impl Drop for TempCorpus {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Reports a fixed observation for every case.
struct Fixed(CaseObservation);

impl ProfileHandler for Fixed {
    fn run(
        &self,
        _case: &vibra_conformance::Case,
    ) -> Result<CaseObservation, HandlerError> {
        Ok(self.0.clone())
    }
}

fn run_with(corpus: &TempCorpus, handler: impl ProfileHandler + 'static) -> CaseReport {
    let runner = ConformanceRunner::new(
        ProfileDispatcher::new()
            .with_handler(ConformanceProfile::InterpreterV1, handler),
    );
    let corpus = corpus.corpus();
    runner.run_case(&corpus.cases()[0])
}

fn run_real(corpus: &TempCorpus) -> CaseReport {
    let runner = ConformanceRunner::new(standard_dispatcher());
    let corpus = corpus.corpus();
    runner.run_case(&corpus.cases()[0])
}

fn void_execution() -> ExecutionObservation {
    ExecutionObservation {
        result: Some(VOID_RESULT.to_owned()),
        audit_trace: Vec::new(),
    }
}

fn backends(report: &CaseReport) -> (&CaseStatus, &WasmStatus) {
    let backends = report.backends.as_ref().expect("an executable case");
    (
        &backends.interpreter,
        backends.wasm.as_ref().expect("a Wasm status"),
    )
}

/// One accepted `void` case expecting the void result.
fn void_case() -> TempCorpus {
    let corpus = TempCorpus::new();
    corpus.interpret(
        "V1-RUNTIME-synthetic-void",
        "(defn main () void (do))\n",
        true,
        Some(VOID_RESULT),
        "",
    );
    corpus
}

fn expected_observation(
    interpreter: Option<ExecutionObservation>,
    wasm: Option<WasmObservation>,
) -> CaseObservation {
    CaseObservation {
        accepted: true,
        interpreter,
        wasm,
        ..CaseObservation::default()
    }
}

/// The Wasm observation of the hand-built empty entry, which runs the module
/// the emitter produces through the same path the interpreter handlers use.
/// No checked source program lowers yet, because every one carries the
/// prelude's module values, so this is how the empty entry enters the harness.
fn empty_entry_wasm_observation() -> WasmObservation {
    let origin = || SourceOrigin::new("input.vib", ByteSpan::new(0, 1));
    let done = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), Type::Void),
        Expr::literal(Value::Void, origin()),
        origin(),
    )
    .expect("a checked function");
    observe_wasm(&CheckedProgram::try_new(vec![done], 0).expect("a program"))
}

#[test]
fn the_empty_entry_runs_in_both_backends_and_matches() {
    let wasm = empty_entry_wasm_observation();
    assert_eq!(wasm, WasmObservation::Completed(void_execution()));
    let report = run_with(
        &void_case(),
        Fixed(expected_observation(Some(void_execution()), Some(wasm))),
    );
    let (interpreter, wasm) = backends(&report);
    assert_eq!(interpreter, &CaseStatus::Passed, "{report:?}");
    assert_eq!(wasm, &WasmStatus::Matched, "{report:?}");
    assert_eq!(report.status, CaseStatus::Passed);
}

#[test]
fn a_checked_source_program_lowers_with_its_module_values() {
    // Every checked program holds the prelude's `true` and `false` (Step 2b),
    // which Step 5b lowers, so the corpus's own void case runs in both backends.
    let report = run_real(&void_case());
    let (interpreter, wasm) = backends(&report);
    assert_eq!(interpreter, &CaseStatus::Passed, "{report:?}");
    assert_eq!(wasm, &WasmStatus::Matched, "{report:?}");
    assert_eq!(report.status, CaseStatus::Passed);
}

#[test]
fn a_program_that_does_not_lower_still_passes_in_the_interpreter() {
    let corpus = TempCorpus::new();
    corpus.interpret(
        "V1-RUNTIME-synthetic-literal",
        "(defn answer () i32 ((lambda () i32 7i32)))\n",
        true,
        Some("(record type: @i32 value: 7i32)\n"),
        "",
    );
    let report = run_real(&corpus);
    let (interpreter, wasm) = backends(&report);
    assert_eq!(interpreter, &CaseStatus::Passed, "{report:?}");
    let WasmStatus::NotLowered { forms } = wasm else {
        panic!("expected not lowered, got {wasm:?}");
    };
    assert!(forms.iter().any(|form| form == "closure"), "{forms:?}");
    assert_eq!(
        report.status,
        CaseStatus::Passed,
        "a not-lowered case does not fail"
    );
}

#[test]
fn a_not_lowered_case_does_not_fail_the_run_and_is_counted() {
    let corpus = TempCorpus::new();
    corpus.interpret(
        "V1-RUNTIME-synthetic-literal",
        "(defn answer () i32 ((lambda () i32 7i32)))\n",
        true,
        Some("(record type: @i32 value: 7i32)\n"),
        "",
    );
    let runner = ConformanceRunner::new(standard_dispatcher());
    let report = runner.run(&corpus.corpus());
    assert!(report.is_success());
    let wasm = report.wasm_counts();
    assert_eq!((wasm.matched, wasm.failed, wasm.not_lowered), (0, 0, 1));    let interpreter = report.interpreter_counts();
    assert_eq!(
        (
            interpreter.passed,
            interpreter.failed,
            interpreter.unavailable
        ),
        (1, 0, 0)
    );
}

#[test]
fn a_rejected_case_is_no_executable_case_and_is_in_neither_backend_line() {
    let corpus = TempCorpus::new();
    corpus.interpret(
        "V1-RUNTIME-synthetic-rejected",
        "(defn broken () u64 true)\n",
        false,
        None,
        "\n[[expect.diagnostics]]\ncode = \"@type.mismatch\"\nlevel = \"@error\"\nsource = \"input.vib\"\nspan = [20, 24]\n",
    );
    assert!(!corpus.corpus().cases()[0].manifest().is_executable());
    let report = run_real(&corpus);
    assert_eq!(report.status, CaseStatus::Passed, "{report:?}");
    assert_eq!(report.backends, None, "{report:?}");
    let all = ConformanceRunner::new(standard_dispatcher()).run(&corpus.corpus());
    assert_eq!(all.interpreter_counts().passed, 0);
    assert_eq!(all.wasm_counts().matched, 0);
    assert_eq!(all.passed(), 1);
}

#[test]
fn a_forced_wasm_disagreement_fails_the_case_and_names_the_backend() {
    let wrong = ExecutionObservation {
        result: Some("(record type: @u64 value: 1u64)\n".to_owned()),
        audit_trace: Vec::new(),
    };
    let report = run_with(
        &void_case(),
        Fixed(expected_observation(
            Some(void_execution()),
            Some(WasmObservation::Completed(wrong)),
        )),
    );
    let (interpreter, wasm) = backends(&report);
    assert_eq!(interpreter, &CaseStatus::Passed);
    assert!(matches!(wasm, WasmStatus::Failed { .. }), "{wasm:?}");
    let CaseStatus::Failed { reason } = &report.status else {
        panic!("the case must fail: {report:?}");
    };
    assert!(reason.contains("wasm backend"), "{reason}");
    assert!(!reason.contains("interpreter backend"), "{reason}");
}

#[test]
fn a_forced_wasm_audit_trace_disagreement_fails_the_case() {
    let noisy = ExecutionObservation {
        result: Some(VOID_RESULT.to_owned()),
        audit_trace: vec!["an event".to_owned()],
    };
    let report = run_with(
        &void_case(),
        Fixed(expected_observation(
            Some(void_execution()),
            Some(WasmObservation::Completed(noisy)),
        )),
    );
    let CaseStatus::Failed { reason } = &report.status else {
        panic!("the case must fail: {report:?}");
    };
    assert!(
        reason.contains("wasm backend") && reason.contains("audit"),
        "{reason}"
    );
}

#[test]
fn an_interpreter_disagreement_fails_the_case_and_names_the_backend() {
    let wrong = ExecutionObservation {
        result: Some("(record type: @u64 value: 1u64)\n".to_owned()),
        audit_trace: Vec::new(),
    };
    let report = run_with(
        &void_case(),
        Fixed(expected_observation(
            Some(wrong),
            Some(WasmObservation::Completed(void_execution())),
        )),
    );
    let (interpreter, wasm) = backends(&report);
    assert!(
        matches!(interpreter, CaseStatus::Failed { .. }),
        "{interpreter:?}"
    );
    assert_eq!(wasm, &WasmStatus::Matched);
    let CaseStatus::Failed { reason } = &report.status else {
        panic!("the case must fail: {report:?}");
    };
    assert!(reason.contains("interpreter backend"), "{reason}");
    assert!(!reason.contains("wasm backend"), "{reason}");
}

#[test]
fn both_backends_failing_names_both() {
    let wrong = ExecutionObservation {
        result: Some("(record type: @u64 value: 1u64)\n".to_owned()),
        audit_trace: Vec::new(),
    };
    let report = run_with(
        &void_case(),
        Fixed(expected_observation(
            Some(wrong.clone()),
            Some(WasmObservation::Completed(wrong)),
        )),
    );
    let CaseStatus::Failed { reason } = &report.status else {
        panic!("the case must fail");
    };
    assert!(reason.contains("interpreter backend") && reason.contains("wasm backend"));
}

#[test]
fn a_wasm_backend_failure_fails_the_case() {
    let report = run_with(
        &void_case(),
        Fixed(expected_observation(
            Some(void_execution()),
            Some(WasmObservation::Failed {
                reason: "the module did not validate".to_owned(),
            }),
        )),
    );
    let CaseStatus::Failed { reason } = &report.status else {
        panic!("the case must fail");
    };
    assert!(
        reason.contains("wasm backend: the module did not validate"),
        "{reason}"
    );
}

#[test]
fn a_not_lowered_observation_is_recovered_and_counted() {
    let report = run_with(
        &void_case(),
        Fixed(expected_observation(
            Some(void_execution()),
            Some(WasmObservation::NotLowered {
                forms: vec!["closure".to_owned()],
            }),
        )),
    );
    assert_eq!(report.status, CaseStatus::Passed);
    assert_eq!(
        backends(&report).1,
        &WasmStatus::NotLowered {
            forms: vec!["closure".to_owned()]
        }
    );
}

#[test]
fn a_missing_wasm_observation_on_an_accepted_case_is_not_lowered() {
    // No Wasm execution handler took part: nothing is lowered, and the case
    // still passes on the interpreter side.
    let report = run_with(
        &void_case(),
        Fixed(expected_observation(Some(void_execution()), None)),
    );
    assert_eq!(report.status, CaseStatus::Passed);
    assert_eq!(
        backends(&report).1,
        &WasmStatus::NotLowered { forms: Vec::new() }
    );
}

#[test]
fn the_host_event_is_held_to_both_backends() {
    let corpus = TempCorpus::new();
    corpus.interpret(
        "V1-RUNTIME-synthetic-event",
        "(defn main () void (do))\n",
        true,
        None,
        "host_event = \"@runtime.memory-exhausted\"\n",
    );
    let event = || CaseObservation {
        accepted: true,
        host_event: Some("@runtime.memory-exhausted".to_owned()),
        ..CaseObservation::default()
    };
    let with_wasm = |wasm| CaseObservation {
        wasm: Some(wasm),
        ..event()
    };

    let matched = run_with(
        &corpus,
        Fixed(with_wasm(WasmObservation::HostEvent(
            "@runtime.memory-exhausted".to_owned(),
        ))),
    );
    assert_eq!(matched.status, CaseStatus::Passed, "{matched:?}");
    assert_eq!(backends(&matched).1, &WasmStatus::Matched);

    let completed = run_with(
        &corpus,
        Fixed(with_wasm(WasmObservation::Completed(void_execution()))),
    );
    let CaseStatus::Failed { reason } = &completed.status else {
        panic!("a module that completes where the event is expected disagrees");
    };
    assert!(
        reason.contains("wasm backend") && reason.contains("host event"),
        "{reason}"
    );

    let other = run_with(
        &corpus,
        Fixed(with_wasm(WasmObservation::HostEvent(
            "@runtime.other".to_owned(),
        ))),
    );
    assert!(matches!(other.status, CaseStatus::Failed { .. }));
}

#[test]
fn only_executable_cases_have_backend_statuses() {
    let corpus = TempCorpus::new();
    let directory = corpus.root.join("V1-DIAG-synthetic-static");
    std::fs::create_dir_all(&directory).expect("directory");
    std::fs::write(
        directory.join("case.toml"),
        "id = \"V1-DIAG-synthetic-static\"\nrule = \"V1-DIAG\"\nprofile = \"static-v1\"\n\n[expect]\naccepted = true\n",
    )
    .expect("manifest");
    let runner = ConformanceRunner::new(ProfileDispatcher::new().with_handler(
        ConformanceProfile::StaticV1,
        Fixed(CaseObservation::new(true)),
    ));
    let report = runner.run(&corpus.corpus());
    assert_eq!(report.cases()[0].backends, None);
    assert_eq!(report.wasm_counts().matched, 0);
}

#[test]
fn the_report_prints_both_backend_lines() {
    let corpus = TempCorpus::new();
    // The binary requires a reader-v1 case; copy a small real one.
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cases/V1-SRC-READER-data-loader-extension");
    let target = corpus.root.join("V1-SRC-READER-data-loader-extension");
    std::fs::create_dir_all(&target).expect("directory");
    for file in ["case.toml", "input.vib"] {
        std::fs::copy(source.join(file), target.join(file)).expect("copy");
    }
    corpus.interpret(
        "V1-RUNTIME-synthetic-void",
        "(defn main () void (do))\n",
        true,
        Some(VOID_RESULT),
        "",
    );
    corpus.interpret(
        "V1-RUNTIME-synthetic-literal",
        "(defn answer () i32 ((lambda () i32 7i32)))\n",
        true,
        Some("(record type: @i32 value: 7i32)\n"),
        "",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vibra-conformance"))
        .arg("--root")
        .arg(&corpus.root)
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("interpreter backend: 2 passed, 0 failed, 0 unavailable\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("wasm backend: 1 matched, 0 failed, 1 not lowered\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("conformance total: 3 passed, 0 failed, 0 unavailable\n"),
        "{stdout}"
    );
}
