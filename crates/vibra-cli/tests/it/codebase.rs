//! Process-level acceptance checks for the durable codebase service.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::support::{
    TempDir, app_target, assert_envelope_schema, json, manifest, run_workspace,
};

const MAIN: &str = "(defn main () void (do))\n";
const MAIN_ENTITY: &str = "(record kind: @declaration identity: \"@demo@0.1.0/app.main.main\" entity-kind: @function)";

fn demo(label: &str) -> TempDir {
    TempDir::project(
        label,
        &manifest("demo", &app_target("main", ""), "(dict)"),
        &[("src/app/main.vib", MAIN)],
    )
}

fn call(root: &Path, arguments: &[&str]) -> (i32, Value) {
    let root = root.canonicalize().expect("canonical temporary workspace");
    let output = run_workspace(&root, arguments, None);
    let code = output.status.code().unwrap_or(-1);
    let envelope = json(&output);
    assert_envelope_schema(&envelope);
    (code, envelope)
}

fn init(root: &Path) {
    let (code, envelope) = call(root, &["--format", "json", "codebase", "init"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["command"], "codebase-init");
}

fn successful_import(root: &Path, expected: &str) -> Value {
    let (code, envelope) = call(
        root,
        &[
            "--format", "json", "codebase", "import", "--expect", expected,
        ],
    );
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["command"], "codebase-import");
    envelope["payload"].clone()
}

