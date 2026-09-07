//! Step 5 typed-literal and reference-execution contract tests.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]
#![allow(missing_docs)]

use std::path::{Path, PathBuf};

use vibra_conformance::{
    Case, CaseObservation, ConformanceProfile, ConformanceRunner, Corpus, HandlerError,
    InterpreterV1Handler, ProfileDispatcher, ProfileHandler, StaticV1TypeHandler,
};
use vibra_ir::Value;
use vibra_syntax::parse_data;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate has a workspace root")
        .to_path_buf()
}

fn corpus() -> Corpus {
    Corpus::discover(workspace_root().join("conformance/cases")).expect("Step 5 corpus")
}

fn case(id: &str) -> Case {
    corpus()
        .cases()
        .iter()
        .find(|case| case.manifest().id() == id)
        .cloned()
        .expect("Step 5 case")
}

#[test]
fn static_handler_produces_an_independent_typed_program_observation() {
    let case = case("V1-TYPE-INFER-primitives");
    let observation = StaticV1TypeHandler.run(&case).expect("type handler");
    assert!(observation.accepted);
    assert!(
        observation
            .types
            .as_deref()
            .is_some_and(|types| types.contains("@types.v1"))
    );
}

#[test]
fn typed_program_observation_is_valid_vibon() {
    let case = case("V1-TYPE-INFER-primitives");
    let observation = StaticV1TypeHandler.run(&case).expect("type handler");
    let types = observation.types.expect("typed program observation");
    let document =
        parse_data(Path::new("types.vibon"), &types).expect("typed observation loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    assert!(document.data().is_some());
}

#[test]
fn bytes_and_unicode_whitespace_observations_are_valid_vibon() {
    let values = [
        Value::Bytes(vec![0, 1, 255]).canonical_observation(),
        Value::Char(' ').canonical_observation(),
        Value::Char('\u{00A0}').canonical_observation(),
        Value::Char('\u{2003}').canonical_observation(),
        Value::Char('\u{2028}').canonical_observation(),
        Value::Char('😀').canonical_observation(),
    ];

    for value in values {
        let document = parse_data(Path::new("value.vibon"), &value)
            .expect("value observation loader");
        assert!(document.accepted(), "{:?}", document.diagnostics());
        assert!(document.data().is_some());
    }
}

#[test]
fn interpreter_handler_produces_an_empty_pure_trace() {
    let case = case("V1-RUNTIME-literal");
    let observation = InterpreterV1Handler
        .run(&case)
        .expect("interpreter handler");
    let repeated = InterpreterV1Handler.run(&case).expect("repeat handler");
    assert_eq!(observation, repeated);
    let execution = observation.interpreter.expect("execution");
    assert_eq!(
        execution.result.as_deref(),
        Some("(record type: @i32 value: 42i32)\n")
    );
    assert!(execution.audit_trace.is_empty());
}

#[derive(Clone, Debug)]
struct FixedHandler {
    observation: CaseObservation,
}

impl ProfileHandler for FixedHandler {
    fn run(&self, _case: &Case) -> Result<CaseObservation, HandlerError> {
        Ok(self.observation.clone())
    }
}

#[test]
fn runner_rejects_a_wrong_typed_program_snapshot() {
    let case = case("V1-TYPE-INFER-primitives");
    let report = ConformanceRunner::new(ProfileDispatcher::new().with_handler(
        ConformanceProfile::StaticV1,
        FixedHandler {
            observation: CaseObservation {
                accepted: true,
                types: Some("wrong".to_owned()),
                ..CaseObservation::default()
            },
        },
    ))
    .run_case(&case);
    assert!(matches!(
        report.status,
        vibra_conformance::CaseStatus::Failed { .. }
    ));
}

#[test]
fn runner_rejects_a_nonempty_trace_for_a_pure_program() {
    let case = case("V1-RUNTIME-literal");
    let report = ConformanceRunner::new(ProfileDispatcher::new().with_handler(
        ConformanceProfile::InterpreterV1,
        FixedHandler {
            observation: CaseObservation {
                accepted: true,
                interpreter: Some(vibra_conformance::ExecutionObservation {
                    result: Some("(record type: @i32 value: 42i32)\n".to_owned()),
                    audit_trace: vec!["ambient.clock".to_owned()],
                }),
                ..CaseObservation::default()
            },
        },
    ))
    .run_case(&case);
    assert!(matches!(
        report.status,
        vibra_conformance::CaseStatus::Failed { .. }
    ));
}
