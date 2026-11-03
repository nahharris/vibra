//! Step 5 typed-literal and runner contract tests that a corpus case cannot
//! state. The typed observation and the result and trace of `V1-TYPE-INFER-primitives`
//! and `V1-RUNTIME-literal` are held by those cases.

use crate::support::{FixedHandler, corpus_case};

use std::path::Path;

use vibra_conformance::{
    BackendObservation, CaseObservation, ConformanceProfile, ConformanceRunner,
    ProfileDispatcher, ProfileHandler, StaticV1WorkspaceCheckHandler,
};
use vibra_ir::Value;
use vibra_syntax::parse_data;

#[test]
fn typed_program_observation_is_valid_vibon() {
    let case = corpus_case("V1-TYPE-INFER-primitives");
    let observation = StaticV1WorkspaceCheckHandler
        .run(&case)
        .expect("workspace-check handler");
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
fn runner_rejects_a_wrong_typed_program_snapshot() {
    let case = corpus_case("V1-TYPE-INFER-primitives");
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
    let case = corpus_case("V1-RUNTIME-literal");
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
