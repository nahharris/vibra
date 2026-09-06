//! Static-v1 project corpus coverage.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::{Path, PathBuf};

use vibra_conformance::{
    Case, CaseObservation, ConformanceProfile, ConformanceRunner, Corpus,
    DispatchResult, HandlerError, ProfileDispatcher, ProfileHandler, ReaderV1Handler,
    StaticV1ProjectHandler, StaticV1ResolveHandler, StaticV1SourceGraphHandler,
};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lives two levels below the workspace root")
        .to_path_buf()
}

#[test]
fn static_project_cases_are_executed_by_the_real_handler() {
    let corpus = Corpus::discover(workspace_root().join("conformance/cases"))
        .expect("workspace corpus");
    let static_cases = corpus
        .cases()
        .iter()
        .filter(|case| case.manifest().profile() == ConformanceProfile::StaticV1)
        .collect::<Vec<_>>();
    assert!(static_cases.len() >= 5);

    let dispatcher = ProfileDispatcher::new()
        .with_handler(ConformanceProfile::ReaderV1, ReaderV1Handler)
        .with_handler(ConformanceProfile::StaticV1, StaticV1ProjectHandler)
        .with_additional_handler(
            ConformanceProfile::StaticV1,
            StaticV1SourceGraphHandler,
        );
    let dispatcher = dispatcher
        .with_additional_handler(ConformanceProfile::StaticV1, StaticV1ResolveHandler);
    let report = ConformanceRunner::new(dispatcher).run(&corpus);
    assert_eq!(report.failed(), 0, "static project cases must pass");
    assert_eq!(report.unavailable(), 0);
}

#[test]
fn valid_project_observation_keeps_source_identity_and_schema_order() {
    let corpus = Corpus::discover(workspace_root().join("conformance/cases"))
        .expect("workspace corpus");
    let case = corpus
        .cases()
        .iter()
        .find(|case| case.manifest().id() == "V1-PROJECT-static-schema-order")
        .expect("schema-order case");
    let observation = StaticV1ProjectHandler
        .run(case)
        .expect("static project handler");
    assert!(observation.accepted);
    assert!(observation.diagnostics.is_empty());
    let formatted = observation.formatted.expect("canonical observation");
    assert!(formatted.contains("; project comment"));
    assert!(
        formatted.find("format:").expect("format")
            < formatted.find("package:").expect("package")
    );
}

#[derive(Clone, Copy, Debug)]
struct NeverProjectHandler;

impl ProfileHandler for NeverProjectHandler {
    fn can_run(&self, _case: &Case) -> bool {
        false
    }

    fn run(&self, _case: &Case) -> Result<CaseObservation, HandlerError> {
        Err(HandlerError::new("the non-project slice must not run"))
    }
}

#[derive(Clone, Copy, Debug)]
struct ClaimingProjectHandler;

impl ProfileHandler for ClaimingProjectHandler {
    fn run(&self, _case: &Case) -> Result<CaseObservation, HandlerError> {
        Ok(CaseObservation::new(true))
    }
}

#[test]
fn static_project_handler_composes_with_future_static_slices() {
    let corpus = Corpus::discover(workspace_root().join("conformance/cases"))
        .expect("workspace corpus");
    let case = corpus
        .cases()
        .iter()
        .find(|case| case.manifest().id() == "V1-PROJECT-static-schema-order")
        .expect("schema-order case");
    let dispatcher = ProfileDispatcher::new()
        .with_handler(ConformanceProfile::StaticV1, StaticV1ProjectHandler)
        .with_additional_handler(ConformanceProfile::StaticV1, NeverProjectHandler);

    let result = dispatcher.dispatch(case);
    let DispatchResult::Executed {
        provided,
        observation,
        ..
    } = result
    else {
        panic!("project handler was replaced by the additional static slice");
    };
    assert_eq!(provided, ConformanceProfile::StaticV1);
    assert!(observation.accepted);
}

#[test]
fn static_dispatch_rejects_ambiguous_same_profile_handlers() {
    let corpus = Corpus::discover(workspace_root().join("conformance/cases"))
        .expect("workspace corpus");
    let case = corpus
        .cases()
        .iter()
        .find(|case| case.manifest().id() == "V1-PROJECT-static-schema-order")
        .expect("schema-order case");
    let dispatcher = ProfileDispatcher::new()
        .with_handler(ConformanceProfile::StaticV1, StaticV1ProjectHandler)
        .with_additional_handler(ConformanceProfile::StaticV1, ClaimingProjectHandler);

    let result = dispatcher.dispatch(case);
    let DispatchResult::Failed { error, .. } = result else {
        panic!("overlapping handlers must be rejected");
    };
    assert!(error.message().contains("multiple handlers"));
}
