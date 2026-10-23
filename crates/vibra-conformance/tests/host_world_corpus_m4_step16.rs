#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

//! The corpus `host-world` input (Milestone 4, Step 16a; ledger D20.3).
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Conformance corpus": a
//! `workspace-run` or `workspace-test` case may supply a `host-world` input, the
//! path of a `@host-world.v1` document, which is copied fresh for each test and
//! each backend. The corpus counts do not move: these cases are synthetic
//! corpora of the host-language suite, not checked-in conformance cases.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_conformance::{
    CaseManifest, CaseStatus, ConformanceRunner, Corpus, HostWorldError,
    InterpreterV1WorkspaceRunHandler, InterpreterV1WorkspaceTestHandler,
    ProfileHandler, standard_dispatcher,
};
use vibra_diagnostics::DiagnosticCode;
use vibra_host::{HostProvider, HostWorld, WorldProvider};

fn manifest(operation: &str, extra_input: &str) -> String {
    let profile = "interpreter-v1";
    format!(
        r#"
id = "V1-RUNTIME-world-{operation}"
rule = "V1-RUNTIME"
profile = "{profile}"
operation = "{operation}"

[inputs]
project = "tree/project.vibon"
tree = "tree"
{extra_input}

[expect]
accepted = true
interpreter = {{ result = "result.vibon", audit_trace = "audit.vibon" }}
"#
    )
}

#[test]
fn a_workspace_case_may_name_a_host_world_and_the_loader_keeps_the_path() {
    for operation in ["workspace-run", "workspace-test"] {
        let text = if operation == "workspace-test" {
            manifest(operation, "host-world = \"world.vibon\"")
                .replace(", audit_trace = \"audit.vibon\"", "")
        } else {
            manifest(operation, "host-world = \"world.vibon\"")
        };
        let parsed = CaseManifest::from_str(&text).expect(operation);
        assert_eq!(parsed.inputs.host_world.as_deref(), Some("world.vibon"));
    }
    let without =
        CaseManifest::from_str(&manifest("workspace-run", "")).expect("no world");
    assert_eq!(without.inputs.host_world, None);
}

#[test]
fn only_workspace_run_and_workspace_test_take_a_host_world() {
    // `interpret` is pure and has no host world.
    let interpret = r#"
id = "V1-RUNTIME-world-interpret"
rule = "V1-RUNTIME"
profile = "interpreter-v1"
operation = "interpret"

[inputs]
source = "main.vib"
host-world = "world.vibon"

[expect]
accepted = true
"#;
    let error = CaseManifest::from_str(interpret).expect_err("interpret is pure");
    assert!(error.to_string().contains("host-world"), "{error}");

    let check = r#"
id = "V1-PROJECT-world-check"
rule = "V1-PROJECT"
profile = "static-v1"
operation = "workspace-check"

[inputs]
project = "tree/project.vibon"
tree = "tree"
host-world = "world.vibon"

[expect]
accepted = true
"#;
    assert!(CaseManifest::from_str(check).is_err());
}