#[test]
fn initializes_a_codebase_and_reports_a_versioned_result() {
    let root = TempDir::new("codebase-init");
    let (code, envelope) = call(root.path(), &["--format", "json", "codebase", "init"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["command"], "codebase-init");
    assert_eq!(envelope["result"], "@command.ok");
    assert_eq!(envelope["payload"]["store"], ".vibra/codebase.sqlite");
    assert!(root.path().join(".vibra/codebase.sqlite").is_file());
}

#[test]
fn reading_an_absent_store_does_not_create_one() {
    let root = TempDir::new("codebase-missing");
    let (code, envelope) = call(root.path(), &["--format", "json", "codebase", "head"]);
    assert_eq!(code, 3, "{envelope}");
    assert_eq!(envelope["command"], "codebase-head");
    assert_eq!(envelope["result"], "@command.operational-failure");
    assert_eq!(envelope["diagnostics"][0]["code"], "@store.not-found");
    assert_eq!(envelope["payload"], serde_json::json!({}));
    assert!(!root.path().join(".vibra").exists());
}

#[test]
fn codebase_reads_and_open_attempts_never_create_an_absent_store() {
    let root = TempDir::new("codebase-read-only-open");
    let zero =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    let commands: &[&[&str]] = &[
        &["codebase", "lookup", "--revision", zero, MAIN_ENTITY],
        &["codebase", "read", "--revision", zero, zero],
        &["codebase", "verify"],
        &["codebase", "reindex"],
        &[
            "codebase",
            "export",
            "--revision",
            zero,
            "--destination",
            "out",
        ],
        &["codebase", "backup", "backup.sqlite"],
    ];
    for arguments in commands {
        let mut command = vec!["--format", "json"];
        command.extend_from_slice(arguments);
        let (code, envelope) = call(root.path(), &command);
        assert_eq!(code, 3, "{arguments:?}: {envelope}");
        assert_eq!(envelope["diagnostics"][0]["code"], "@store.not-found");
        assert!(!root.path().join(".vibra").exists(), "{arguments:?}");
    }
}

#[test]
fn an_initialized_empty_store_reports_an_unknown_default_head() {
    let root = TempDir::new("codebase-empty-head");
    init(root.path());
    let (code, envelope) = call(root.path(), &["--format", "json", "codebase", "head"]);
    assert_eq!(code, 3, "{envelope}");
    assert_eq!(envelope["diagnostics"][0]["code"], "@codebase.unknown-head");
}

#[test]
fn codebase_grammar_is_closed_and_failures_have_empty_payloads() {
    let root = TempDir::new("codebase-grammar");
    init(root.path());
    let invalid = [
        ["codebase", "query"].as_slice(),
        ["codebase", "verify", "extra"].as_slice(),
        [
            "codebase",
            "lookup",
            "--head",
            "main",
            "--revision",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            MAIN_ENTITY,
        ]
        .as_slice(),
        [
            "codebase",
            "export",
            "--head",
            "main",
            "--destination",
            "out",
            "--write",
        ]
        .as_slice(),
    ];
    for arguments in invalid {
        let mut command = vec!["--format", "json"];
        command.extend_from_slice(arguments);
        let (code, envelope) = call(root.path(), &command);
        assert_eq!(code, 2, "{arguments:?}: {envelope}");
        assert_eq!(envelope["result"], "@command.invalid-input");
        assert_eq!(envelope["payload"], serde_json::json!({}));
    }
    assert_eq!(
        fs::read_dir(root.path().join(".vibra"))
            .expect("store directory remains readable")
            .count(),
        1,
        "invalid commands must not create sidecar files",
    );
}

#[test]
fn malformed_revision_is_a_registered_codebase_input_error() {
    let root = TempDir::new("codebase-bad-revision");
    init(root.path());
    let (code, envelope) = call(
        root.path(),
        &[
            "--format",
            "json",
            "codebase",
            "read",
            "--revision",
            "not-an-address",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        ],
    );
    assert_eq!(code, 3, "{envelope}");
    assert_eq!(envelope["result"], "@command.operational-failure");
    assert_eq!(
        envelope["diagnostics"][0]["code"],
        "@codebase.invalid-input"
    );
    assert_eq!(envelope["payload"], serde_json::json!({}));

    let (entity_code, entity_envelope) = call(
        root.path(),
        &[
            "--format",
            "json",
            "codebase",
            "lookup",
            "--revision",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "@app.main.main",
        ],
    );
    assert_eq!(entity_code, 3, "{entity_envelope}");
    assert_eq!(
        entity_envelope["diagnostics"][0]["code"],
        "@codebase.invalid-input"
    );
    assert_eq!(entity_envelope["payload"], serde_json::json!({}));

    let (plan_code, plan_envelope) = call(
        root.path(),
        &[
            "--format",
            "json",
            "codebase",
            "export",
            "--revision",
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "--destination",
            "out",
            "--write",
            "--plan",
            "bad-plan",
        ],
    );
    assert_eq!(plan_code, 3, "{plan_envelope}");
    assert_eq!(
        plan_envelope["diagnostics"][0]["code"],
        "@codebase.invalid-input"
    );
    assert_eq!(plan_envelope["payload"], serde_json::json!({}));
}

#[test]
fn import_reopen_lookup_read_verify_and_reindex_are_revision_scoped() {
    let root = demo("codebase-lifecycle");
    let first_source =
        "(defn dependency () u32 1u32)\n(defn main () void (do (dependency) void))\n";
    root.write("src/app/main.vib", first_source);
    init(root.path());

    let first = successful_import(root.path(), "absent");
    assert_eq!(first["head"], "main");
    assert_eq!(first["published"], true);
    let first_revision = first["revision"]
        .as_str()
        .expect("revision string")
        .to_owned();

    let (head_code, head) =
        call(root.path(), &["--format", "json", "codebase", "head"]);
    assert_eq!(head_code, 0, "{head}");
    assert_eq!(head["payload"]["head"], "main");
    assert_eq!(head["payload"]["revision"], first_revision);

    let (lookup_code, lookup) = call(
        root.path(),
        &[
            "--format",
            "json",
            "codebase",
            "lookup",
            "--revision",
            &first_revision,
            MAIN_ENTITY,
        ],
    );
    assert_eq!(lookup_code, 0, "{lookup}");
    assert_eq!(lookup["payload"]["revision"], first_revision);
    assert_eq!(lookup["payload"]["entity"], MAIN_ENTITY);
    assert!(lookup["payload"]["part"].as_array().is_some());
    let first_object = lookup["payload"]["object"]
        .as_str()
        .expect("object address")
        .to_owned();

    let unknown_revision = format!("sha256:{}", "0".repeat(64));
    assert_ne!(unknown_revision, first_revision);
    let unknown_entity = "(record kind: @declaration identity: \"@demo@0.1.0/app.main.missing\" entity-kind: @function)";
    let unknown_address = format!("sha256:{}", "0".repeat(64));
    let unknown_selectors = [
        (
            vec![
                "--format",
                "json",
                "codebase",
                "lookup",
                "--revision",
                unknown_revision.as_str(),
                MAIN_ENTITY,
            ],
            "@codebase.unknown-revision",
            "codebase-lookup",
        ),
        (
            vec![
                "--format",
                "json",
                "codebase",
                "lookup",
                "--revision",
                first_revision.as_str(),
                unknown_entity,
            ],
            "@codebase.unknown-entity",
            "codebase-lookup",
        ),
        (
            vec![
                "--format",
                "json",
                "codebase",
                "read",
                "--revision",
                first_revision.as_str(),
                unknown_address.as_str(),
            ],
            "@codebase.unknown-object",
            "codebase-read",
        ),
    ];
    for (arguments, diagnostic, command) in unknown_selectors {
        let (status, envelope) = call(root.path(), &arguments);
        assert_eq!(status, 3, "{envelope}");
        assert_eq!(envelope["command"], command);
        assert_eq!(envelope["result"], "@command.operational-failure");
        assert_eq!(envelope["diagnostics"][0]["code"], diagnostic);
        assert_eq!(envelope["payload"], serde_json::json!({}));
    }

    assert_ne!(unknown_address, first_object);

    let (read_code, read) = call(
        root.path(),
        &[
            "--format",
            "json",
            "codebase",
            "read",
            "--revision",
            &first_revision,
            &first_object,
        ],
    );
    assert_eq!(read_code, 0, "{read}");
    assert_eq!(read["payload"]["object"], first_object);
    assert!(
        !read["payload"]["payload"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );
    assert_eq!(read["payload"]["revision"], first_revision);
    let first_call_target = read["payload"]["references"]
        .as_array()
        .expect("reference list")
        .iter()
        .find(|reference| reference["kind"] == "call")
        .and_then(|reference| reference["target"].as_str())
        .expect("main call edge target")
        .to_owned();

    let (verify_code, verify) =
        call(root.path(), &["--format", "json", "codebase", "verify"]);
    assert_eq!(verify_code, 0, "{verify}");
    assert_eq!(verify["payload"]["valid"], true);

    let (reindex_code, reindex) =
        call(root.path(), &["--format", "json", "codebase", "reindex"]);
    assert_eq!(reindex_code, 0, "{reindex}");
    assert!(reindex["payload"]["entityCount"].as_u64().is_some());

    root.write(
        "src/app/main.vib",
        "(defn dependency () u32 2u32)\n(defn main () void (do (dependency) void))\n",
    );
    let second = successful_import(root.path(), &first_revision);
    assert_eq!(second["published"], true);
    let second_revision = second["revision"].as_str().expect("revision string");
    assert_ne!(first_revision, second_revision);

    let (new_lookup_code, new_lookup) = call(
        root.path(),
        &[
            "--format",
            "json",
            "codebase",
            "lookup",
            "--revision",
            second_revision,
            MAIN_ENTITY,
        ],
    );
    assert_eq!(new_lookup_code, 0, "{new_lookup}");
    let new_object = new_lookup["payload"]["object"]
        .as_str()
        .expect("new object address")
        .to_owned();
    let (old_read_code, old_read) = call(
        root.path(),
        &[
            "--format",
            "json",
            "codebase",
            "read",
            "--revision",
            &first_revision,
            &first_object,
        ],
    );
    assert_eq!(old_read_code, 0, "{old_read}");
    assert_eq!(old_read["payload"]["revision"], first_revision);
    let old_call_target = old_read["payload"]["references"]
        .as_array()
        .expect("old revision references")
        .iter()
        .find(|reference| reference["kind"] == "call")
        .and_then(|reference| reference["target"].as_str())
        .expect("old call edge target");
    let (new_read_code, new_read) = call(
        root.path(),
        &[
            "--format",
            "json",
            "codebase",
            "read",
            "--revision",
            second_revision,
            &new_object,
        ],
    );
    assert_eq!(new_read_code, 0, "{new_read}");
    assert_eq!(new_read["payload"]["revision"], second_revision);
    let new_call_target = new_read["payload"]["references"]
        .as_array()
        .expect("new revision references")
        .iter()
        .find(|reference| reference["kind"] == "call")
        .and_then(|reference| reference["target"].as_str())
        .expect("new call edge target");
    assert_eq!(old_call_target, first_call_target);
    assert_ne!(old_call_target, new_call_target);

    let (old_lookup_code, old_lookup) = call(
        root.path(),
        &[
            "--format",
            "json",
            "codebase",
            "lookup",
            "--revision",
            &first_revision,
            MAIN_ENTITY,
        ],
    );
    assert_eq!(old_lookup_code, 0, "{old_lookup}");
    assert_eq!(old_lookup["payload"]["object"], first_object);
}

#[test]
fn export_preview_apply_refuses_a_destination_created_after_preview() {
    let root = demo("codebase-export");
    init(root.path());
    let imported = successful_import(root.path(), "absent");
    let revision = imported["revision"].as_str().expect("revision string");
    let destination = root.path().join("exported");
    let destination_text = "exported";

    let (preview_code, preview) = call(
        root.path(),
        &[
            "--format",
            "json",
            "codebase",
            "export",
            "--revision",
            revision,
            "--destination",
            destination_text,
        ],
    );
    assert_eq!(preview_code, 0, "{preview}");
    assert_eq!(preview["payload"]["written"], false);
    let plan = preview["payload"]["plan"].as_str().expect("plan address");
    fs::create_dir(&destination).expect("create a race destination");
    fs::write(destination.join("sentinel"), b"keep me").expect("write sentinel");

    let (apply_code, apply) = call(
        root.path(),
        &[
            "--format",
            "json",
            "codebase",
            "export",
            "--revision",
            revision,
            "--destination",
            destination_text,
            "--write",
            "--plan",
            plan,
        ],
    );
    assert_eq!(apply_code, 3, "{apply}");
    assert_eq!(apply["result"], "@command.operational-failure");
    assert_eq!(
        fs::read(destination.join("sentinel")).expect("sentinel remains"),
        b"keep me"
    );
}

#[test]
fn backup_restore_reopens_an_independent_store() {
    let root = demo("codebase-backup-restore");
    init(root.path());
    let imported = successful_import(root.path(), "absent");
    let revision = imported["revision"].as_str().expect("revision string");
    let backup = "backup.sqlite";
    let restored = ".vibra/restored.sqlite";

    let (backup_code, backup_envelope) = call(
        root.path(),
        &["--format", "json", "codebase", "backup", backup],
    );
    assert_eq!(backup_code, 0, "{backup_envelope}");
    assert_eq!(backup_envelope["command"], "codebase-backup");
    assert!(root.path().join(backup).is_file());

    let (restore_code, restore_envelope) = call(
        root.path(),
        &[
            "--format", "json", "codebase", "restore", backup, "--store", restored,
        ],
    );
    assert_eq!(restore_code, 0, "{restore_envelope}");
    assert_eq!(restore_envelope["command"], "codebase-restore");

    let (head_code, head) = call(
        root.path(),
        &["--format", "json", "codebase", "head", "--store", restored],
    );
    assert_eq!(head_code, 0, "{head}");
    assert_eq!(head["payload"]["revision"], revision);
}

#[test]
fn separate_publisher_processes_compare_the_same_expected_head_atomically() {
    use std::process::Stdio;
    let root = demo("codebase-process-cas");
    // System temporary directories can have linked ancestors (macOS /var).
    // Match `call` so both children reach publication through a confined root.
    let workspace = root
        .path()
        .canonicalize()
        .expect("canonical temporary workspace");
    init(root.path());
    let imported = successful_import(root.path(), "absent");
    let expected = imported["revision"].as_str().expect("initial revision");
    root.write("src/app/main.vib", "(defn main () void (do 1u32 void))\n");
    let spawn = || {
        crate::support::vibra()
            .args([
                "--workspace",
                workspace.to_str().expect("UTF-8 fixture"),
                "--format",
                "json",
                "codebase",
                "import",
                "--expect",
                expected,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn publisher")
    };
    let left = spawn();
    let right = spawn();
    let outputs = [
        left.wait_with_output().expect("left publisher"),
        right.wait_with_output().expect("right publisher"),
    ];
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1,
        "publisher outputs: {outputs:?}"
    );
    for output in outputs {
        let envelope = json(&output);
        assert_envelope_schema(&envelope);
        if output.status.success() {
            assert_eq!(envelope["payload"]["published"], true);
        } else {
            assert_eq!(output.status.code(), Some(3), "{envelope}");
            let code = envelope["diagnostics"][0]["code"]
                .as_str()
                .expect("diagnostic");
            assert!(
                matches!(code, "@codebase.stale-head" | "@store.busy"),
                "{envelope}"
            );
            if code == "@store.busy" {
                let (status, retried) = call(
                    root.path(),
                    &[
                        "--format", "json", "codebase", "import", "--expect", expected,
                    ],
                );
                assert_eq!(status, 3, "{retried}");
                assert_eq!(retried["diagnostics"][0]["code"], "@codebase.stale-head");
            }
        }
    }
    let (status, verified) =
        call(root.path(), &["--format", "json", "codebase", "verify"]);
    assert_eq!(status, 0, "{verified}");
    assert_eq!(verified["payload"]["revisionCount"], 2);
}

#[test]
fn head_names_accept_digit_endings_and_reject_digit_prefixes() {
    let root = demo("codebase-head-grammar");
    init(root.path());
    let (status, absent) = call(
        root.path(),
        &["--format", "json", "codebase", "head", "release-2"],
    );
    assert_eq!(status, 3, "{absent}");
    assert_eq!(absent["diagnostics"][0]["code"], "@codebase.unknown-head");
    let (status, invalid) = call(
        root.path(),
        &["--format", "json", "codebase", "head", "2release"],
    );
    assert_eq!(status, 3, "{invalid}");
    assert_eq!(invalid["diagnostics"][0]["code"], "@codebase.invalid-input");
}

#[test]
fn malformed_inner_entity_values_are_invalid_before_revision_lookup() {
    let root = demo("codebase-selector-shape");
    init(root.path());
    let revision =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    for selector in [
        "(record kind: @declaration identity: \"not-an-identity\" entity-kind: @function)",
        "(record kind: @implementation receiver: \"garbage\" interface: \"garbage\")",
        "(record kind: @test module: \"../outside\" name: \"test\")",
        "(record kind: @declaration identity: \"@demo@0.1.0/tests.main.test\" entity-kind: @test)",
    ] {
        let (status, envelope) = call(
            root.path(),
            &[
                "--format",
                "json",
                "codebase",
                "lookup",
                "--revision",
                revision,
                selector,
            ],
        );
        assert_eq!(status, 3, "{envelope}");
        assert_eq!(
            envelope["diagnostics"][0]["code"],
            "@codebase.invalid-input"
        );
        assert_eq!(envelope["payload"], serde_json::json!({}));
    }
}

#[test]
fn rejected_source_import_reports_exit_one_and_preserves_the_published_head() {
    let root = demo("codebase-source-rejection");
    init(root.path());
    let first = successful_import(root.path(), "absent");
    let revision = first["revision"].as_str().expect("published revision");
    let invalid = "(defn main () void 1u32)\n";
    root.write("src/app/main.vib", invalid);
    let (status, rejected) = call(
        root.path(),
        &[
            "--format", "json", "codebase", "import", "--expect", revision,
        ],
    );
    assert_eq!(status, 1, "{rejected}");
    assert_eq!(rejected["result"], "@command.diagnostics");
    assert_eq!(rejected["payload"], serde_json::json!({}));
    assert!(
        rejected["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diagnostic| {
                let span = &diagnostic["primarySpan"];
                let start = span["start"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok());
                let end = span["end"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok());
                span["sourceId"] == "src/app/main.vib"
                    && start.zip(end).is_some_and(|(start, end)| {
                        start < end
                            && invalid
                                .get(start..end)
                                .is_some_and(|text| text.contains("1u32"))
                    })
            }),
        "{rejected}"
    );
    let (status, head) = call(root.path(), &["--format", "json", "codebase", "head"]);
    assert_eq!(status, 0, "{head}");
    assert_eq!(head["payload"]["revision"], revision);
    let (status, report) =
        call(root.path(), &["--format", "json", "codebase", "verify"]);
    assert_eq!(status, 0, "{report}");
    assert_eq!(report["payload"]["revisionCount"], 1);
}
