//! The independent lifecycle corpus must run real, non-executing services.

use vibra_conformance::{
    ConformanceOperation, ConformanceRunner, Corpus, standard_dispatcher,
};

use crate::support::workspace_root;

#[test]
fn every_authored_codebase_scenario_runs_through_the_real_handler() {
    let corpus = Corpus::discover(workspace_root().join("conformance/cases"))
        .expect("valid corpus");
    let cases = corpus
        .cases()
        .iter()
        .filter(|case| case.manifest().operation() == ConformanceOperation::Codebase)
        .collect::<Vec<_>>();
    assert_eq!(
        cases.len(),
        7,
        "review the scenario inventory when it changes"
    );
    let runner = ConformanceRunner::new(standard_dispatcher());
    let reports = runner.run_cases(&cases);
    for report in reports {
        assert!(
            matches!(report.status, vibra_conformance::CaseStatus::Passed),
            "{report:?}"
        );
    }
}