#[test]
fn the_world_path_is_a_relative_vibon_file_of_the_case() {
    for path in [
        "../world.vibon",
        "/world.vibon",
        "world.json",
        "",
        "dir/../../w.vibon",
        // The tree is the workspace and holds no world.
        "tree/world.vibon",
    ] {
        let text = manifest("workspace-run", &format!("host-world = \"{path}\""));
        assert!(CaseManifest::from_str(&text).is_err(), "{path:?}");
    }
    for path in ["world.vibon", "worlds/empty-stdin.vibon"] {
        let text = manifest("workspace-run", &format!("host-world = \"{path}\""));
        assert!(CaseManifest::from_str(&text).is_ok(), "{path:?}");
    }
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "vibra-conformance-world-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("temporary corpus");
        Self(root)
    }

    /// One `workspace-run` case with a trivial entry and the given world text.
    fn case(&self, id: &str, accepted: bool, world: Option<&str>, expected: &str) {
        let directory = self.0.join(id);
        std::fs::create_dir_all(directory.join("tree/src/app")).expect("tree");
        std::fs::write(
            directory.join("tree/project.vibon"),
            "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))) dependencies: (dict))\n",
        )
        .expect("project");
        std::fs::write(
            directory.join("tree/src/app/main.vib"),
            "(defn execute () void (do))\n",
        )
        .expect("source");
        std::fs::write(
            directory.join("result.vibon"),
            "(record type: @void value: void)\n",
        )
        .expect("result");
        std::fs::write(
            directory.join("audit.vibon"),
            "(record format: @audit-trace.v1 events: (array))\n",
        )
        .expect("audit");
        let world_input = world.map_or_else(String::new, |text| {
            std::fs::write(directory.join("world.vibon"), text).expect("world");
            "host-world = \"world.vibon\"\n".to_owned()
        });
        let outcome = if accepted {
            "accepted = true\ninterpreter = { result = \"result.vibon\", audit_trace = \"audit.vibon\" }\n".to_owned()
        } else {
            expected.to_owned()
        };
        std::fs::write(
            directory.join("case.toml"),
            format!(
                "id = \"{id}\"\nrule = \"V1-RUNTIME\"\nprofile = \"interpreter-v1\"\noperation = \"workspace-run\"\n\n[inputs]\nproject = \"tree/project.vibon\"\ntree = \"tree\"\n{world_input}\n[expect]\n{outcome}"
            ),
        )
        .expect("manifest");
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const GOOD_WORLD: &str =
    "(record format: @host-world.v1 stdin: \"hi\" files: (dict \"/f\" \"orig\"))\n";

#[test]
fn a_case_with_a_valid_world_loads_a_fresh_copy_each_time() {
    let temp = Temp::new();
    temp.case("V1-RUNTIME-world-valid", true, Some(GOOD_WORLD), "");
    let corpus = Corpus::discover(&temp.0).expect("corpus");
    let case = &corpus.cases()[0];

    let mut first = WorldProvider::new(&case.host_world().expect("world"));
    first.write_text("/f", "changed").expect("writes");
    // A second load is the document again, not the first run's writes.
    let world = case.host_world().expect("world");
    assert_eq!(world, HostWorld::decode("world.vibon", GOOD_WORLD).unwrap());
    let mut second = WorldProvider::new(&world);
    assert_eq!(second.read_text("/f").ok(), Some("orig".to_owned()));

    // A case without the input runs in the default world.
    temp.case("V1-RUNTIME-world-default", true, None, "");
    let corpus = Corpus::discover(&temp.0).expect("corpus");
    let defaulted = corpus
        .cases()
        .iter()
        .find(|case| case.manifest().id() == "V1-RUNTIME-world-default")
        .expect("case");
    assert_eq!(
        defaulted.host_world().expect("default"),
        HostWorld::default()
    );
}

#[test]
fn a_malformed_world_is_reported_once_per_diagnostic_at_the_document() {
    let temp = Temp::new();
    temp.case(
        "V1-RUNTIME-world-malformed",
        true,
        Some("(record format: @host-world.v1 clock: (record wall: 0u64) random: (record seed: \"x\") network: (array))\n"),
        "",
    );
    let corpus = Corpus::discover(&temp.0).expect("corpus");
    let case = &corpus.cases()[0];
    let Err(HostWorldError::Invalid(diagnostics)) = case.host_world() else {
        panic!("the world must be rejected");
    };
    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
    let mut seen = std::collections::BTreeSet::new();
    for diagnostic in &diagnostics {
        assert_eq!(diagnostic.source_id(), Some("world.vibon"));
        assert!(
            matches!(
                diagnostic.code(),
                DiagnosticCode::DataInvalidShape | DiagnosticCode::DataInvalidValue
            ),
            "{diagnostic:?}"
        );
        assert!(
            seen.insert((diagnostic.code().as_atom(), diagnostic.primary_span())),
            "reported twice: {diagnostic:?}"
        );
    }

    for handler in [
        &InterpreterV1WorkspaceRunHandler as &dyn ProfileHandler,
        &InterpreterV1WorkspaceTestHandler,
    ] {
        let observation = handler.run(case).expect("a handler outcome, not a failure");
        assert!(!observation.accepted);
        assert_eq!(observation.diagnostics.len(), 3);
    }
}

#[test]
fn a_malformed_world_fails_its_case_and_not_the_corpus() {
    let temp = Temp::new();
    temp.case("V1-RUNTIME-world-a-good", true, Some(GOOD_WORLD), "");
    // Expects the run to succeed, but the world is malformed: this case fails.
    temp.case(
        "V1-RUNTIME-world-b-bad",
        true,
        Some("(record format: @host-world.v1 stdin: 7u8)\n"),
        "",
    );
    // Authored as a negative case, the same malformed world passes.
    temp.case(
        "V1-RUNTIME-world-c-negative",
        false,
        Some("(record format: @host-world.v1 stdin: 7u8)\n"),
        "accepted = false\n\n[[expect.diagnostics]]\ncode = \"@data.invalid-shape\"\nlevel = \"@error\"\nsource = \"world.vibon\"\nspan = [38, 41]\n",
    );
    temp.case("V1-RUNTIME-world-d-good", true, None, "");

    let corpus = Corpus::discover(&temp.0).expect("corpus");
    let report = ConformanceRunner::new(standard_dispatcher()).run(&corpus);
    let status = |id: &str| {
        report
            .cases()
            .iter()
            .find(|case| case.case_id == id)
            .map(|case| case.status.clone())
            .unwrap_or_else(|| panic!("no report for {id}"))
    };
    assert!(
        matches!(status("V1-RUNTIME-world-a-good"), CaseStatus::Passed),
        "{report:?}"
    );
    assert!(
        matches!(status("V1-RUNTIME-world-b-bad"), CaseStatus::Failed { .. }),
        "{report:?}"
    );
    assert!(
        matches!(status("V1-RUNTIME-world-c-negative"), CaseStatus::Passed),
        "{report:?}"
    );
    assert!(
        matches!(status("V1-RUNTIME-world-d-good"), CaseStatus::Passed),
        "{report:?}"
    );
    assert_eq!(report.failed(), 1);
}
