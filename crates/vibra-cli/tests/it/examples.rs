//! The example projects of the M1 exit: `examples/gate-pure`, the pure
//! modules of the release gate program, and `examples/pilot-pure`, the
//! reference solutions and negative controls of the pilot's pure tasks.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::support::{LIBRARY, TempDir, envelope_in};

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

/// `check` and `test` in `directory` both pass with no diagnostic.
fn checks_and_tests(directory: &Path) {
    let (code, document) = envelope_in(directory, &["--format", "json", "check"]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["status"], "passed");
    assert_eq!(document["diagnostics"], json!([]));
    let (code, tests) = envelope_in(directory, &["--format", "json", "test"]);
    assert_eq!(code, 0, "{tests}");
    assert_eq!(tests["status"], "passed");
}

/// Every file under `directory`, as a path relative to it with `/`.
fn files(directory: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("an example directory reads") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path.strip_prefix(directory).expect("under the example");
                found.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

fn codes(document: &Value) -> Vec<String> {
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().expect("code").to_owned())
        .collect()
}

#[test]
fn gate_pure_tests() {
    checks_and_tests(&example("gate-pure"));
}

#[test]
fn gate_pure_format() {
    for name in ["gate-pure", "pilot-pure"] {
        let directory = example(name);
        let files = files(&directory);
        assert!(!files.is_empty(), "`examples/{name}` has files");
        for file in files {
            let (code, document) =
                envelope_in(&directory, &["--format", "json", "fmt", &file]);
            assert_eq!(code, 0, "{name}/{file}: {document}");
            assert_eq!(
                document["payload"]["changed"], false,
                "{name}/{file} is canonical"
            );
        }
    }
}

#[test]
fn pilot_pure_tests() {
    checks_and_tests(&example("pilot-pure"));
}

/// Each negative control, checked as the one module of a library, gets the
/// diagnostic the contract rules give it: a construction or call whose bare
/// guard is false at constants is `@contract.refuted`, one the rules do not
/// establish is `@contract.unproved`, and a function with a bare `requires`
/// passed as a plain `Fn` is `@contract.obligationLost`.
#[test]
fn pilot_controls_rejected() {
    let controls = [
        ("positiveZero.vib", "@contract.refuted"),
        ("positiveUnchecked.vib", "@contract.unproved"),
        ("orderUnguarded.vib", "@contract.unproved"),
        ("orderSwapped.vib", "@contract.unproved"),
        ("orderConstantFalse.vib", "@contract.refuted"),
        ("optionalObligationLost.vib", "@contract.obligationLost"),
    ];
    let directory = example("pilot-pure").join("controls");
    let mut listed: Vec<String> = files(&directory);
    let mut expected: Vec<String> = controls
        .iter()
        .map(|(file, _)| (*file).to_owned())
        .collect();
    listed.sort();
    expected.sort();
    assert_eq!(listed, expected, "every control has its diagnostic here");
    for (file, code) in controls {
        let source =
            std::fs::read_to_string(directory.join(file)).expect("a control reads");
        let root = TempDir::project(
            &format!("pilot-control-{file}"),
            LIBRARY,
            &[("src/app/main.vib", &source)],
        );
        let (exit, document) = envelope_in(root.path(), &["--format", "json", "check"]);
        assert_eq!(exit, 1, "{file}: {document}");
        assert_eq!(codes(&document), [code], "{file}");
    }
}
