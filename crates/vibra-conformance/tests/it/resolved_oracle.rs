//! Step 4 manifest and resolved-artifact oracle checks.

use crate::support::{FixedHandler, TempDir};

use vibra_conformance::{
    CaseManifest, CaseObservation, ConformanceOperation, ConformanceProfile,
    ConformanceRunner, Corpus, ProfileDispatcher,
};

struct TempCase {
    root: TempDir,
}

impl TempCase {
    fn new(id: &str, manifest: &str) -> Self {
        let root = TempDir::new("resolved-oracle");
        let directory = root.join(id);
        std::fs::create_dir_all(directory.join("tree")).expect("create temporary case");
        std::fs::write(directory.join("case.toml"), manifest)
            .expect("write temporary manifest");
        std::fs::write(directory.join("tree/project.vibon"), "project")
            .expect("write declared project");
        std::fs::write(directory.join("resolved.vibon"), "expected resolved\n")
            .expect("write resolved oracle");
        Self { root }
    }

    fn corpus(&self) -> Corpus {
        Corpus::discover(&self.root).expect("temporary case is valid")
    }
}

#[test]
fn resolve_operation_requires_the_declared_tree_project_marker() {
    let manifest = CaseManifest::from_str(
        r#"
id = "V1-TYPE-NAMES-resolve-binding"
rule = "V1-TYPE-NAMES-resolution"
profile = "static-v1"
operation = "resolve"

[inputs]
project = "tree/project.vibon"
tree = "tree"

[expect]
accepted = true
"#,
    )
    .expect("resolve operation manifest");
    assert_eq!(manifest.operation(), ConformanceOperation::Resolve);

    let outside = CaseManifest::from_str(
        r#"
id = "V1-TYPE-NAMES-resolve-outside"
rule = "V1-TYPE-NAMES-resolution"
profile = "static-v1"
operation = "resolve"

[inputs]
project = "project.vibon"
tree = "tree"

[expect]
accepted = true
"#,
    )
    .expect_err("resolve must bind project to the declared tree");
    assert!(
        outside
            .to_string()
            .contains("must be exactly `tree/project.vibon`")
    );
}

#[test]
fn runner_rejects_a_wrong_resolved_snapshot() {
    let case = TempCase::new(
        "V1-TYPE-NAMES-resolve-oracle",
        r#"
id = "V1-TYPE-NAMES-resolve-oracle"
rule = "V1-TYPE-NAMES-resolution"
profile = "static-v1"
operation = "resolve"

[inputs]
project = "tree/project.vibon"
tree = "tree"

[expect]
accepted = true
resolved = "resolved.vibon"
"#,
    );
    let report = ConformanceRunner::new(ProfileDispatcher::new().with_handler(
        ConformanceProfile::StaticV1,
        FixedHandler {
            observation: CaseObservation {
                accepted: true,
                resolved: Some("wrong resolved\n".to_owned()),
                ..CaseObservation::default()
            },
        },
    ))
    .run(&case.corpus());

    assert!(!report.is_success());
    assert!(format!("{:?}", report.cases()).contains("resolved snapshot mismatch"));
}
