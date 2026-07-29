use std::path::{Path, PathBuf};
use std::process::Command;

fn vibra_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vibra"))
}

fn path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn write_project(root: &Path) -> PathBuf {
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("project.vibra"),
        "(project\n  (package \"flags\" \"0.1.0\")\n  (target flags kind: bin root: \"src\" entry: \"main.vibra\"))\n",
    )
    .unwrap();
    let entry = root.join("src/main.vibra");
    std::fs::write(&entry, "(fn main () void (do (let value 1)))\n").unwrap();
    std::fs::write(
        root.join("src/main.release.vibra"),
        "(fn enabled () void (do (let value 1)) doc: \"Enabled only in release mode\")\n",
    )
    .unwrap();
    entry
}

#[test]
fn run_expand_and_docs_receive_repeatable_compilation_flags() {
    let root = tempfile::tempdir().unwrap();
    let entry = write_project(root.path());

    let run = vibra_cmd()
        .args(["run", &path(&entry), "--flag", "release"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let expanded = vibra_cmd()
        .args([
            "expand",
            &path(&entry),
            "--flag",
            "platform-x",
            "--flag",
            "release",
        ])
        .output()
        .unwrap();
    assert!(expanded.status.success());
    let expanded: serde_json::Value = serde_json::from_slice(&expanded.stdout).unwrap();
    assert!(expanded.get("enabled").is_some(), "{expanded}");

    let docs = vibra_cmd()
        .args(["docs", &path(&entry), "enabled", "--flag", "release"])
        .output()
        .unwrap();
    assert!(docs.status.success());
    assert!(String::from_utf8_lossy(&docs.stdout).contains("Enabled only"));
}

#[test]
fn expand_command_does_not_route_through_the_legacy_value_loader() {
    let source = include_str!("../src/main.rs");
    assert!(
        !source.contains(
            "Command::Expand { path, format, flag } => {\n            let loaded = load::load_legacy_yaml_program"
        ),
        "expand must render the typed frontend directly"
    );
}

#[test]
fn check_and_effects_compile_only_the_selected_parts() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    let entry = root.path().join("src/main.vibra");
    std::fs::write(
        root.path().join("project.vibra"),
        "(project\n  (package \"flags\" \"0.1.0\")\n  (target flags kind: bin root: \"src\" entry: \"main.vibra\"))\n",
    )
    .unwrap();
    std::fs::write(&entry, "(fn main () void (do (let value 1)))\n").unwrap();
    std::fs::write(
        root.path().join("src/main.broken.vibra"),
        "(import missing \"absent.vibra\")\n",
    )
    .unwrap();

    assert!(vibra_cmd()
        .args(["check", &path(root.path())])
        .status()
        .unwrap()
        .success());
    let checked = vibra_cmd()
        .args(["check", &path(root.path()), "--flag", "broken"])
        .output()
        .unwrap();
    assert!(!checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stderr).contains("absent.vibra"));

    assert!(vibra_cmd()
        .args(["effects", &path(&entry)])
        .status()
        .unwrap()
        .success());
    let effects = vibra_cmd()
        .args(["effects", &path(&entry), "--flag", "broken"])
        .output()
        .unwrap();
    assert!(!effects.status.success());
    assert!(String::from_utf8_lossy(&effects.stderr).contains("absent.vibra"));
}

#[test]
fn package_metadata_and_bytes_identify_flags_and_selected_sources() {
    let root = tempfile::tempdir().unwrap();
    write_project(root.path());
    let plain = root.path().join("plain.vapp");
    let release = root.path().join("release.vapp");

    let plain_build = vibra_cmd()
        .args(["build", &path(root.path()), "--output", &path(&plain)])
        .output()
        .unwrap();
    assert!(
        plain_build.status.success(),
        "{}",
        String::from_utf8_lossy(&plain_build.stderr)
    );
    let release_build = vibra_cmd()
        .args([
            "build",
            &path(root.path()),
            "--output",
            &path(&release),
            "--flag",
            "release",
        ])
        .output()
        .unwrap();
    assert!(
        release_build.status.success(),
        "{}",
        String::from_utf8_lossy(&release_build.stderr)
    );
    assert_ne!(
        std::fs::read(&plain).unwrap(),
        std::fs::read(&release).unwrap()
    );

    let inspect = vibra_cmd()
        .args(["package", "inspect", &path(&release), "--format", "json"])
        .output()
        .unwrap();
    assert!(inspect.status.success());
    let metadata: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(
        metadata["compilation-flags"],
        serde_json::json!(["release"])
    );
    assert!(metadata["selected-sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source == "source/src/main.release.vibra"));

    let run = vibra_cmd().args(["run", &path(&release)]).output().unwrap();
    assert!(
        run.status.success(),
        "packaged run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn malformed_flags_and_conditional_suffixes_have_stable_diagnostics() {
    let root = tempfile::tempdir().unwrap();
    let entry = write_project(root.path());
    let invalid = vibra_cmd()
        .args(["expand", &path(&entry), "--flag", "Bad..flag"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("E-FLAG-001"));

    std::fs::write(root.path().join("src/main..debug.vibra"), "extra: 1\n").unwrap();
    let malformed = vibra_cmd()
        .args(["expand", &path(&entry)])
        .output()
        .unwrap();
    assert!(!malformed.status.success());
    assert!(String::from_utf8_lossy(&malformed.stderr).contains("E-FLAG-002"));
}
