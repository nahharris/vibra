//! Step 5 typed-literal and runner contract tests that a corpus case cannot
//! state. The typed observation and the result and trace of `V1-TYPE-INFER-primitives`
//! and `V1-RUNTIME-literal` are held by those cases.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]
#![allow(missing_docs)]

use std::path::Path;

use vibra_conformance::{
    BackendObservation, Case, CaseObservation, ConformanceProfile, ConformanceRunner,
    Corpus, HandlerError, ProfileDispatcher, ProfileHandler, StaticV1TypeHandler,
};
use vibra_ir::Value;
use vibra_syntax::parse_data;

/// The one case `id`, discovered from its own directory and not from the whole
/// corpus.
fn case(id: &str) -> Case {
    Corpus::discover(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../conformance/cases")
            .join(id),
    )
    .expect("Step 5 case directory")
    .cases()
    .first()
    .cloned()
    .expect("Step 5 case")
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
                interpreter: Some(BackendObservation::new(
                    vibra_conformance::ExecutionObservation {
                        result: Some("(record type: @i32 value: 42i32)\n".to_owned()),
                        audit_trace: vec![vibra_host::AuditEvent::completed(
                            &vibra_host::HostCall::NowWall,
                            &Ok(vibra_host::HostOk::Integer(0)),
                        )],
                    },
                )),
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
